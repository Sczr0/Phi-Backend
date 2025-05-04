use crate::models::player_archive::{
    PlayerArchive, ChartScore, ChartScoreHistory, ArchiveConfig, PlayerBasicInfo, RKSRankingEntry
};
use crate::models::rks::RksRecord;
use crate::utils::error::AppError;
use chrono::{DateTime, Utc};
use sqlx::{SqlitePool, query, query_as};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use log;
use sqlx::Row;

#[derive(Clone)]
pub struct PlayerArchiveService {
    pool: SqlitePool,
    config: ArchiveConfig,
    // 为频繁访问的数据添加内存缓存
    cache: Arc<Mutex<HashMap<String, (PlayerArchive, DateTime<Utc>)>>>,
}

impl PlayerArchiveService {
    pub fn new(pool: SqlitePool, config: Option<ArchiveConfig>) -> Self {
        Self {
            pool,
            config: config.unwrap_or_default(),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取玩家存档
    pub async fn get_player_archive(&self, player_id: &str) -> Result<Option<PlayerArchive>, AppError> {
        // 检查缓存
        {
            let cache = self.cache.lock().unwrap();
            if let Some((archive, timestamp)) = cache.get(player_id) {
                // 如果缓存不超过5分钟，直接返回
                let now = Utc::now();
                if (now - *timestamp).num_seconds() < 300 {
                    log::debug!("从缓存获取玩家[{}]存档", player_id);
                    return Ok(Some(archive.clone()));
                }
            }
        }
        
        // 查询玩家基本信息
        let player = query_as::<_, PlayerBasicInfo>(
            "SELECT player_id, player_name, rks, update_time FROM player_archives WHERE player_id = ?"
        )
        .bind(player_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询玩家失败: {}", e)))?;
        
        let player = match player {
            Some(p) => p,
            None => {
                log::debug!("玩家[{}]不存在", player_id);
                return Ok(None);
            }
        };
        
        // 查询玩家所有当前成绩
        let scores = query_as::<_, DbChartScore>(
            "SELECT song_id, song_name, difficulty, difficulty_value, score, acc, rks, is_fc, is_phi, play_time 
             FROM chart_scores 
             WHERE player_id = ? AND is_current = 1"
        )
        .bind(player_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询成绩失败: {}", e)))?;
        
        let mut best_scores = HashMap::new();
        let mut all_scores = Vec::new();
        
        for db_score in scores {
            let score = ChartScore {
                song_id: db_score.song_id,
                song_name: db_score.song_name,
                difficulty: db_score.difficulty,
                difficulty_value: db_score.difficulty_value,
                score: db_score.score,
                acc: db_score.acc,
                rks: db_score.rks,
                is_fc: db_score.is_fc != 0,
                is_phi: db_score.is_phi != 0,
                play_time: db_score.play_time,
            };
            
            let key = format!("{}-{}", score.song_id, score.difficulty);
            best_scores.insert(key, score.clone());
            all_scores.push(score);
        }
        
        // 获取BestN成绩
        let mut best_n_scores = all_scores.clone();
        best_n_scores.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
        best_n_scores.truncate(self.config.best_n_count as usize);
        
        // 查询成绩历史记录
        let mut chart_histories = HashMap::new();
        
        let histories = query_as::<_, DbChartScoreHistory>(
            "SELECT song_id, difficulty, score, acc, rks, is_fc, is_phi, play_time 
             FROM chart_scores 
             WHERE player_id = ? 
             ORDER BY song_id, difficulty, play_time DESC"
        )
        .bind(player_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询历史记录失败: {}", e)))?;
        
        for db_history in histories {
            let key = format!("{}-{}", db_history.song_id, db_history.difficulty);
            let history = ChartScoreHistory {
                score: db_history.score,
                acc: db_history.acc,
                rks: db_history.rks,
                is_fc: db_history.is_fc != 0,
                is_phi: db_history.is_phi != 0,
                play_time: db_history.play_time,
            };
            
            chart_histories.entry(key).or_insert_with(Vec::new).push(history);
        }
        
        // 为每个谱面按时间倒序排序并限制数量
        for histories in chart_histories.values_mut() {
            histories.sort_by(|a, b| b.play_time.cmp(&a.play_time));
            if self.config.history_max_records > 0 && histories.len() > self.config.history_max_records {
                histories.truncate(self.config.history_max_records);
            }
        }
        
        // 获取推分ACC (如果配置启用)
        let push_acc_map = if self.config.store_push_acc {
            let push_accs = query!(
                "SELECT song_id, difficulty, push_acc FROM push_acc WHERE player_id = ?",
                player_id
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("查询推分ACC失败: {}", e)))?;
            
            let mut map = HashMap::new();
            for record in push_accs {
                let key = format!("{}-{}", record.song_id, record.difficulty);
                map.insert(key, record.push_acc);
            }
            
            if !map.is_empty() {
                Some(map)
            } else {
                None
            }
        } else {
            None
        };
        
        let archive = PlayerArchive {
            player_id: player.player_id,
            player_name: player.player_name,
            rks: player.rks,
            update_time: player.update_time,
            best_scores,
            best_n_scores,
            chart_histories,
            push_acc_map,
        };
        
        // 更新缓存
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(archive.player_id.clone(), (archive.clone(), Utc::now()));
        }
        
        Ok(Some(archive))
    }
    
    /// 更新玩家成绩
    pub async fn update_player_score(&self, player_id: &str, player_name: &str, score: ChartScore) -> Result<(), AppError> {
        log::info!(
            "更新玩家[{}]成绩: 歌曲={}, 难度={}, ACC={:.2}%, RKS={:.2}", 
            player_id, score.song_id, score.difficulty, score.acc, score.rks
        );
        
        // 开始事务
        let mut tx = self.pool.begin().await
            .map_err(|e| AppError::DatabaseError(format!("开始事务失败: {}", e)))?;
        
        // 更新或插入玩家信息
        let update_time_str = Utc::now().to_rfc3339();
        query!(
            "INSERT OR REPLACE INTO player_archives (player_id, player_name, rks, update_time) VALUES (?, ?, ?, ?)",
            player_id,
            player_name,
            0.0, // RKS将在后续计算并更新
            update_time_str,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("更新玩家失败: {}", e)))?;
        
        // 将当前成绩设为非当前
        query!(
            "UPDATE chart_scores SET is_current = 0 WHERE player_id = ? AND song_id = ? AND difficulty = ?",
            player_id, score.song_id, score.difficulty
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("更新旧成绩状态失败: {}", e)))?;
        
        // 插入新成绩
        let is_fc_i32 = score.is_fc as i32;
        let is_phi_i32 = score.is_phi as i32;
        let play_time_str = score.play_time.to_rfc3339();
        query!(
            "INSERT INTO chart_scores (
                player_id, song_id, song_name, difficulty, difficulty_value, 
                score, acc, rks, is_fc, is_phi, play_time, is_current
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
            player_id, 
            score.song_id, 
            score.song_name, 
            score.difficulty, 
            score.difficulty_value,
            score.score, 
            score.acc, 
            score.rks, 
            is_fc_i32,
            is_phi_i32,
            play_time_str,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("插入新成绩失败: {}", e)))?;
        
        // 提交事务
        tx.commit().await
            .map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
        
        // 清除缓存
        {
            let mut cache = self.cache.lock().unwrap();
            cache.remove(player_id);
        }
        
        // 计算并更新玩家RKS
        self.recalculate_player_rks(player_id).await?;
        
        // 如果配置了存储推分ACC，计算并更新
        if self.config.store_push_acc {
            self.recalculate_push_acc(player_id).await?;
        }
        
        Ok(())
    }
    
