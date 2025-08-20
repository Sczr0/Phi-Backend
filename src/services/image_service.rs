use crate::models::rks::RksRecord;
use crate::models::user::IdentifierRequest;
use crate::services::phigros::PhigrosService;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::services::song::SongService;
use crate::services::user::UserService;
use crate::utils::cover_loader;
use crate::utils::error::AppError;
use crate::utils::image_renderer::LeaderboardRenderData;
use crate::utils::image_renderer::{self, PlayerStats, SongDifficultyScore, SongRenderData};
use crate::utils::rks_utils;
use crate::utils::token_helper::resolve_token;
use actix_web::web;
use chrono::Utc;
use moka::future::Cache;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio;

// 添加用于缓存统计的原子计数器
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

// --- ImageService 结构体定义 ---

pub struct ImageService {
    bn_image_cache: Cache<(u32, String, crate::controllers::image::Theme), Arc<Vec<u8>>>,
    song_image_cache: Cache<(String, String), Arc<Vec<u8>>>,
    leaderboard_image_cache: Cache<(usize, String), Arc<Vec<u8>>>,
    // 添加缓存统计计数器
    bn_cache_hits: AtomicU64,
    bn_cache_misses: AtomicU64,
    song_cache_hits: AtomicU64,
    song_cache_misses: AtomicU64,
    leaderboard_cache_hits: AtomicU64,
    leaderboard_cache_misses: AtomicU64,
    // 数据库连接池，用于持久化计数器
    db_pool: Option<sqlx::SqlitePool>,
    // 推分ACC预计算缓存
    push_acc_cache: Cache<(String, String), f64>,
}

impl ImageService {
    pub fn new() -> Self {
        Self {
            // B-side图片缓存：最多缓存3000张，每张图片缓存20分钟
            // 考虑到BN图片生成较重，增加缓存容量和时间
            bn_image_cache: Cache::builder()
                .max_capacity(3000)
                .time_to_live(Duration::from_secs(20 * 60))
                .build(),
            // 歌曲图片缓存：最多缓存5000张，每张图片缓存20分钟
            // 歌曲图片相对轻量，可以缓存更多
            song_image_cache: Cache::builder()
                .max_capacity(5000)
                .time_to_live(Duration::from_secs(20 * 60))
                .build(),
            // 排行榜图片缓存：最多缓存100张，每张图片缓存5分钟
            // 排行榜变化频繁，适当增加缓存时间和容量
            leaderboard_image_cache: Cache::builder()
                .max_capacity(100)
                .time_to_live(Duration::from_secs(5 * 60))
                .build(),
            // 推分ACC缓存：最多缓存10000个计算结果，缓存10分钟
            // 推分ACC计算复杂度高，需要更大的缓存
            push_acc_cache: Cache::builder()
                .max_capacity(10000)
                .time_to_live(Duration::from_secs(10 * 60))
                .build(),
            // 初始化缓存统计计数器
            bn_cache_hits: AtomicU64::new(0),
            bn_cache_misses: AtomicU64::new(0),
            song_cache_hits: AtomicU64::new(0),
            song_cache_misses: AtomicU64::new(0),
            leaderboard_cache_hits: AtomicU64::new(0),
            leaderboard_cache_misses: AtomicU64::new(0),
            // 数据库连接池初始化为 None，需要在创建服务时设置
            db_pool: None,
        }
    }
    
    pub fn with_db_pool(mut self, pool: sqlx::SqlitePool) -> Self {
        self.db_pool = Some(pool);
        self
    }
}

// --- 服务层函数 (现在是 ImageService 的方法) ---

