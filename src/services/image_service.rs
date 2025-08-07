use crate::models::rks::RksRecord;
use crate::models::user::IdentifierRequest;
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::services::song::SongService;
use crate::utils::error::AppError;
use crate::utils::image_renderer::{self, PlayerStats, SongRenderData, SongDifficultyScore};
use crate::utils::cover_loader;
use crate::utils::rks_utils;
use chrono::Utc;
use actix_web::web;
use std::cmp::Ordering;
use std::collections::HashMap;
use tokio;
use std::path::PathBuf;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::utils::image_renderer::LeaderboardRenderData;
use crate::utils::token_helper::resolve_token;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

// --- ImageService 结构体定义 ---

pub struct ImageService {
    bn_image_cache: Cache<(u32, String, crate::controllers::image::Theme), Arc<Vec<u8>>>,
    song_image_cache: Cache<(String, String), Arc<Vec<u8>>>,
    leaderboard_image_cache: Cache<usize, Arc<Vec<u8>>>,
}

impl ImageService {
    pub fn new() -> Self {
        Self {
            // B-side图片缓存：最多缓存1000张，每张图片缓存10分钟
            bn_image_cache: Cache::builder()
                .max_capacity(1000)
                .time_to_live(Duration::from_secs(10 * 60))
                .build(),
            // 歌曲图片缓存：最多缓存1000张，每张图片缓存10分钟
            song_image_cache: Cache::builder()
                .max_capacity(1000)
                .time_to_live(Duration::from_secs(10 * 60))
                .build(),
            // 排行榜图片缓存：最多缓存100张，每张图片缓存5分钟
            leaderboard_image_cache: Cache::builder()
                .max_capacity(100)
                .time_to_live(Duration::from_secs(5 * 60))
                .build(),
        }
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
        let cache_key = (n, token.clone(), theme.clone());

        let image_bytes_arc = self.bn_image_cache.try_get_with(
            cache_key,
            async {
                let (rks_save_res, profile_res) = tokio::join!(
                    phigros_service.get_rks(&token),
                    phigros_service.get_profile(&token)
                );

                let (all_rks_result, save) = rks_save_res?;
                if all_rks_result.records.is_empty() {
                    return Err(AppError::Other(format!("用户无成绩记录，无法生成 B{} 图片", n)));
                }
                let all_scores = all_rks_result.records;

                let player_id = save.user.as_ref()
                    .and_then(|u| u.get("objectId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let player_nickname = profile_res.ok().map(|p| p.nickname);

                let player_name_for_archive = player_nickname.clone().unwrap_or_else(|| player_id.clone());

                let mut fc_map = HashMap::new();
                if let Some(game_record_map) = &save.game_record {
                    for (song_id, difficulties) in game_record_map {
                        for (diff_name, record) in difficulties {
                            if record.fc == Some(true) {
                                fc_map.insert(format!("{}-{}", song_id, diff_name), true);
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
                    if let Err(e) = archive_service_clone.update_player_scores_from_rks_records(
                        &player_id_clone,
                        &player_name_clone,
                        &scores_clone,
                        &fc_map_clone,
                    ).await {
                        log::error!("后台更新玩家 {} ({}) 存档失败: {}", player_name_clone, player_id_clone, e);
                    }
                });

                let mut sorted_scores = all_scores;
                sorted_scores.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

                let (exact_rks, _rounded_rks) = rks_utils::calculate_player_rks_details(&sorted_scores);

                let top_n_scores = sorted_scores.iter().take(n as usize).cloned().collect::<Vec<_>>();

                let ap_scores_ranked: Vec<_> = sorted_scores.iter().filter(|s| s.acc == 100.0).collect();
                let ap_top_3_scores: Vec<RksRecord> = ap_scores_ranked.iter().take(3).map(|&s| s.clone()).collect();
                
                let ap_top_3_avg = if ap_top_3_scores.len() >= 3 {
                    Some(ap_top_3_scores.iter().map(|s| s.rks).sum::<f64>() / 3.0)
                } else {
                    None
                };
                
                let count_for_b27_display_avg = sorted_scores.len().min(27);
                let best_27_avg = if count_for_b27_display_avg > 0 {
                    Some(sorted_scores.iter().take(count_for_b27_display_avg).map(|s| s.rks).sum::<f64>() / count_for_b27_display_avg as f64)
                } else {
                    None
                };

                let push_acc_map: HashMap<String, f64> = top_n_scores.iter()
                    .filter(|score| score.acc < 100.0 && score.difficulty_value > 0.0)
                    .filter_map(|score| {
                        let target_chart_id_full = format!("{}-{}", score.song_id, score.difficulty);
                        rks_utils::calculate_target_chart_push_acc(&target_chart_id_full, score.difficulty_value, &sorted_scores)
                            .map(|push_acc| (target_chart_id_full, push_acc))
                    })
                    .collect();

                let stats = PlayerStats {
                    ap_top_3_avg,
                    best_27_avg,
                    real_rks: Some(exact_rks),
                    player_name: player_nickname,
                    update_time: Utc::now(),
                    n,
                    ap_top_3_scores,
                };
                
                let theme_clone = theme.clone();
                let png_data = web::block(move || {
                    let svg_string = image_renderer::generate_svg_string(&top_n_scores, &stats, Some(&push_acc_map), &theme_clone)?;
                    image_renderer::render_svg_to_png(svg_string)
                })
                .await
                .map_err(|e| AppError::InternalError(format!("Blocking task join error: {}", e)))??;

                Ok(Arc::new(png_data))
            }
        ).await.map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

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
        let cache_key = (song_query.clone(), token.clone());

        let image_bytes_arc = self.song_image_cache.try_get_with(
            cache_key,
            async {
                let song_info = song_service.search_song(&song_query)?;
                let song_id = song_info.id.clone();
                let song_name = song_info.song.clone();

                let (rks_save_res, profile_res) = tokio::join!(
                    phigros_service.get_rks(&token),
                    phigros_service.get_profile(&token)
                );

                let (all_rks_result, save) = rks_save_res?;
                let mut all_records = all_rks_result.records;

                let player_id = save.user.as_ref()
                    .and_then(|u| u.get("objectId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let player_nickname = profile_res.ok().map(|p| p.nickname);
                let player_name_for_archive = player_nickname.clone().unwrap_or_else(|| player_id.clone());

                let fc_map: HashMap<String, bool> = if let Some(game_record_map) = &save.game_record {
                    game_record_map.iter().flat_map(|(song_id, difficulties)| {
                        difficulties.iter().filter_map(move |(diff_name, record)| {
                            if record.fc == Some(true) {
                                Some((format!("{}-{}", song_id, diff_name), true))
                            } else {
                                None
                            }
                        })
                    }).collect()
                } else {
                    HashMap::new()
                };

                let archive_service_clone = player_archive_service.clone();
                let player_id_clone = player_id.clone();
                let player_name_clone = player_name_for_archive.clone();
                let records_clone = all_records.clone();
                let fc_map_clone = fc_map.clone();

                tokio::spawn(async move {
                     if let Err(e) = archive_service_clone.update_player_scores_from_rks_records(
                        &player_id_clone,
                        &player_name_clone,
                        &records_clone,
                        &fc_map_clone,
                    ).await {
                        log::error!("后台更新玩家 {} ({}) 存档失败: {}", player_name_clone, player_id_clone, e);
                    }
                });

                all_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

                let game_record_map = save.game_record.ok_or_else(|| AppError::Other("存档中无成绩记录，无法生成单曲图片".to_string()))?;
                let song_difficulties_from_save = game_record_map.get(&song_id).cloned().unwrap_or_default();
                
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
                    let is_phi = acc.map_or(false, |a| a == 100.0);

                    let push_acc = if let Some(dv) = difficulty_value {
                        if dv > 0.0 && !is_phi {
                            let target_chart_id = format!("{}-{}", song_id, diff_key);
                            rks_utils::calculate_target_chart_push_acc(&target_chart_id, dv, &all_records)
                        } else { Some(100.0) }
                    } else { Some(100.0) };

                    difficulty_scores_map.insert(diff_key.to_string(), Some(SongDifficultyScore {
                        score: record.and_then(|r| r.score),
                        acc,
                        rks: record.and_then(|r| r.rks),
                        difficulty_value,
                        is_fc: record.and_then(|r| r.fc),
                        is_phi: Some(is_phi),
                        player_push_acc: push_acc,
                    }));
                }

                let illustration_path_png = PathBuf::from(cover_loader::COVERS_DIR).join("ill").join(format!("{}.png", song_id));
                let illustration_path_jpg = PathBuf::from(cover_loader::COVERS_DIR).join("ill").join(format!("{}.jpg", song_id));
                let illustration_path = if illustration_path_png.exists() {
                    Some(illustration_path_png)
                } else if illustration_path_jpg.exists() {
                    Some(illustration_path_jpg)
                } else {
                    None
                };
                
                let render_data = SongRenderData {
                    song_name,
                    song_id,
                    player_name: player_nickname,
                    update_time: Utc::now(),
                    difficulty_scores: difficulty_scores_map,
                    illustration_path,
                };
                
                let png_data = web::block(move || {
                    let svg_string = image_renderer::generate_song_svg_string(&render_data)?;
                    image_renderer::render_svg_to_png(svg_string)
                })
                .await.map_err(|e| AppError::InternalError(format!("Blocking task join error: {}", e)))??;

                Ok(Arc::new(png_data))
            }
        ).await.map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;

        Ok(image_bytes_arc.to_vec())
    }


// --- 排行榜相关函数 ---

    pub async fn generate_rks_leaderboard_image(
        &self,
        limit: Option<usize>, // 显示多少名玩家，默认 20
        player_archive_service: web::Data<PlayerArchiveService>,
    ) -> Result<Vec<u8>, AppError> {
        let actual_limit = limit.unwrap_or(20).min(100);
        let cache_key = actual_limit;

        let image_bytes_arc = self.leaderboard_image_cache.try_get_with(
            cache_key,
            async {
                log::info!("生成RKS排行榜图片，显示前{}名玩家", actual_limit);
        
                let top_players = player_archive_service.get_ref().get_rks_ranking(actual_limit).await?;
                
                let render_data = LeaderboardRenderData {
                    title: "RKS 排行榜".to_string(),
                    entries: top_players,
                    display_count: actual_limit,
                    update_time: Utc::now(),
                };
                
                let svg_string = image_renderer::generate_leaderboard_svg_string(&render_data)?;
                
                let png_data = web::block(move || image_renderer::render_svg_to_png(svg_string))
                    .await
                    .map_err(|e| AppError::InternalError(format!("Blocking task error for leaderboard: {}", e)))??;

                Ok(Arc::new(png_data))
            }
        ).await.map_err(|e: Arc<AppError>| AppError::InternalError(e.to_string()))?;
        
        Ok(image_bytes_arc.to_vec())
    }

}