    /// 计算玩家RKS
    pub async fn recalculate_player_rks(&self, player_id: &str) -> Result<f64, AppError> {
        log::info!("重新计算玩家[{}]RKS", player_id);
        
        // 获取玩家所有当前成绩，并按RKS降序排序
        let scores = query!(
            "SELECT rks FROM chart_scores 
             WHERE player_id = ? AND is_current = 1 
             ORDER BY rks DESC",
            player_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询成绩RKS失败: {}", e)))?;
        
        let mut rks_values: Vec<f64> = Vec::new();
        for record in scores {
            rks_values.push(record.rks);
        }
        
        // 获取AP成绩
        let ap_scores = query!(
            "SELECT rks FROM chart_scores 
             WHERE player_id = ? AND is_current = 1 AND acc >= 100.0
             ORDER BY rks DESC",
            player_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询AP成绩失败: {}", e)))?;
        
        let mut ap_rks_values: Vec<f64> = Vec::new();
        for record in ap_scores {
            ap_rks_values.push(record.rks);
        }
        
        // 计算Best N和AP Top 3的RKS
        let best_n_count = self.config.best_n_count as usize;
        let best_n_sum: f64 = rks_values.iter().take(best_n_count.min(rks_values.len())).sum();
        let best_n_avg = if rks_values.len() >= best_n_count {
            best_n_sum / best_n_count as f64
        } else if !rks_values.is_empty() {
            best_n_sum / rks_values.len() as f64
        } else {
            0.0
        };
        
        let ap_count = ap_rks_values.len().min(3);
        let ap_sum: f64 = ap_rks_values.iter().take(ap_count).sum();
        let ap_avg = if ap_count > 0 {
            ap_sum / ap_count as f64
        } else {
            0.0
        };
        
        // 根据AP数量计算最终RKS
        let final_rks = match ap_count {
            0 => best_n_avg, // 无AP成绩
            1 => best_n_avg * 5.0 / 6.0 + ap_avg / 6.0, // 1个AP成绩
            2 => best_n_avg * 2.0 / 3.0 + ap_avg / 3.0, // 2个AP成绩
            _ => best_n_avg * 0.5 + ap_avg * 0.5, // 3个或更多AP成绩
        };
        
        log::info!(
            "玩家[{}]RKS计算: Best{}平均={:.4}, AP Top {}平均={:.4}, 最终RKS={:.4}", 
            player_id, best_n_count, best_n_avg, ap_count, ap_avg, final_rks
        );
        
        // 更新玩家RKS
        let update_time_str = Utc::now().to_rfc3339();
        query!(
            "UPDATE player_archives SET rks = ?, update_time = ? WHERE player_id = ?",
            final_rks, update_time_str, player_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("更新玩家RKS失败: {}", e)))?;
        
        Ok(final_rks)
    }
    
    /// (优化后) 从RKS记录批量更新玩家成绩，并在最后计算RKS和推分ACC
    pub async fn update_player_scores_from_rks_records(
        &self, 
        player_id: &str, 
        player_name: &str,
        rks_records: &Vec<RksRecord>,
        fc_map: &HashMap<String, bool>,
    ) -> Result<(), AppError> {
        log::info!("批量更新玩家[{}] ({}) 的成绩, 共{}条记录", player_id, player_name, rks_records.len());
        
        if rks_records.is_empty() {
            log::warn!("RKS记录为空，无需更新玩家[{}] ({}) 的成绩", player_id, player_name);
            // Even if no records, update the player_archives table timestamp if needed, or ensure the player exists
            let mut tx = self.pool.begin().await
                .map_err(|e| AppError::DatabaseError(format!("开始事务失败: {}", e)))?;
            let update_time_str = Utc::now().to_rfc3339();
            query!(
                "INSERT INTO player_archives (player_id, player_name, rks, update_time) VALUES (?, ?, ?, ?) 
                 ON CONFLICT(player_id) DO UPDATE SET player_name = excluded.player_name, update_time = excluded.update_time",
                player_id,
                player_name,
                0.0, // Default RKS, will be recalculated if scores exist
                update_time_str,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("更新玩家信息失败: {}", e)))?;
            tx.commit().await.map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
            return Ok(());
        }
        
        // 开始事务
        let mut tx = self.pool.begin().await
            .map_err(|e| AppError::DatabaseError(format!("开始事务失败: {}", e)))?;
            
        // 1. 更新或插入玩家信息
        let update_time = Utc::now();
        let update_time_str = update_time.to_rfc3339();
        query!(
             "INSERT INTO player_archives (player_id, player_name, rks, update_time) VALUES (?, ?, ?, ?) 
              ON CONFLICT(player_id) DO UPDATE SET player_name = excluded.player_name, update_time = excluded.update_time",
             player_id,
             player_name,
             0.0, // RKS将在后面重新计算
             update_time_str,
         )
         .execute(&mut *tx)
         .await
         .map_err(|e| AppError::DatabaseError(format!("更新玩家信息失败: {}", e)))?;

        // 2. 将该玩家所有谱面的 is_current 设为 0
        // 这样做比逐个更新更高效，即使某些谱面没有新记录
        query!(
            "UPDATE chart_scores SET is_current = 0 WHERE player_id = ?",
            player_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(format!("重置旧成绩状态失败: {}", e)))?;

        // 3. 逐条插入新的当前成绩 (移除 UNNEST 尝试)
        log::debug!("开始逐条插入 {} 条新成绩记录...", rks_records.len());
        for record in rks_records {
            let key = format!("{}-{}", record.song_id, record.difficulty);
            let is_fc = fc_map.get(&key).copied().unwrap_or(false);
            let is_phi = record.acc >= 100.0;
            let is_fc_i32 = is_fc as i32;
            let is_phi_i32 = is_phi as i32;
            let play_time_str = update_time.to_rfc3339(); // Use the same transaction start time
            let score_value = record.score.unwrap_or(0.0); // Store score in a variable

            query!(
                "INSERT INTO chart_scores (
                    player_id, song_id, song_name, difficulty, difficulty_value, 
                    score, acc, rks, is_fc, is_phi, play_time, is_current
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
                player_id, 
                record.song_id, 
                record.song_name, 
                record.difficulty, 
                record.difficulty_value,
                score_value, // Use the variable here
                record.acc, 
                record.rks, 
                is_fc_i32,
                is_phi_i32,
                play_time_str,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e_inner| AppError::DatabaseError(format!("插入单条成绩失败 for {}-{}: {}", record.song_id, record.difficulty, e_inner)))?;
        }
        log::debug!("逐条插入完成");
        
        // 提交事务
        tx.commit().await
            .map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
        
        // 4. 在所有数据库操作完成后，计算并更新玩家RKS
        log::info!("成绩批量更新完成，开始重新计算玩家[{}] ({}) 的 RKS...", player_id, player_name);
        match self.recalculate_player_rks(player_id).await {
             Ok(new_rks) => log::info!("玩家[{}] ({}) RKS 更新为: {:.4}", player_id, player_name, new_rks),
             Err(e) => log::error!("重新计算玩家[{}] ({}) RKS 失败: {}", player_id, player_name, e),
             // Continue even if RKS recalculation fails
        }
        
        // 5. 计算并更新推分ACC (如果启用)
        if self.config.store_push_acc {
             log::info!("开始重新计算玩家[{}] ({}) 的推分 ACC...", player_id, player_name);
             match self.recalculate_push_acc(player_id).await {
                 Ok(_) => log::info!("玩家[{}] ({}) 推分 ACC 更新完成", player_id, player_name),
                 Err(e) => log::error!("重新计算玩家[{}] ({}) 推分 ACC 失败: {}", player_id, player_name, e),
                 // Continue even if push acc recalculation fails
             }
        }

        // 清除缓存
        {
            let mut cache = self.cache.lock().unwrap();
            cache.remove(player_id);
            log::debug!("玩家[{}] ({}) 缓存已清除", player_id, player_name);
        }
        
        Ok(())
    }
    
