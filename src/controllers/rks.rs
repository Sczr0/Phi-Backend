use actix_web::{post, web, HttpResponse};
use std::collections::HashMap;

use crate::models::{ApiResponse, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::check_session_token;
use crate::utils::token_helper::resolve_token;
use tokio;

#[post("/rks")]
pub async fn get_rks(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    player_archive_service: web::Data<PlayerArchiveService>,
) -> AppResult<HttpResponse> {
    // 解析并获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;
    
    // 检查会话令牌
    check_session_token(&token)?;
    
    // (优化后) 并行获取 RKS列表+存档 和 Profile
    let (rks_save_res, profile_res) = tokio::join!(
        phigros_service.get_rks(&token), // get_rks 现在返回 (RksResult, GameSave)
        phigros_service.get_profile(&token)
    );

    // 解包结果
    let (rks_result, save) = rks_save_res?;
    // let save = save_res?; // 不再需要单独获取 save

    // 获取玩家ID和昵称 (使用从 get_rks 返回的 save)
    let player_id = save.user.as_ref()
        .and_then(|u| u.get("objectId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let player_name = match profile_res {
        Ok(profile) => profile.nickname,
        Err(e) => {
            log::warn!("获取用户 Profile 失败 (get_rks): {}, 将使用 Player ID 作为名称", e);
            player_id.clone()
        }
    };

    // 从存档构建 FC Map
    let mut fc_map = HashMap::new();
    if let Some(game_record_map) = &save.game_record {
        for (song_id, difficulties) in game_record_map {
            for (diff_name, record) in difficulties {
                if let Some(true) = record.fc {
                    let key = format!("{}-{}", song_id, diff_name);
                    fc_map.insert(key, true);
                }
            }
        }
    }

    // 更新数据库中的玩家存档和 RKS (异步执行)
    let archive_service_clone = player_archive_service.clone();
    let player_id_clone = player_id.clone();
    let player_name_clone = player_name.clone();
    let records_clone = rks_result.records.clone();
    let fc_map_clone = fc_map.clone();

    tokio::spawn(async move {
        log::info!("[后台任务] (get_rks) 开始为玩家 {} ({}) 更新数据库存档...", player_name_clone, player_id_clone);
        match archive_service_clone.update_player_scores_from_rks_records(
            &player_id_clone,
            &player_name_clone,
            &records_clone,
            &fc_map_clone
        ).await {
            Ok(_) => log::info!("[后台任务] (get_rks) 玩家 {} ({}) 数据库存档更新完成。", player_name_clone, player_id_clone),
            Err(e) => log::error!("[后台任务] (get_rks) 更新玩家 {} ({}) 数据库存档失败: {}", player_name_clone, player_id_clone, e),
        }
    });

    // 注意：API 仍然只返回 RksResult
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(rks_result), 
    }))
} 