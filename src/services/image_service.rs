use crate::models::cloud_save::FullSaveData;
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
use chrono::{DateTime, Utc};
use moka::future::Cache;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::{self, sync::Semaphore};

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
    // 新增：用于限制并发图片渲染任务的信号量
    render_semaphore: Arc<Semaphore>,
}

impl ImageService {
    pub fn new(max_concurrent_renders: usize) -> Self {
        Self {
            // B-side图片缓存：最多缓存3000张，每张图片缓存5分钟
            // 考虑到BN图片生成较重，增加缓存容量和时间
            bn_image_cache: Cache::builder()
                .max_capacity(3000)
                .time_to_live(Duration::from_secs(5 * 60))
                .build(),
            // 歌曲图片缓存：最多缓存5000张，每张图片缓存5分钟
            // 歌曲图片相对轻量，可以缓存更多
            song_image_cache: Cache::builder()
                .max_capacity(5000)
                .time_to_live(Duration::from_secs(5 * 60))
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
            // 初始化信号量，限制并发渲染数量
            render_semaphore: Arc::new(Semaphore::new(max_concurrent_renders)),
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
        let start_time = std::time::Instant::now();
        log::info!("BN图片生成 - 开始处理请求: {:?}", start_time.elapsed());

        let checksum_start = std::time::Instant::now();
        let save_checksum = if identifier.data_source.as_deref() == Some("external") {
            // 外部数据源：使用平台和ID生成唯一校验和
            if let Some(api_user_id) = &identifier.api_user_id {
                format!("external_api_{}", api_user_id)
            } else {
                format!(
                    "external_{}_{}",
                    identifier.platform.as_deref().unwrap_or(""),
                    identifier.platform_id.as_deref().unwrap_or("")
                )
            }
        } else {
            // 内部数据源使用token获取校验和
            let token = resolve_token(&identifier, &user_service).await?;
            phigros_service
                .get_save_checksum(&token)
                .await
                .unwrap_or_else(|_| "unknown".to_string())
        };
        log::info!(
            "BN图片生成 - 获取存档校验和耗时: {:?}",
            checksum_start.elapsed()
        );

        let cache_key = (n, save_checksum.clone(), theme.clone());

        if let Some(cached) = self.bn_image_cache.get(&cache_key).await {
            self.bn_cache_hits.fetch_add(1, AtomicOrdering::Relaxed);
            log::debug!("BN图片缓存命中: n={}, checksum={}", n, &save_checksum[..8]);
            log::info!("BN图片生成 - 总耗时(缓存命中): {:?}", start_time.elapsed());
            return Ok(cached.to_vec());
        }

        let image_bytes_arc = self
            .bn_image_cache
            .try_get_with(cache_key, async {
                let data_fetch_start = std::time::Instant::now();
                let (full_data_res, profile_res) = if identifier.data_source.as_deref() == Some("external") {
                    // 使用外部数据源
                    tokio::join!(
                        phigros_service.get_full_save_data_with_source(&identifier),
                        async { Ok(crate::models::user::UserProfile {
                            object_id: "external".to_string(),
                            nickname: identifier.platform.as_ref()
                                .map(|p| format!("{}:{}", p, identifier.platform_id.as_ref().unwrap_or(&"unknown".to_string())))
                                .unwrap_or_else(|| "External User".to_string())
                        }) }
                    )
                } else {
                    // 使用内部数据源
                    let token = resolve_token(&identifier, &user_service).await?;
                    tokio::join!(
                        phigros_service.get_full_save_data(&token),
                        phigros_service.get_profile(&token)
                    )
                };
                log::info!(
                    "BN图片生成 - 数据获取耗时: {:?}",
                    data_fetch_start.elapsed()
                );

                let full_data = full_data_res?;
                if full_data.rks_result.records.is_empty() {
                    return Err(AppError::Other(format!(
                        "用户无成绩记录，无法生成 B{n} 图片"
                    )));
                }

                let player_nickname = profile_res.ok().map(|p| p.nickname);

                let (player_id, player_name) = if identifier.data_source.as_deref() == Some("external") {
                    // 外部数据源：从外部API响应中获取PlayerId和玩家名称
                    let player_id = full_data.cloud_summary["results"][0]["PlayerId"]
                        .as_str()
                        .unwrap_or("external:unknown")
                        .to_string();
                    let player_name = full_data.cloud_summary["results"][0]["PlayerId"]
                        .as_str()
                        .unwrap_or("external:unknown")
                        .to_string();
                    (player_id, player_name)
                } else {
                    // 内部数据源：从存档数据中获取objectId和玩家名称
                    let player_id = full_data
                        .save
                        .user
                        .as_ref()
                        .and_then(|u| u.get("objectId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let player_name = player_nickname.clone().unwrap_or(player_id.clone());
                    (player_id, player_name)
                };

                // --- 异步更新玩家存档 ---
                let player_name_for_archive = player_name.clone();
                let mut fc_map = HashMap::new();
                if let Some(game_record_map) = &full_data.save.game_record {
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
                let scores_clone = full_data.rks_result.records.clone();
                let is_external = identifier.data_source.as_deref() == Some("external");
                tokio::spawn(async move {
                    if let Err(e) = archive_service_clone
                        .update_player_scores_from_rks_records(
                            &player_id_clone,
                            &player_name_clone,
                            &scores_clone,
                            &fc_map,
                            is_external,
                        )
                        .await
                    {
                        log::error!(
                            "后台更新玩家 {player_name_clone} ({player_id_clone}) 存档失败: {e}"
                        );
                    }
                });

                // --- 异步预计算推分ACC ---
                let push_acc_start = std::time::Instant::now();
                let mut push_acc_map: HashMap<String, f64> = HashMap::new();
                let mut sorted_scores_for_push = full_data.rks_result.records.clone();
                sorted_scores_for_push.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));
                let top_n_scores_for_push: Vec<RksRecord> = sorted_scores_for_push
                    .iter()
                    .take(n as usize)
                    .cloned()
                    .collect();

                for score in top_n_scores_for_push
                    .iter()
                    .filter(|s| s.acc < 100.0 && s.difficulty_value > 0.0)
                {
                    let key = (format!("{}-{}", score.song_id, score.difficulty), player_id.clone());
                    if let Some(cached) = self.push_acc_cache.get(&key).await {
                        push_acc_map.insert(key.0, cached);
                    } else if let Some(push_acc) = rks_utils::calculate_target_chart_push_acc(
                        &key.0,
                        score.difficulty_value,
                        &top_n_scores_for_push,
                    ) {
                        self.push_acc_cache.insert(key.clone(), push_acc).await;
                        push_acc_map.insert(key.0, push_acc);
                    }
                }
                log::info!(
                    "BN图片生成 - 推分ACC计算耗时: {:?}",
                    push_acc_start.elapsed()
                );

                // --- 将所有权转移到阻塞任务 ---
                let render_start = std::time::Instant::now();
                let theme_clone = theme.clone();

                let permit = self.render_semaphore.clone().acquire_owned().await.map_err(|e| AppError::InternalError(format!("Failed to acquire semaphore permit: {e}")))?;

                let png_data_result = web::block(move || {
                    let _permit = permit;
                    Self::_render_bn_image_sync(
                        full_data,
                        Some(player_name),
                        n,
                        push_acc_map,
                        theme_clone,
                    )
                })
                .await
                .map_err(|e| AppError::InternalError(format!("Blocking task join error: {e}")))?;

                let png_data = png_data_result?;
                log::info!("BN图片生成 - 渲染总耗时: {:?}", render_start.elapsed());

                Ok(Arc::new(png_data))
            })
            .await
            .map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

        self.bn_cache_misses.fetch_add(1, AtomicOrdering::Relaxed);
        log::debug!(
            "BN图片缓存未命中: n={}, checksum={}",
            n,
            &save_checksum[..8]
        );

        if let Err(e) = self.increment_counter("bn").await {
            log::error!("更新BN图片计数器失败: {e}");
        }

        log::info!(
            "BN图片生成 - 总耗时(缓存未命中): {:?}",
            start_time.elapsed()
        );
        Ok(image_bytes_arc.to_vec())
    }

    /// 同步执行的BN图片渲染函数
    fn _render_bn_image_sync(
        full_data: FullSaveData,
        player_name: Option<String>,
        n: u32,
        push_acc_map: HashMap<String, f64>,
        theme: crate::controllers::image::Theme,
    ) -> Result<Vec<u8>, AppError> {
        let data_process_start = std::time::Instant::now();
        let mut sorted_scores = full_data.rks_result.records;
        sorted_scores.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

        let (exact_rks, _) = rks_utils::calculate_player_rks_details(&sorted_scores);

        let top_n_scores: Vec<RksRecord> =
            sorted_scores.iter().take(n as usize).cloned().collect();

        let ap_scores_ranked: Vec<_> = sorted_scores.iter().filter(|s| s.acc == 100.0).collect();
        let ap_top_3_scores: Vec<RksRecord> =
            ap_scores_ranked.iter().take(3).map(|&s| s.clone()).collect();

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

        let (challenge_rank, data_string) = if let Some(game_progress) = &full_data.save.game_progress {
            let rank = game_progress
                .get("challengeModeRank")
                .and_then(|v| v.as_i64())
                .and_then(|rank_num| {
                    if rank_num <= 0 { return None; }
                    let rank_str = rank_num.to_string();
                    if rank_str.is_empty() { return None; }
                    let (color_char, level_str) = rank_str.split_at(1);
                    let color = match color_char {
                        "1" => "Green", "2" => "Blue", "3" => "Red",
                        "4" => "Gold", "5" => "Rainbow", _ => return None,
                    };
                    Some((color.to_string(), level_str.to_string()))
                });
            let money_str = game_progress.get("money").and_then(|v| v.as_array()).and_then(|arr| {
                let units = ["KB", "MB", "GB", "TB"];
                let mut parts: Vec<String> = arr.iter().zip(units.iter()).filter_map(|(val, &unit)| {
                    val.as_u64().and_then(|u_val| if u_val > 0 { Some(format!("{u_val} {unit}")) } else { None })
                }).collect();
                parts.reverse();
                if parts.is_empty() { None } else { Some(format!("Data: {}", parts.join(", "))) }
            });
            (rank, money_str)
        } else {
            (None, None)
        };
        log::info!("BN图片生成 - 数据处理耗时: {:?}", data_process_start.elapsed());

        let stats_creation_start = std::time::Instant::now();
        let app_config = crate::utils::config::get_config()?;
        let stats = PlayerStats {
            ap_top_3_avg,
            best_27_avg,
            real_rks: Some(exact_rks),
            player_name,
            update_time: {
                let date_str = full_data.cloud_summary["results"][0]["updatedAt"]
                    .as_str()
                    .unwrap_or_default();
                DateTime::parse_from_rfc3339(date_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now())
            },
            n,
            ap_top_3_scores,
            challenge_rank,
            data_string,
            custom_footer_text: Some(app_config.custom_footer_text),
            is_user_generated: false, // 官方数据
        };
        log::info!("BN图片生成 - Stats创建耗时: {:?}", stats_creation_start.elapsed());

        let svg_gen_start = std::time::Instant::now();
        let svg_string = image_renderer::generate_svg_string(
            &top_n_scores,
            &stats,
            Some(&push_acc_map),
            &theme,
        )?;
        log::info!("BN图片生成 - SVG生成耗时: {:?}", svg_gen_start.elapsed());

        let png_render_start = std::time::Instant::now();
        let result = image_renderer::render_svg_to_png(svg_string, false); // 官方数据
        log::info!("BN图片生成 - PNG渲染耗时: {:?}", png_render_start.elapsed());
        result
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
        let start_time = std::time::Instant::now();
        log::info!("歌曲图片生成 - 开始处理请求: {:?}", start_time.elapsed());

        // 使用新的 search_songs 函数来处理可能的歧义
        let search_results = song_service.search_songs(&song_query)?;

        // 检查搜索结果的数量
        let song_info = if search_results.len() == 1 {
            search_results.into_iter().next().unwrap()
        } else {
            // 如果找到多个或零个结果，则返回错误
            let found_songs: Vec<String> = search_results
                .into_iter()
                .map(|s| format!("{} ({})", s.song, s.id))
                .collect();
            return Err(AppError::BadRequest(format!(
                "歌曲 '{}' 存在歧义或未找到，请使用更精确的名称或歌曲ID。可能匹配: {}",
                song_query,
                found_songs.join(", ")
            )));
        };
        let song_id = song_info.id.clone();

        let save_checksum = if identifier.data_source.as_deref() == Some("external") {
            // 外部数据源：使用平台和ID生成唯一校验和
            if let Some(api_user_id) = &identifier.api_user_id {
                format!("external_api_{}", api_user_id)
            } else {
                format!(
                    "external_{}_{}",
                    identifier.platform.as_deref().unwrap_or(""),
                    identifier.platform_id.as_deref().unwrap_or("")
                )
            }
        } else {
            // 内部数据源使用token获取校验和
            let token = resolve_token(&identifier, &user_service).await?;
            phigros_service
                .get_save_checksum(&token)
                .await
                .unwrap_or_else(|_| "unknown".to_string())
        };

        let cache_key = (song_id.clone(), save_checksum.clone());

        if let Some(cached) = self.song_image_cache.get(&cache_key).await {
            self.song_cache_hits.fetch_add(1, AtomicOrdering::Relaxed);
            log::debug!(
                "歌曲图片缓存命中: song_id={}, checksum={}",
                &song_id[..std::cmp::min(20, song_id.len())],
                &save_checksum[..8]
            );
            log::info!(
                "歌曲图片生成 - 总耗时(缓存命中): {:?}",
                start_time.elapsed()
            );
            return Ok(cached.to_vec());
        }

        let image_bytes_arc = self
            .song_image_cache
            .try_get_with(cache_key, async {
                let (full_data_res, profile_res) = if identifier.data_source.as_deref() == Some("external") {
                    // 使用外部数据源
                    tokio::join!(
                        phigros_service.get_full_save_data_with_source(&identifier),
                        async { Ok(crate::models::user::UserProfile {
                            object_id: "external".to_string(),
                            nickname: identifier.platform.as_ref()
                                .map(|p| format!("{}:{}", p, identifier.platform_id.as_ref().unwrap_or(&"unknown".to_string())))
                                .unwrap_or_else(|| "External User".to_string())
                        }) }
                    )
                } else {
                    // 使用内部数据源
                    let token = resolve_token(&identifier, &user_service).await?;
                    tokio::join!(
                        phigros_service.get_full_save_data(&token),
                        phigros_service.get_profile(&token)
                    )
                };

                let full_data = full_data_res?;
                let player_nickname = profile_res.ok().map(|p| p.nickname);
                let (player_id, player_name) = if identifier.data_source.as_deref() == Some("external") {
                    // 外部数据源：从外部API响应中获取PlayerId和玩家名称
                    let player_id = full_data.cloud_summary["results"][0]["PlayerId"]
                        .as_str()
                        .unwrap_or("external:unknown")
                        .to_string();
                    let player_name = full_data.cloud_summary["results"][0]["PlayerId"]
                        .as_str()
                        .unwrap_or("external:unknown")
                        .to_string();
                    (player_id, player_name)
                } else {
                    // 内部数据源：从存档数据中获取objectId和玩家名称
                    let player_id = full_data
                        .save
                        .user
                        .as_ref()
                        .and_then(|u| u.get("objectId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let player_name = player_nickname.unwrap_or(player_id.clone());
                    (player_id, player_name)
                };

                // --- 异步更新玩家存档 ---
                let player_name_for_archive = player_name.clone();
                let fc_map: HashMap<String, bool> =
                    if let Some(game_record_map) = &full_data.save.game_record {
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
                let records_clone = full_data.rks_result.records.clone();
                let is_external = identifier.data_source.as_deref() == Some("external");
                tokio::spawn(async move {
                    if let Err(e) = archive_service_clone
                        .update_player_scores_from_rks_records(
                            &player_id_clone,
                            &player_name_clone,
                            &records_clone,
                            &fc_map,
                            is_external,
                        )
                        .await
                    {
                        log::error!(
                            "后台更新玩家 {player_name_clone} ({player_id_clone}) 存档失败: {e}"
                        );
                    }
                });

                // --- 将所有权转移到阻塞任务 ---
                let render_start = std::time::Instant::now();
                let song_service_clone = song_service.clone();

                let permit = self.render_semaphore.clone().acquire_owned().await.map_err(|e| AppError::InternalError(format!("Failed to acquire semaphore permit: {e}")))?;

                let png_data_result = web::block(move || {
                    let _permit = permit;
                    Self::_render_song_image_sync(
                        full_data,
                        Some(player_name),
                        song_info,
                        song_service_clone,
                    )
                })
                .await
                .map_err(|e| AppError::InternalError(format!("Blocking task join error: {e}")))?;

                let png_data = png_data_result?;
                log::info!("歌曲图片生成 - 渲染总耗时: {:?}", render_start.elapsed());

                Ok(Arc::new(png_data))
            })
            .await
            .map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

        self.song_cache_misses.fetch_add(1, AtomicOrdering::Relaxed);
        log::debug!(
            "歌曲图片缓存未命中: song_id={}, checksum={}",
            &song_id[..std::cmp::min(20, song_id.len())],
            &save_checksum[..8]
        );

        if let Err(e) = self.increment_counter("song").await {
            log::error!("更新歌曲图片计数器失败: {e}");
        }

        log::info!(
            "歌曲图片生成 - 总耗时(缓存未命中): {:?}",
            start_time.elapsed()
        );
        Ok(image_bytes_arc.to_vec())
    }

    /// 同步执行的单曲图片渲染函数
    fn _render_song_image_sync(
        full_data: FullSaveData,
        player_name: Option<String>,
        song_info: crate::models::song::SongInfo,
        song_service: web::Data<SongService>,
    ) -> Result<Vec<u8>, AppError> {
        let data_process_start = std::time::Instant::now();
        let mut all_records_sorted = full_data.rks_result.records;
        all_records_sorted.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

        let game_record_map = full_data.save.game_record.ok_or_else(|| {
            AppError::Other("存档中无成绩记录，无法生成单曲图片".to_string())
        })?;
        let song_difficulties_from_save =
            game_record_map.get(&song_info.id).cloned().unwrap_or_default();

        let difficulty_constants = song_service.get_song_difficulty(&song_info.id)?;

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
                    let target_chart_id = format!("{}-{}", song_info.id, diff_key);
                    rks_utils::calculate_target_chart_push_acc(
                        &target_chart_id,
                        dv,
                        &all_records_sorted,
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
        log::info!("歌曲图片生成 - 数据处理耗时: {:?}", data_process_start.elapsed());

        let illustration_process_start = std::time::Instant::now();
        let illustration_path_png = PathBuf::from(cover_loader::COVERS_DIR)
            .join("ill")
            .join(format!("{}.png", song_info.id));
        let illustration_path_jpg = PathBuf::from(cover_loader::COVERS_DIR)
            .join("ill")
            .join(format!("{}.jpg", song_info.id));
        let illustration_path = if illustration_path_png.exists() {
            Some(illustration_path_png)
        } else if illustration_path_jpg.exists() {
            Some(illustration_path_jpg)
        } else {
            None
        };
        log::info!("歌曲图片生成 - 插画处理耗时: {:?}", illustration_process_start.elapsed());

        let render_data_creation_start = std::time::Instant::now();
        let render_data = SongRenderData {
            song_name: song_info.song,
            song_id: song_info.id,
            player_name: player_name,
            update_time: {
                let date_str = full_data.cloud_summary["results"][0]["updatedAt"]
                    .as_str()
                    .unwrap_or_default();
                DateTime::parse_from_rfc3339(date_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now())
            },
            difficulty_scores: difficulty_scores_map,
            illustration_path,
        };
        log::info!("歌曲图片生成 - RenderData创建耗时: {:?}", render_data_creation_start.elapsed());

        let svg_gen_start = std::time::Instant::now();
        let svg_string = image_renderer::generate_song_svg_string(&render_data)?;
        log::info!("歌曲图片生成 - SVG生成耗时: {:?}", svg_gen_start.elapsed());

        let png_render_start = std::time::Instant::now();
        let result = image_renderer::render_svg_to_png(svg_string, false); // 官方数据
        log::info!("歌曲图片生成 - PNG渲染耗时: {:?}", png_render_start.elapsed());
        result
    }

    // --- 排行榜相关函数 ---

    pub async fn generate_rks_leaderboard_image(
        &self,
        limit: Option<usize>,
        player_archive_service: web::Data<PlayerArchiveService>,
    ) -> Result<Vec<u8>, AppError> {
        let start_time = std::time::Instant::now();
        let actual_limit = limit.unwrap_or(20).min(100);

        let last_update = player_archive_service
            .get_ref()
            .get_latest_rks_update_time()
            .await
            .unwrap_or_else(|_| "unknown".to_string());

        let cache_key = (actual_limit, last_update.clone());

        if let Some(cached) = self.leaderboard_image_cache.get(&cache_key).await {
            self.leaderboard_cache_hits
                .fetch_add(1, AtomicOrdering::Relaxed);
            log::debug!(
                "排行榜图片缓存命中: limit={}, update_time={}",
                actual_limit,
                &last_update[..std::cmp::min(10, last_update.len())]
            );
            log::info!(
                "排行榜图片生成 - 总耗时(缓存命中): {:?}",
                start_time.elapsed()
            );
            return Ok(cached.to_vec());
        }

        let image_bytes_arc = self
            .leaderboard_image_cache
            .try_get_with(cache_key, async {
                let top_players = player_archive_service
                    .get_ref()
                    .get_rks_ranking(actual_limit)
                    .await?;

                let permit = self.render_semaphore.clone().acquire_owned().await.map_err(|e| AppError::InternalError(format!("Failed to acquire semaphore permit: {e}")))?;

                let png_data_result = web::block(move || {
                    let _permit = permit;
                    Self::_render_rks_leaderboard_image_sync(
                        top_players,
                        actual_limit,
                    )
                })
                .await
                .map_err(|e| AppError::InternalError(format!("Blocking task join error: {e}")))?;

                let png_data = png_data_result?;
                Ok(Arc::new(png_data))
            })
            .await
            .map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

        self.leaderboard_cache_misses
            .fetch_add(1, AtomicOrdering::Relaxed);
        log::debug!(
            "排行榜图片缓存未命中: limit={}, update_time={}",
            actual_limit,
            &last_update[..std::cmp::min(10, last_update.len())]
        );

        if let Err(e) = self.increment_counter("leaderboard").await {
            log::error!("更新排行榜图片计数器失败: {e}");
        }

        log::info!(
            "排行榜图片生成 - 总耗时(缓存未命中): {:?}",
            start_time.elapsed()
        );
        Ok(image_bytes_arc.to_vec())
    }

    /// 同步执行的排行榜图片渲染函数
    fn _render_rks_leaderboard_image_sync(
        top_players: Vec<crate::models::player_archive::RKSRankingEntry>,
        actual_limit: usize,
    ) -> Result<Vec<u8>, AppError> {
        let render_data = LeaderboardRenderData {
            title: "RKS 排行榜".to_string(),
            entries: top_players,
            display_count: actual_limit,
            update_time: Utc::now(),
        };

        let svg_string = image_renderer::generate_leaderboard_svg_string(&render_data)?;
        image_renderer::render_svg_to_png(svg_string, false) // 排行榜不是用户生成的
    }
}

// 添加缓存统计方法
impl ImageService {
    pub fn get_cache_stats(&self) -> serde_json::Value {
        let bn_hits = self
            .bn_cache_hits
            .load(std::sync::atomic::Ordering::Relaxed);
        let bn_misses = self
            .bn_cache_misses
            .load(std::sync::atomic::Ordering::Relaxed);
        let bn_hit_rate = if bn_hits + bn_misses > 0 {
            format!(
                "{:.2}%",
                (bn_hits as f64 / (bn_hits + bn_misses) as f64) * 100.0
            )
        } else {
            "0.00%".to_string()
        };

        let song_hits = self
            .song_cache_hits
            .load(std::sync::atomic::Ordering::Relaxed);
        let song_misses = self
            .song_cache_misses
            .load(std::sync::atomic::Ordering::Relaxed);
        let song_hit_rate = if song_hits + song_misses > 0 {
            format!(
                "{:.2}%",
                (song_hits as f64 / (song_hits + song_misses) as f64) * 100.0
            )
        } else {
            "0.00%".to_string()
        };

        let leaderboard_hits = self
            .leaderboard_cache_hits
            .load(std::sync::atomic::Ordering::Relaxed);
        let leaderboard_misses = self
            .leaderboard_cache_misses
            .load(std::sync::atomic::Ordering::Relaxed);
        let leaderboard_hit_rate = if leaderboard_hits + leaderboard_misses > 0 {
            format!(
                "{:.2}%",
                (leaderboard_hits as f64 / (leaderboard_hits + leaderboard_misses) as f64) * 100.0
            )
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
            let counters = sqlx::query_as!(
                crate::models::image_counter::ImageCounter,
                "SELECT id, image_type, count, last_updated FROM image_counter"
            )
            .fetch_all(pool)
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

            let mut stats = serde_json::Map::new();
            for counter in counters {
                stats.insert(
                    counter.image_type,
                    serde_json::json!({
                        "count": counter.count,
                        "last_updated": counter.last_updated
                    }),
                );
            }

            Ok(serde_json::Value::Object(stats))
        } else {
            // 如果没有数据库连接，返回空对象
            Ok(serde_json::json!({}))
        }
    }

    // 获取特定类型的计数器统计信息
    pub async fn get_image_stats_by_type(
        &self,
        image_type: &str,
    ) -> Result<Option<crate::models::image_counter::ImageCounter>, AppError> {
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

    // --- 用户数据生成图片相关函数 ---

    /// 根据用户提供的成绩数据生成Best N成绩图片
    pub async fn generate_bn_image_from_user_data(
        &self,
        user_data: crate::controllers::image::UserGeneratedBnData,
        song_service: web::Data<SongService>,
    ) -> Result<Vec<u8>, AppError> {
        let start_time = std::time::Instant::now();
        log::info!("用户数据BN图片生成 - 开始处理请求: {:?}", start_time.elapsed());

        // 转换用户数据为RksRecord
        let mut rks_records = Vec::new();
        let mut song_ids = Vec::new();

        for (index, score) in user_data.scores.iter().enumerate() {
            // 使用新的 search_songs 函数来处理可能的歧义
            let search_results = song_service.search_songs(&score.song_name)?;

            // 检查搜索结果的数量
            let song_info = if search_results.len() == 1 {
                search_results.into_iter().next().unwrap()
            } else {
                // 如果找到多个或零个结果，则返回错误
                let found_songs: Vec<String> = search_results
                    .into_iter()
                    .map(|s| format!("{} ({})", s.song, s.id))
                    .collect();
                return Err(AppError::BadRequest(format!(
                    "歌曲 '{}' 存在歧义或未找到，请使用更精确的名称或歌曲ID。可能匹配: {}",
                    score.song_name,
                    found_songs.join(", ")
                )));
            };

            // 获取难度常量
            let difficulty_constants = song_service.get_song_difficulty(&song_info.id)?;

            let difficulty_value = match score.difficulty.as_str() {
                "EZ" => difficulty_constants.ez,
                "HD" => difficulty_constants.hd,
                "IN" => difficulty_constants.inl,
                "AT" => difficulty_constants.at,
                _ => return Err(AppError::BadRequest(format!(
                    "第{}条成绩的难度无效: {}",
                    index + 1, score.difficulty
                )))
            };

            if difficulty_value.is_none() || difficulty_value.unwrap() <= 0.0 {
                return Err(AppError::BadRequest(format!(
                    "歌曲 '{}' 的难度 '{}' 常量未找到或无效",
                    song_info.song, score.difficulty
                )));
            }

            let dv = difficulty_value.unwrap();

            // 计算RKS
            let rks = rks_utils::calculate_chart_rks(score.acc, dv);

            let record = RksRecord {
                song_id: song_info.id.clone(),
                song_name: song_info.song.clone(),
                difficulty: score.difficulty.clone(),
                score: Some(score.score as f64),
                acc: score.acc,
                rks,
                difficulty_value: dv,
                is_fc: false, // 用户数据不提供FC信息
            };

            rks_records.push(record);
            song_ids.push(song_info.id);
        }

        // 按RKS排序
        rks_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));

        // 计算统计数据
        let n = rks_records.len() as u32;
        let (exact_rks, _) = rks_utils::calculate_player_rks_details(&rks_records);

        // 计算AP数量和B27平均值
        let ap_scores: Vec<RksRecord> = rks_records.iter().filter(|s| s.acc == 100.0).cloned().collect();
        let ap_top_3_scores: Vec<RksRecord> = ap_scores.into_iter().take(3).collect();

        let ap_top_3_avg = if ap_top_3_scores.len() >= 3 {
            Some(ap_top_3_scores.iter().map(|s| s.rks).sum::<f64>() / 3.0)
        } else {
            None
        };

        let count_for_b27_display_avg = rks_records.len().min(27);
        let best_27_avg = if count_for_b27_display_avg > 0 {
            Some(
                rks_records
                    .iter()
                    .take(count_for_b27_display_avg)
                    .map(|s| s.rks)
                    .sum::<f64>()
                    / count_for_b27_display_avg as f64,
            )
        } else {
            None
        };

        // 计算推分ACC
        let mut push_acc_map: HashMap<String, f64> = HashMap::new();
        let top_n_scores_for_push: Vec<RksRecord> = rks_records
            .iter()
            .take(n as usize)
            .cloned()
            .collect();

        for score in top_n_scores_for_push
            .iter()
            .filter(|s| s.acc < 100.0 && s.difficulty_value > 0.0)
        {
            let key = format!("{}-{}", score.song_id, score.difficulty);
            if let Some(push_acc) = rks_utils::calculate_target_chart_push_acc(
                &key,
                score.difficulty_value,
                &top_n_scores_for_push,
            ) {
                push_acc_map.insert(key, push_acc);
            }
        }

        // 构建PlayerStats
        let stats = PlayerStats {
            ap_top_3_avg,
            best_27_avg,
            real_rks: Some(exact_rks),
            player_name: Some(user_data.player_name),
            update_time: Utc::now(),
            n,
            ap_top_3_scores,
            challenge_rank: None, // 用户数据不提供挑战模式信息
            data_string: None, // 用户数据不提供数据信息
            custom_footer_text: Some("*由玩家提供数据生成".to_string()), // 标记数据来源
            is_user_generated: true, // 用户数据
        };

        log::info!("用户数据BN图片生成 - 数据处理耗时: {:?}", start_time.elapsed());

        // 渲染图片
        let render_start = std::time::Instant::now();
        let theme = crate::controllers::image::Theme::Black; // 默认使用黑色主题

        let permit = self.render_semaphore.clone().acquire_owned().await.map_err(|e| AppError::InternalError(format!("Failed to acquire semaphore permit: {e}")))?;

        let png_data_result = web::block(move || {
            let _permit = permit;
            Self::_render_bn_image_from_user_data_sync(
                rks_records,
                stats,
                push_acc_map,
                theme,
            )
        })
        .await
        .map_err(|e| AppError::InternalError(format!("Blocking task join error: {e}")))?;

        let png_data = png_data_result?;
        log::info!("用户数据BN图片生成 - 渲染总耗时: {:?}", render_start.elapsed());

        // 更新计数器
        if let Err(e) = self.increment_counter("user-generated").await {
            log::error!("更新用户生成图片计数器失败: {e}");
        }

        log::info!("用户数据BN图片生成 - 总耗时: {:?}", start_time.elapsed());
        Ok(png_data)
    }

    /// 同步执行的用户数据BN图片渲染函数
    fn _render_bn_image_from_user_data_sync(
        rks_records: Vec<RksRecord>,
        stats: PlayerStats,
        push_acc_map: HashMap<String, f64>,
        theme: crate::controllers::image::Theme,
    ) -> Result<Vec<u8>, AppError> {
        let svg_gen_start = std::time::Instant::now();
        let svg_string = image_renderer::generate_svg_string(
            &rks_records,
            &stats,
            Some(&push_acc_map),
            &theme,
        )?;
        log::info!("用户数据BN图片生成 - SVG生成耗时: {:?}", svg_gen_start.elapsed());

        let png_render_start = std::time::Instant::now();
        let result = image_renderer::render_svg_to_png(svg_string, true); // 用户数据
        log::info!("用户数据BN图片生成 - PNG渲染耗时: {:?}", png_render_start.elapsed());
        result
    }
}