    /// 计算并更新推分ACC
    pub async fn recalculate_push_acc(&self, player_id: &str) -> Result<(), AppError> {
        use crate::services::image_service::calculate_target_chart_push_acc;
        log::info!("重新计算玩家[{}]推分ACC", player_id);
        
        // 获取玩家存档
        let _archive = self.get_player_archive(player_id).await?
            .ok_or_else(|| AppError::DatabaseError(format!("玩家不存在: {}", player_id)))?;
        
        // 获取所有当前成绩用于推分计算
        let all_scores = query_as::<_, DbChartScore>(
            "SELECT song_id, song_name, difficulty, difficulty_value, score, acc, rks, is_fc, is_phi, play_time 
             FROM chart_scores 
             WHERE player_id = ? AND is_current = 1"
        )
        .bind(player_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取成绩记录失败: {}", e)))?;
        
        // 将数据库记录转换为RksRecord用于推分计算
        let rks_records: Vec<RksRecord> = all_scores.iter().map(|s| {
            RksRecord {
                song_id: s.song_id.clone(),
                song_name: s.song_name.clone(),
                difficulty: s.difficulty.clone(),
                difficulty_value: s.difficulty_value,
                acc: s.acc,
                score: Some(s.score),
                rks: s.rks,
            }
        }).collect();
        
        // 按RKS排序
        let mut sorted_records = rks_records.clone();
        sorted_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
        
        // 开始事务
        let mut tx = self.pool.begin().await
            .map_err(|e| AppError::DatabaseError(format!("开始事务失败: {}", e)))?;
        
        // 清除旧的推分ACC记录
        query!("DELETE FROM push_acc WHERE player_id = ?", player_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("清除旧推分ACC记录失败: {}", e)))?;
        
        // 计算每个谱面的推分ACC
        let mut push_acc_count = 0;
        for score in &all_scores {
            // 已经是AP成绩或定数为0，不需要计算推分
            if score.acc >= 100.0 || score.difficulty_value <= 0.0 {
                continue;
            }
            
            let target_chart_id = format!("{}-{}", score.song_id, score.difficulty);
            
            // 计算推分ACC
            if let Some(push_acc) = calculate_target_chart_push_acc(&target_chart_id, score.difficulty_value, &sorted_records) {
                // 如果计算出来的推分ACC低于当前ACC或等于100%，则没有实际意义
                if push_acc <= score.acc {
                    continue;
                }
                
                // 插入推分ACC记录
                let update_time_str = Utc::now().to_rfc3339();
                query!(
                    "INSERT INTO push_acc (player_id, song_id, difficulty, push_acc, update_time) 
                     VALUES (?, ?, ?, ?, ?)",
                    player_id, score.song_id, score.difficulty, push_acc, update_time_str
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::DatabaseError(format!("插入推分ACC记录失败: {}", e)))?;
                
                push_acc_count += 1;
            }
        }
        
        // 提交事务
        tx.commit().await
            .map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
        
        log::info!("计算完成，玩家[{}]共有{}个谱面可推分", player_id, push_acc_count);
        
        // 清除缓存
        {
            let mut cache = self.cache.lock().unwrap();
            cache.remove(player_id);
        }
        
        Ok(())
    }
    