impl ImageService {
    pub async fn generate_bn_image(
        &self,
        n: u32,
        identifier: web::Json<IdentifierRequest>,
        theme: &crate::controllers::image::Theme,
        phigros_service: web::Data<PhigrosService>,
        user_service: web::Data<UserService>,
        player_archive_service: web::Data<PlayerArchiveService>,
    ) -> Result<Vec<u8>, AppError> {
        let token = resolve_token(&identifier, &user_service).await?;
        
        // 获取存档校验和作为数据版本标识
        let save_checksum = phigros_service.get_save_checksum(&token).await.unwrap_or_else(|_| "unknown".to_string());
        
        // 使用数据版本标识和参数作为缓存键，确保数据变化时缓存失效
        let cache_key = (n, save_checksum.clone(), theme.clone());
        
        // 先检查缓存中是否已存在
        if let Some(cached) = self.bn_image_cache.get(&cache_key).await {
            // 记录真实的缓存命中
            self.bn_cache_hits.fetch_add(1, AtomicOrdering::Relaxed);
            log::debug!("BN图片缓存命中: n={}, checksum={}", n, &save_checksum[..8]);
            return Ok(cached.to_vec());
        }

        let image_bytes_arc = self
            .bn_image_cache
            .try_get_with(cache_key, async {
                // 只在这里获取一次数据
                let (rks_save_res, profile_res) = tokio::join!(
                    phigros_service.get_rks(&token),
                    phigros_service.get_profile(&token)
                );

                let (all_rks_result, save) = rks_save_res?;
                if all_rks_result.records.is_empty() {
                    return Err(AppError::Other(format!(
                        "用户无成绩记录，无法生成 B{n} 图片"
                    )));
                }
                let all_scores = all_rks_result.records;

                let player_id = save
                    .user
                    .as_ref()
                    .and_then(|u| u.get("objectId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let player_nickname = profile_res.ok().map(|p| p.nickname);

                let player_name_for_archive =
                    player_nickname.clone().unwrap_or_else(|| player_id.clone());

                let mut fc_map = HashMap::new();
                if let Some(game_record_map) = &save.game_record {
                    for (song_id, difficulties) in game_record_map {
                        for (diff_name, record) in difficulties {
                            if record.fc == Some(true) {
                                fc_map.insert(format!("{song_id}-{diff_name}"), true);
                            }
                        }
                    }
                }

                let archive_service_clone = player_archive_service.clone();
                let player_id_clone = player_id.clone();
                let player_name_clone = player_name_for_archive.clone();
                let scores_clone = all_scores.clone();
                let fc_map_clone = fc_map.clone();

                tokio::spawn(async move {
                    if let Err(e) = archive_service_clone
                        .update_player_scores_from_rks_records(
                            &player_id_clone,
                            &player_name_clone,
                            &scores_clone,
                            &fc_map_clone,
                        )
                        .await
                    {
                        log::error!(
                            "后台更新玩家 {player_name_clone} ({player_id_clone}) 存档失败: {e}"
                        );
                    }
                });

                let mut sorted_scores = all_scores;
                sorted_scores.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

                let (exact_rks, _rounded_rks) =
                    rks_utils::calculate_player_rks_details(&sorted_scores);

                let top_n_scores = sorted_scores
                    .iter()
                    .take(n as usize)
                    .cloned()
                    .collect::<Vec<_>>();

                let ap_scores_ranked: Vec<_> =
                    sorted_scores.iter().filter(|s| s.acc == 100.0).collect();
                let ap_top_3_scores: Vec<RksRecord> = ap_scores_ranked
                    .iter()
                    .take(3)
                    .map(|&s| s.clone())
                    .collect();

                let ap_top_3_avg = if ap_top_3_scores.len() >= 3 {
                    Some(ap_top_3_scores.iter().map(|s| s.rks).sum::<f64>() / 3.0)
                } else {
                    None
                };

                let count_for_b27_display_avg = sorted_scores.len().min(27);
                let best_27_avg = if count_for_b27_display_avg > 0 {
                    Some(
                        sorted_scores
                            .iter()
                            .take(count_for_b27_display_avg)
                            .map(|s| s.rks)
                            .sum::<f64>()
                            / count_for_b27_display_avg as f64,
                    )
                } else {
                    None
                };

                // 性能优化：对于未绑定的直接提供Token的用户，计算推分acc时只限制在规定范围内
                // 比如渲染Bn图片时仅计算BestN以内的推分acc
                // 优化：预计算推分ACC，利用服务层缓存减少重复计算
                let mut push_acc_map: HashMap<String, f64> = HashMap::new();
                for score in top_n_scores.iter()
                    .filter(|score| score.acc < 100.0 && score.difficulty_value > 0.0) 
                {
                    let target_chart_id_full = format!("{}-{}", score.song_id, score.difficulty);
                    
                    // 尝试从服务层缓存获取
                    if let Some(cached_push_acc) = self.push_acc_cache.get(&(target_chart_id_full.clone(), player_id.clone())).await {
                        push_acc_map.insert(target_chart_id_full.clone(), cached_push_acc);
                    } else {
                        // 缓存未命中，重新计算
                        if let Some(push_acc) = rks_utils::calculate_target_chart_push_acc(
                            &target_chart_id_full,
                            score.difficulty_value,
                            &top_n_scores,
                        ) {
                            // 存入缓存
                            self.push_acc_cache.insert((target_chart_id_full.clone(), player_id.clone()), push_acc).await;
                            push_acc_map.insert(target_chart_id_full.clone(), push_acc);
                        }
                    }
                }

                let (challenge_rank, data_string) = if let Some(game_progress) = &save.game_progress
                {
                    // 1. 解析课题等级
                    let rank = game_progress
                        .get("challengeModeRank")
                        .and_then(|v| v.as_i64())
                        .and_then(|rank_num| {
                            if rank_num <= 0 {
                                return None;
                            }
                            let rank_str = rank_num.to_string();
                            if rank_str.is_empty() {
                                return None;
                            }
                            let (color_char, level_str) = rank_str.split_at(1);
                            let color = match color_char {
                                "1" => "Green",
                                "2" => "Blue",
                                "3" => "Red",
                                "4" => "Gold",
                                "5" => "Rainbow",
                                _ => return None,
                            };
                            Some((color.to_string(), level_str.to_string()))
                        });

                    // 2. 格式化Data
                    let money_str = game_progress
                        .get("money")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| {
                            let units = ["KB", "MB", "GB", "TB"];
                            let mut parts: Vec<String> = arr
                                .iter()
                                .zip(units.iter())
                                .filter_map(|(val, &unit)| {
                                    val.as_u64().and_then(|u_val| {
                                        if u_val > 0 {
                                            Some(format!("{u_val} {unit}"))
                                        } else {
                                            None
                                        }
                                    })
                                })
                                .collect();
                            parts.reverse(); // 从大单位开始显示
                            if parts.is_empty() {
                                None
                            } else {
                                Some(format!("Data: {}", parts.join(", ")))
                            }
                        });

                    (rank, money_str)
                } else {
                    (None, None)
                };

                let app_config = crate::utils::config::get_config()?;
                let stats = PlayerStats {
                    ap_top_3_avg,
                    best_27_avg,
                    real_rks: Some(exact_rks),
                    player_name: player_nickname,
                    update_time: Utc::now(),
                    n,
                    ap_top_3_scores,
                    challenge_rank,
                    data_string,
                    custom_footer_text: Some(app_config.custom_footer_text),
                };

                let theme_clone = theme.clone();
                let png_data = tokio::task::spawn_blocking(move || {
                    let svg_string = image_renderer::generate_svg_string(
                        &top_n_scores,
                        &stats,
                        Some(&push_acc_map),
                        &theme_clone,
                    )?;
                    image_renderer::render_svg_to_png(svg_string)
                })
                .await
                .map_err(|e| AppError::InternalError(format!("Blocking task join error: {e}")))?
                .map_err(|e| AppError::InternalError(format!("SVG rendering error: {e}")))?;

                Ok(Arc::new(png_data))
            })
            .await
            .map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

        // 记录缓存未命中（只有在try_get_with实际计算生成图片时才会执行到这里）
        self.bn_cache_misses.fetch_add(1, AtomicOrdering::Relaxed);
        log::debug!("BN图片缓存未命中: n={}, checksum={}", n, &save_checksum[..8]);
        
        // 增加计数器
        if let Err(e) = self.increment_counter("bn").await {
            log::error!("更新BN图片计数器失败: {e}");
        }
        
        Ok(image_bytes_arc.to_vec())
    }

    // 新增：生成单曲成绩图片的服务逻辑
    pub async fn generate_song_image(
        &self,
        song_query: String,
        identifier: web::Json<IdentifierRequest>,
        phigros_service: web::Data<PhigrosService>,
        user_service: web::Data<UserService>,
        song_service: web::Data<SongService>,
        player_archive_service: web::Data<PlayerArchiveService>,
    ) -> Result<Vec<u8>, AppError> {
        let token = resolve_token(&identifier, &user_service).await?;
        
        // 获取歌曲信息用于缓存键
        let song_info = song_service.search_song(&song_query)?;
        let song_id = song_info.id.clone();
        
        // 获取存档校验和作为数据版本标识
        let save_checksum = phigros_service.get_save_checksum(&token).await.unwrap_or_else(|_| "unknown".to_string());
        
        // 使用数据版本标识和参数作为缓存键，确保数据变化时缓存失效
        let cache_key = (song_id.clone(), save_checksum.clone());
        
        // 先检查缓存中是否已存在
        if let Some(cached) = self.song_image_cache.get(&cache_key).await {
            // 记录真实的缓存命中
            self.song_cache_hits.fetch_add(1, AtomicOrdering::Relaxed);
            log::debug!("歌曲图片缓存命中: song_id={}, checksum={}", &song_id[..std::cmp::min(20, song_id.len())], &save_checksum[..8]);
            return Ok(cached.to_vec());
        }

        let image_bytes_arc = self
            .song_image_cache
            .try_get_with(cache_key, async {
                let song_info = song_service.search_song(&song_query)?;
                let song_id = song_info.id.clone();
                let song_name = song_info.song.clone();

                // 只在这里获取一次数据
                let (rks_save_res, profile_res) = tokio::join!(
                    phigros_service.get_rks(&token),
                    phigros_service.get_profile(&token)
                );

                let (all_rks_result, save) = rks_save_res?;
                let mut all_records = all_rks_result.records;

                let player_id = save
                    .user
                    .as_ref()
                    .and_then(|u| u.get("objectId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let player_nickname = profile_res.ok().map(|p| p.nickname);
                let player_name_for_archive =
                    player_nickname.clone().unwrap_or_else(|| player_id.clone());

                let fc_map: HashMap<String, bool> = if let Some(game_record_map) = &save.game_record
                {
                    game_record_map
                        .iter()
                        .flat_map(|(song_id, difficulties)| {
                            difficulties.iter().filter_map(move |(diff_name, record)| {
                                if record.fc == Some(true) {
                                    Some((format!("{song_id}-{diff_name}"), true))
                                } else {
                                    None
                                }
                            })
                        })
                        .collect()
                } else {
                    HashMap::new()
                };

                let archive_service_clone = player_archive_service.clone();
                let player_id_clone = player_id.clone();
                let player_name_clone = player_name_for_archive.clone();
                let records_clone = all_records.clone();
                let fc_map_clone = fc_map.clone();

                tokio::spawn(async move {
                    if let Err(e) = archive_service_clone
                        .update_player_scores_from_rks_records(
                            &player_id_clone,
                            &player_name_clone,
                            &records_clone,
                            &fc_map_clone,
                        )
                        .await
                    {
                        log::error!(
                            "后台更新玩家 {player_name_clone} ({player_id_clone}) 存档失败: {e}"
                        );
                    }
                });

                all_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

                let game_record_map = save.game_record.ok_or_else(|| {
                    AppError::Other("存档中无成绩记录，无法生成单曲图片".to_string())
                })?;
                let song_difficulties_from_save =
                    game_record_map.get(&song_id).cloned().unwrap_or_default();

                let difficulty_constants = song_service.get_song_difficulty(&song_id)?;

                let mut difficulty_scores_map = HashMap::new();
                for diff_key in ["EZ", "HD", "IN", "AT"] {
                    let difficulty_value = match diff_key {
                        "EZ" => difficulty_constants.ez,
                        "HD" => difficulty_constants.hd,
                        "IN" => difficulty_constants.inl,
                        "AT" => difficulty_constants.at,
                        _ => None,
                    };

                    let record = song_difficulties_from_save.get(diff_key);
                    let acc = record.and_then(|r| r.acc);
                    let is_phi = acc == Some(100.0);

                    let push_acc = if let Some(dv) = difficulty_value {
                        if dv > 0.0 && !is_phi {
                            let target_chart_id = format!("{song_id}-{diff_key}");
                            rks_utils::calculate_target_chart_push_acc(
                                &target_chart_id,
                                dv,
                                &all_records,
                            )
                        } else {
                            Some(100.0)
                        }
                    } else {
                        Some(100.0)
                    };

                    difficulty_scores_map.insert(
                        diff_key.to_string(),
                        Some(SongDifficultyScore {
                            score: record.and_then(|r| r.score),
                            acc,
                            rks: record.and_then(|r| r.rks),
                            difficulty_value,
                            is_fc: record.and_then(|r| r.fc),
                            is_phi: Some(is_phi),
                            player_push_acc: push_acc,
                        }),
                    );
                }

                let illustration_path_png = PathBuf::from(cover_loader::COVERS_DIR)
                    .join("ill")
                    .join(format!("{song_id}.png"));
                let illustration_path_jpg = PathBuf::from(cover_loader::COVERS_DIR)
                    .join("ill")
                    .join(format!("{song_id}.jpg"));
                let illustration_path = if illustration_path_png.exists() {
                    Some(illustration_path_png)
                } else if illustration_path_jpg.exists() {
                    Some(illustration_path_jpg)
                } else {
                    None
                };

                let render_data = SongRenderData {
                    song_name,
                    song_id: song_id.clone(),
                    player_name: player_nickname,
                    update_time: Utc::now(),
                    difficulty_scores: difficulty_scores_map,
                    illustration_path,
                };

                let png_data = tokio::task::spawn_blocking(move || {
                    let svg_string = image_renderer::generate_song_svg_string(&render_data)?;
                    image_renderer::render_svg_to_png(svg_string)
                })
                .await
                .map_err(|e| AppError::InternalError(format!("Blocking task join error: {e}")))?
                .map_err(|e| AppError::InternalError(format!("SVG rendering error: {e}")))?;

                Ok(Arc::new(png_data))
            })
            .await
            .map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

        // 记录缓存未命中（只有在try_get_with实际计算生成图片时才会执行到这里）
        self.song_cache_misses.fetch_add(1, AtomicOrdering::Relaxed);
        log::debug!("歌曲图片缓存未命中: song_id={}, checksum={}", &song_id[..std::cmp::min(20, song_id.len())], &save_checksum[..8]);
        
        // 增加计数器
        if let Err(e) = self.increment_counter("song").await {
            log::error!("更新歌曲图片计数器失败: {e}");
        }
        
        Ok(image_bytes_arc.to_vec())
    }

    // --- 排行榜相关函数 ---

    pub async fn generate_rks_leaderboard_image(
        &self,
        limit: Option<usize>, // 显示多少名玩家，默认 20
        player_archive_service: web::Data<PlayerArchiveService>,
    ) -> Result<Vec<u8>, AppError> {
        let actual_limit = limit.unwrap_or(20).min(100);
        
        // 获取排行榜更新时间用于缓存键
        let last_update = player_archive_service
            .get_ref()
            .get_latest_rks_update_time()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        
        // 使用包含更新时间的缓存键
        let cache_key = (actual_limit, last_update.clone());
        
        // 尝试从缓存获取
        if let Some(cached) = self.leaderboard_image_cache.get(&cache_key).await {
            self.leaderboard_cache_hits.fetch_add(1, AtomicOrdering::Relaxed);
            log::debug!("排行榜图片缓存命中: limit={}, update_time={}", actual_limit, &last_update[..std::cmp::min(10, last_update.len())]);
            return Ok(cached.to_vec());
        }
        
        self.leaderboard_cache_misses.fetch_add(1, AtomicOrdering::Relaxed);
        log::debug!("排行榜图片缓存未命中: limit={}, update_time={}", actual_limit, &last_update[..std::cmp::min(10, last_update.len())]);

        let image_bytes_arc = self
            .leaderboard_image_cache
            .try_get_with(cache_key, async {
                log::info!("生成RKS排行榜图片，显示前{actual_limit}名玩家");

                let top_players = player_archive_service
                    .get_ref()
                    .get_rks_ranking(actual_limit)
                    .await?;

                let render_data = LeaderboardRenderData {
                    title: "RKS 排行榜".to_string(),
                    entries: top_players,
                    display_count: actual_limit,
                    update_time: Utc::now(),
                };

                let svg_string = image_renderer::generate_leaderboard_svg_string(&render_data)?;

                let png_data = tokio::task::spawn_blocking(move || image_renderer::render_svg_to_png(svg_string))
                    .await
                    .map_err(|e| {
                        AppError::InternalError(format!("Blocking task error for leaderboard: {e}"))
                    })?
                    .map_err(|e| AppError::InternalError(format!("SVG rendering error: {e}")))?;

                Ok(Arc::new(png_data))
            })
            .await
            .map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

        // 增加计数器
        if let Err(e) = self.increment_counter("leaderboard").await {
            log::error!("更新排行榜图片计数器失败: {e}");
        }

        Ok(image_bytes_arc.to_vec())
    }
}

// 添加缓存统计方法
impl ImageService {
    pub fn get_cache_stats(&self) -> serde_json::Value {
        let bn_hits = self.bn_cache_hits.load(std::sync::atomic::Ordering::Relaxed);
        let bn_misses = self.bn_cache_misses.load(std::sync::atomic::Ordering::Relaxed);
        let bn_hit_rate = if bn_hits + bn_misses > 0 {
            format!("{:.2}%", (bn_hits as f64 / (bn_hits + bn_misses) as f64) * 100.0)
        } else {
            "0.00%".to_string()
        };

        let song_hits = self.song_cache_hits.load(std::sync::atomic::Ordering::Relaxed);
        let song_misses = self.song_cache_misses.load(std::sync::atomic::Ordering::Relaxed);
        let song_hit_rate = if song_hits + song_misses > 0 {
            format!("{:.2}%", (song_hits as f64 / (song_hits + song_misses) as f64) * 100.0)
        } else {
            "0.00%".to_string()
        };

        let leaderboard_hits = self.leaderboard_cache_hits.load(std::sync::atomic::Ordering::Relaxed);
        let leaderboard_misses = self.leaderboard_cache_misses.load(std::sync::atomic::Ordering::Relaxed);
        let leaderboard_hit_rate = if leaderboard_hits + leaderboard_misses > 0 {
            format!("{:.2}%", (leaderboard_hits as f64 / (leaderboard_hits + leaderboard_misses) as f64) * 100.0)
        } else {
            "0.00%".to_string()
        };

        serde_json::json!({
            "bn_image_cache": {
                "hits": bn_hits,
                "misses": bn_misses,
                "hit_rate": bn_hit_rate
            },
            "song_image_cache": {
                "hits": song_hits,
                "misses": song_misses,
                "hit_rate": song_hit_rate
            },
            "leaderboard_image_cache": {
                "hits": leaderboard_hits,
                "misses": leaderboard_misses,
                "hit_rate": leaderboard_hit_rate
            }
        })
    }
    
    // 增加图片生成计数
    async fn increment_counter(&self, image_type: &str) -> Result<(), AppError> {
        if let Some(ref pool) = self.db_pool {
            sqlx::query!(
                "UPDATE image_counter SET count = count + 1, last_updated = datetime('now') WHERE image_type = ?",
                image_type
            )
            .execute(pool)
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        }
        Ok(())
    }
    
    // 获取所有计数器统计信息
    pub async fn get_image_stats(&self) -> Result<serde_json::Value, AppError> {
        if let Some(ref pool) = self.db_pool {
            let counters = sqlx::query_as!(crate::models::image_counter::ImageCounter,
                "SELECT id, image_type, count, last_updated FROM image_counter"
            )
            .fetch_all(pool)
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            
            let mut stats = serde_json::Map::new();
            for counter in counters {
                stats.insert(counter.image_type, serde_json::json!({
                    "count": counter.count,
                    "last_updated": counter.last_updated
                }));
            }
            
            Ok(serde_json::Value::Object(stats))
        } else {
            // 如果没有数据库连接，返回空对象
            Ok(serde_json::json!({}))
        }
    }
    
    // 获取特定类型的计数器统计信息
    pub async fn get_image_stats_by_type(&self, image_type: &str) -> Result<Option<crate::models::image_counter::ImageCounter>, AppError> {
        if let Some(ref pool) = self.db_pool {
            let counter = sqlx::query_as!(crate::models::image_counter::ImageCounter,
                "SELECT id, image_type, count, last_updated FROM image_counter WHERE image_type = ?",
                image_type
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            
            Ok(counter)
        } else {
            // 如果没有数据库连接，返回 None
            Ok(None)
        }
    }
}
