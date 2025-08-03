use crate::models::player_archive::{
    PlayerArchive, ChartScore, ChartScoreHistory, ArchiveConfig, RKSRankingEntry,
};
use crate::models::rks::RksRecord;
use crate::utils::error::AppError;
use chrono::{DateTime, Utc};
use sqlx::{SqlitePool, query, query_as};
use std::collections::HashMap;
use std::sync::Arc;
use log;
use sqlx::Row;
use moka::future::Cache;
use std::time::Duration;

#[derive(Clone)]
pub struct PlayerArchiveService {
    pool: SqlitePool,
    config: ArchiveConfig,
    // 使用 moka 作为高性能并发缓存
    cache: Cache<String, Arc<PlayerArchive>>,
}

impl PlayerArchiveService {
    pub fn new(pool: SqlitePool, config: Option<ArchiveConfig>) -> Self {
        // 初始化 moka 缓存
        // - 设置最大容量为 1000 个条目
        // - 设置生存时间 (TTL) 为 5 分钟
        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(300))
            .build();

        Self {
            pool,
            config: config.unwrap_or_default(),
            cache,
        }
    }

    /// 获取玩家存档 (已重构)
    /// - 使用 moka 缓存，自动处理过期。
    /// - 将多个数据库查询合并为一个，解决 N+1 问题。
    /// - 使用窗口函数在数据库端直接筛选历史记录。
    pub async fn get_player_archive(&self, player_id: &str) -> Result<Option<PlayerArchive>, AppError> {
        // 1. 检查缓存
        if let Some(archive_arc) = self.cache.get(player_id).await {
            log::debug!("从缓存获取玩家[{}]存档", player_id);
            return Ok(Some(archive_arc.as_ref().clone()));
        }
        
        log::debug!("缓存未命中，从数据库查询玩家[{}]存档", player_id);

        // 2. 核心查询：合并玩家信息、当前成绩和历史成绩
        let history_limit = self.config.history_max_records as i64;
        let query_sql = "
WITH RankedScores AS (
    SELECT 
        *,
        ROW_NUMBER() OVER(PARTITION BY player_id, song_id, difficulty ORDER BY play_time DESC) as history_rank
    FROM chart_scores
    WHERE player_id = ?1
)
SELECT 
    pa.player_id,
    pa.player_name,
    pa.rks,
    pa.update_time,
    rs.song_id,
    rs.song_name,
    rs.difficulty,
    rs.difficulty_value,
    rs.score,
    rs.acc,
    rs.rks as score_rks,
    rs.is_fc,
    rs.is_phi,
    rs.play_time,
    rs.is_current,
    rs.history_rank
FROM player_archives pa
LEFT JOIN RankedScores rs ON pa.player_id = rs.player_id
WHERE pa.player_id = ?1 AND (rs.is_current = 1 OR rs.history_rank <= ?2 OR rs.song_id IS NULL)
ORDER BY rs.play_time DESC;
        ";

        let rows = query_as::<_, CombinedScoreRecord>(query_sql)
            .bind(player_id)
            .bind(history_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("合并查询玩家存档失败: {}", e)))?;

        if rows.is_empty() {
            log::debug!("玩家[{}]不存在", player_id);
            return Ok(None);
        }

        // 3. 在Rust代码中处理结果，组装成 PlayerArchive
        let first_row = &rows[0];
        // 如果第一个结果就没有 song_id，说明玩家存在但没有任何成绩
        if first_row.song_id.is_none() {
            let push_acc_map = self.get_push_acc_map(player_id).await?;
            let archive = PlayerArchive {
                player_id: first_row.player_id.clone(),
                player_name: first_row.player_name.clone(),
                rks: first_row.rks,
                update_time: first_row.update_time,
                best_scores: HashMap::new(),
                best_n_scores: Vec::new(),
                chart_histories: HashMap::new(),
                push_acc_map,
            };
            let arc_archive = Arc::new(archive.clone());
            self.cache.insert(player_id.to_string(), arc_archive).await;
            log::debug!("玩家[{}]存在但无成绩，已存入缓存", player_id);
            return Ok(Some(archive));
        }

        let mut best_scores = HashMap::new();
        let mut all_current_scores = Vec::new();
        let mut chart_histories = HashMap::new();

        for row in &rows {
            let song_id = row.song_id.clone().unwrap();
            let difficulty = row.difficulty.clone().unwrap();
            let key = format!("{}-{}", song_id, difficulty);

            // 处理当前成绩
            if row.is_current.unwrap_or(0) == 1 {
                let score = ChartScore {
                    song_id: song_id.clone(),
                    song_name: row.song_name.clone().unwrap_or_default(),
                    difficulty: difficulty.clone(),
                    difficulty_value: row.difficulty_value.unwrap_or(0.0),
                    score: row.score.unwrap_or(0.0),
                    acc: row.acc.unwrap_or(0.0),
                    rks: row.score_rks.unwrap_or(0.0),
                    is_fc: row.is_fc.unwrap_or(0) != 0,
                    is_phi: row.is_phi.unwrap_or(0) != 0,
                    play_time: row.play_time.unwrap_or_else(Utc::now),
                };
                if !best_scores.contains_key(&key) {
                    best_scores.insert(key.clone(), score.clone());
                    all_current_scores.push(score);
                }
            }

            // 处理历史成绩
            let history = ChartScoreHistory {
                score: row.score.unwrap_or(0.0),
                acc: row.acc.unwrap_or(0.0),
                rks: row.score_rks.unwrap_or(0.0),
                is_fc: row.is_fc.unwrap_or(0) != 0,
                is_phi: row.is_phi.unwrap_or(0) != 0,
                play_time: row.play_time.unwrap_or_else(Utc::now),
            };
            chart_histories.entry(key).or_insert_with(Vec::new).push(history);
        }

        // 获取BestN成绩
        all_current_scores.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
        all_current_scores.truncate(self.config.best_n_count as usize);
        
        // 4. 单独获取推分ACC
        let push_acc_map = self.get_push_acc_map(player_id).await?;

        let archive = PlayerArchive {
            player_id: first_row.player_id.clone(),
            player_name: first_row.player_name.clone(),
            rks: first_row.rks,
            update_time: first_row.update_time,
            best_scores,
            best_n_scores: all_current_scores,
            chart_histories,
            push_acc_map,
        };

        // 5. 更新缓存
        let arc_archive = Arc::new(archive.clone());
        self.cache.insert(player_id.to_string(), arc_archive).await;
        log::debug!("玩家[{}]存档已查询并存入缓存", player_id);

        Ok(Some(archive))
    }
    
    
    /// 计算玩家RKS
    pub async fn recalculate_player_rks(&self, player_id: &str) -> Result<f64, AppError> {
        log::info!("重新计算玩家[{}]RKS", player_id);
        
        // (优化后) 一次查询获取所有当前成绩的rks和acc
        let scores = query!(
            "SELECT rks, acc FROM chart_scores
             WHERE player_id = ? AND is_current = 1
             ORDER BY rks DESC",
            player_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询成绩RKS失败: {}", e)))?;

        let rks_values: Vec<f64> = scores.iter().map(|s| s.rks).collect();
        let ap_rks_values: Vec<f64> = scores.iter()
            .filter(|s| s.acc >= 100.0)
            .map(|s| s.rks)
            .collect();
        
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
    
    /// (已重构) 从RKS记录批量更新玩家成绩。
    /// - 使用事务保证操作的原子性。
    /// - 放弃手动拼接SQL，改用循环执行预处理语句的方式进行批量插入，更安全高效。
    pub async fn update_player_scores_from_rks_records(
        &self, 
        player_id: &str, 
        player_name: &str,
        rks_records: &Vec<RksRecord>,
        fc_map: &HashMap<String, bool>,
    ) -> Result<(), AppError> {
        log::info!("批量更新玩家[{}] ({}) 的成绩, 共{}条记录", player_id, player_name, rks_records.len());
        
        let mut tx = self.pool.begin().await
            .map_err(|e| AppError::DatabaseError(format!("开始事务失败: {}", e)))?;
        
        let update_time = Utc::now();

        // 1. 更新或插入玩家信息
        query!(
             "INSERT INTO player_archives (player_id, player_name, rks, update_time) VALUES (?, ?, ?, ?) 
              ON CONFLICT(player_id) DO UPDATE SET player_name = excluded.player_name, update_time = excluded.update_time",
             player_id,
             player_name,
             0.0, // RKS将在后面重新计算
             update_time,
         )
         .execute(&mut *tx)
         .await
         .map_err(|e| AppError::DatabaseError(format!("更新玩家信息失败: {}", e)))?;

        if rks_records.is_empty() {
            log::warn!("RKS记录为空，仅更新玩家[{}] ({}) 的信息和时间戳", player_id, player_name);
            tx.commit().await.map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
            return Ok(());
        }

        // 2. 将该玩家所有谱面的 is_current 设为 0
        query!("UPDATE chart_scores SET is_current = 0 WHERE player_id = ?", player_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(format!("重置旧成绩状态失败: {}", e)))?;

        // 3. 循环插入新的当前成绩
        log::debug!("开始批量插入 {} 条新成绩记录...", rks_records.len());
        let insert_sql = "
            INSERT INTO chart_scores 
            (player_id, song_id, song_name, difficulty, difficulty_value, score, acc, rks, is_fc, is_phi, play_time, is_current) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
        ";

        for record in rks_records {
            let key = format!("{}-{}", record.song_id, record.difficulty);
            let is_fc = fc_map.get(&key).copied().unwrap_or(false) as i32;
            let is_phi = (record.acc >= 100.0) as i32;
            
            query(insert_sql)
                .bind(player_id)
                .bind(&record.song_id)
                .bind(&record.song_name)
                .bind(&record.difficulty)
                .bind(record.difficulty_value)
                .bind(record.score.unwrap_or(0.0))
                .bind(record.acc)
                .bind(record.rks)
                .bind(is_fc)
                .bind(is_phi)
                .bind(update_time)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::DatabaseError(format!("批量插入成绩失败: song_id={}, {}", record.song_id, e)))?;
        }
        log::debug!("批量插入完成");
        
        // 提交事务
        tx.commit().await
            .map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
        
        // 4. 在所有数据库操作完成后，异步计算并更新玩家RKS和推分ACC
        let self_clone = self.clone();
        let player_id_clone = player_id.to_string();
        let player_name_clone = player_name.to_string();
        tokio::spawn(async move {
            log::info!("成绩批量更新完成，开始异步重新计算玩家[{}] ({}) 的 RKS...", player_id_clone, player_name_clone);
            if let Err(e) = self_clone.recalculate_player_rks(&player_id_clone).await {
                log::error!("异步重新计算玩家[{}] ({}) RKS 失败: {}", player_id_clone, player_name_clone, e);
            }
            
            if self_clone.config.store_push_acc {
                log::info!("开始异步重新计算玩家[{}] ({}) 的推分 ACC...", player_id_clone, player_name_clone);
                if let Err(e) = self_clone.recalculate_push_acc(&player_id_clone).await {
                    log::error!("异步重新计算玩家[{}] ({}) 推分 ACC 失败: {}", player_id_clone, player_name_clone, e);
                }
            }
        });

        // 5. 清除缓存
        self.cache.invalidate(player_id).await;
        log::debug!("玩家[{}] ({}) 缓存已清除", player_id, player_name);
        
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
        let all_scores: Vec<ChartScore> = query_as(
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
        
        // (优化后) 计算并批量插入推分ACC
        let mut records_to_insert = Vec::new();
        for score in &all_scores {
            if score.acc >= 100.0 || score.difficulty_value <= 0.0 {
                continue;
            }
            let target_chart_id = format!("{}-{}", score.song_id, score.difficulty);
            if let Some(push_acc) = calculate_target_chart_push_acc(&target_chart_id, score.difficulty_value, &sorted_records) {
                if push_acc > score.acc {
                    records_to_insert.push((score.song_id.clone(), score.difficulty.clone(), push_acc));
                }
            }
        }

        let push_acc_count = records_to_insert.len();
        if push_acc_count > 0 {
            log::debug!("批量插入 {} 条推分ACC记录...", push_acc_count);
            let update_time_str = Utc::now().to_rfc3339();
            
            let mut sql = String::from("INSERT INTO push_acc (player_id, song_id, difficulty, push_acc, update_time) VALUES ");
            let mut bindings: Vec<String> = Vec::new();

            for (i, (song_id, difficulty, push_acc)) in records_to_insert.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str("(?, ?, ?, ?, ?)");
                bindings.push(player_id.to_string());
                bindings.push(song_id.clone());
                bindings.push(difficulty.clone());
                bindings.push(push_acc.to_string());
                bindings.push(update_time_str.clone());
            }

            let mut q = query(&sql);
            for binding in &bindings {
                q = q.bind(binding);
            }
            
            q.execute(&mut *tx)
                .await
                .map_err(|e| AppError::DatabaseError(format!("批量插入推分ACC失败: {}", e)))?;
        }

        // 提交事务
        tx.commit().await
            .map_err(|e| AppError::DatabaseError(format!("提交事务失败: {}", e)))?;
        
        if push_acc_count > 0 {
            log::info!("计算完成，玩家[{}]共有{}个谱面可推分", player_id, push_acc_count);
        } else {
            log::info!("计算完成，玩家[{}]没有可推分的谱面", player_id);
        }
        
        // 清除缓存
        self.cache.invalidate(player_id).await;
        
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

    /// 辅助函数：获取推分ACC
    async fn get_push_acc_map(&self, player_id: &str) -> Result<Option<HashMap<String, f64>>, AppError> {
        if !self.config.store_push_acc {
            return Ok(None);
        }

        let push_accs = query!(
            "SELECT song_id, difficulty, push_acc FROM push_acc WHERE player_id = ?",
            player_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询推分ACC失败: {}", e)))?;
        
        if push_accs.is_empty() {
            return Ok(None);
        }

        let mut map = HashMap::new();
        for record in push_accs {
            let key = format!("{}-{}", record.song_id, record.difficulty);
            map.insert(key, record.push_acc);
        }
        
        Ok(Some(map))
    }
}
    
// 用于合并查询结果的数据库模型
#[derive(sqlx::FromRow, Clone)]
#[allow(dead_code)]
struct CombinedScoreRecord {
    // 玩家信息
    player_id: String,
    player_name: String,
    rks: f64,
    update_time: DateTime<Utc>,
    // 成绩信息 (由于是LEFT JOIN, 可能为NULL)
    song_id: Option<String>,
    song_name: Option<String>,
    difficulty: Option<String>,
    difficulty_value: Option<f64>,
    score: Option<f64>,
    acc: Option<f64>,
    score_rks: Option<f64>, // 重命名以避免与玩家rks冲突
    is_fc: Option<i32>,
    is_phi: Option<i32>,
    play_time: Option<DateTime<Utc>>,
    is_current: Option<i32>,
    history_rank: Option<i64>,
}