    // 删除玩家存档
    pub async fn delete_player_archive(&self, player_id: &str) -> Result<(), AppError> {
        log::info!("删除玩家[{}]存档", player_id);
        
        let mut tx = self.pool.begin().await
            .map_err(|e| AppError::DatabaseError(format!("开始事务失败: {}", e)))?;
        
        // 删除玩家信息
        query!("DELETE FROM player_archives WHERE player_id = ?", player_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("删除玩家信息失败: {}", e)))?;
        
        // 删除玩家成绩
        query!("DELETE FROM chart_scores WHERE player_id = ?", player_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("删除玩家成绩失败: {}", e)))?;
        
        // 删除推分ACC
        query!("DELETE FROM push_acc WHERE player_id = ?", player_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("删除推分ACC记录失败: {}", e)))?;
        
        tx.commit().await
            .map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
        
        // 清除缓存
        {
            let mut cache = self.cache.lock().unwrap();
            cache.remove(player_id);
        }
        
        Ok(())
    }

    /// 获取RKS排行榜数据
    pub async fn get_rks_ranking(&self, limit: usize) -> Result<Vec<RKSRankingEntry>, AppError> {
        log::info!("获取RKS排行榜，显示前{}名玩家", limit);

        let rows = sqlx::query(
            "SELECT player_id, player_name, rks, update_time 
             FROM player_archives 
             ORDER BY rks DESC 
             LIMIT ?"
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取基础排行榜数据失败: {}", e)))?;

        let mut ranking_entries = Vec::with_capacity(rows.len());
        for row in rows {
            let player_id: String = row.try_get("player_id")
                .map_err(|e| AppError::DatabaseError(format!("获取 player_id 失败: {}", e)))?;
            let player_name: String = row.try_get("player_name")
                .map_err(|e| AppError::DatabaseError(format!("获取 player_name 失败: {}", e)))?;
            let rks: f64 = row.try_get("rks")
                .map_err(|e| AppError::DatabaseError(format!("获取 rks 失败: {}", e)))?;
            let update_time_str: String = row.try_get("update_time")
                .map_err(|e| AppError::DatabaseError(format!("获取 update_time 失败: {}", e)))?;

            let update_time = DateTime::parse_from_rfc3339(&update_time_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| AppError::InternalError(format!("解析排行榜更新时间失败 ({}): {}", player_id, e)))?;

            ranking_entries.push(RKSRankingEntry {
                player_id,
                player_name,
                rks,
                update_time,
                b27_rks: None,
                ap3_rks: None,
                ap_count: None,
            });
        }

        log::debug!("成功转换{}条排行榜数据", ranking_entries.len());

        Ok(ranking_entries)
    }

    pub async fn get_player_best_scores(
        &self,
        player_id: &str,
        _n: Option<usize>,
    ) -> Result<Vec<ChartScore>, AppError> {
        let _archive = self.get_player_archive(player_id).await?;
        // ... rest of the function ...
        let scores = query_as::<_, DbChartScore>(
            "SELECT song_id, song_name, difficulty, difficulty_value, score, acc, rks, is_fc, is_phi, play_time 
             FROM chart_scores 
             WHERE player_id = ? AND is_current = 1"
        )
        .bind(player_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询成绩失败: {}", e)))?;

        let result_scores = scores.into_iter().map(|db_score| ChartScore {
            song_id: db_score.song_id,
            song_name: db_score.song_name,
            difficulty: db_score.difficulty,
            difficulty_value: db_score.difficulty_value,
            score: db_score.score,
            acc: db_score.acc,
            rks: db_score.rks,
            is_fc: db_score.is_fc != 0,
            is_phi: db_score.is_phi != 0,
            play_time: db_score.play_time,
        }).collect();
        
        Ok(result_scores)
    }
}

// 数据库模型，用于从数据库查询结果映射
#[derive(sqlx::FromRow)]
struct DbChartScore {
    song_id: String,
    song_name: String,
    difficulty: String,
    difficulty_value: f64,
    score: f64,
    acc: f64,
    rks: f64,
    is_fc: i32,
    is_phi: i32,
    play_time: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct DbChartScoreHistory {
    song_id: String,
    difficulty: String,
    score: f64,
    acc: f64,
    rks: f64,
    is_fc: i32,
    is_phi: i32,
    play_time: DateTime<Utc>,
}

// Temporary struct for querying basic ranking info
#[derive(sqlx::FromRow, Debug)]
struct BasicRankingInfo {
    player_id: String,
    player_name: String,
    rks: f64,
    update_time: String, // Query as String first
}

// 使用示例
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    
    /// 示例：如何使用玩家存档服务
    #[tokio::test]
    async fn test_player_archive_service() {
        // 使用内存数据库进行测试
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to connect to in-memory database");
        
        // 创建玩家存档服务并初始化表
        let config = ArchiveConfig {
            store_push_acc: true,
            best_n_count: 27,
            history_max_records: 10,
        };
        let service = PlayerArchiveService::new(pool, Some(config));
        service.init_tables().await.expect("Failed to initialize tables");
        
        // 创建一些测试数据
        let player_id = "test_player_123";
        let player_name = "测试玩家";
        
        // 创建一个成绩
        let score = ChartScore {
            song_id: "song123".to_string(),
            song_name: "测试歌曲".to_string(),
            difficulty: "IN".to_string(),
            difficulty_value: 15.5,
            score: 999300.0,
            acc: 98.3,
            rks: 14.72,
            is_fc: true,
            is_phi: false,
            play_time: Utc::now(),
        };
        
        // 更新玩家成绩
        service.update_player_score(player_id, player_name, score).await.expect("Failed to update score");
        
        // 获取玩家存档
        let archive = service.get_player_archive(player_id).await.expect("Failed to get archive");
        
        if let Some(archive) = archive {
            println!("玩家RKS: {:.2}", archive.rks);
            println!("最佳成绩数量: {}", archive.best_scores.len());
            
            // 使用推分ACC信息
            if let Some(push_acc_map) = &archive.push_acc_map {
                for (key, acc) in push_acc_map {
                    println!("{} 需要提升至 {:.2}%", key, acc);
                }
            }
        }
        
        // 删除玩家存档
        service.delete_player_archive(player_id).await.expect("Failed to delete archive");
    }
} 