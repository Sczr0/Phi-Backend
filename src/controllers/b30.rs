use actix_web::{post, web, HttpResponse};
use std::collections::HashMap;
use utoipa;

use crate::models::b30::B30Result;
use crate::models::cloud_save::FullSaveData;
use crate::models::user::{ApiResponse, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::services::user::UserService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::{calculate_b30, check_session_token};
use crate::utils::token_helper::resolve_token;
use tokio;

/// 计算并返回玩家的B30成绩
#[utoipa::path(
    post,
    path = "/b30",
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功计算B30", body = ApiResponse<B30Result>)
    )
)]
#[post("/b30")]
pub async fn get_b30(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    player_archive_service: web::Data<PlayerArchiveService>,
) -> AppResult<HttpResponse> {
    // 解析并获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;

    // 检查会话令牌
    check_session_token(&token)?;

    // 根据数据源获取完整存档数据
    let full_data = if req.data_source.as_deref() == Some("external") {
        // 外部数据源：直接获取完整数据
        phigros_service.get_full_save_data_with_source(&req).await?
    } else {
        // 内部数据源：并行获取数据
        let (full_data_res, profile_res) = tokio::join!(
            phigros_service.get_full_save_data_with_source(&req),
            phigros_service.get_profile(&token)
        );

        let full_data = full_data_res?;

        // 获取玩家ID和昵称（内部数据源）
        let player_id = full_data.save
            .user
            .as_ref()
            .and_then(|u| u.get("objectId"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let player_name = match profile_res {
            Ok(profile) => profile.nickname,
            Err(e) => {
                log::warn!("获取用户 Profile 失败 (get_b30): {e}, 将使用 Player ID 作为名称");
                player_id.clone()
            }
        };

        // 构造包含玩家信息的完整数据
        let mut modified_summary = full_data.cloud_summary.clone();
        if let Some(results) = modified_summary.get_mut("results") {
            if let Some(results_array) = results.as_array_mut() {
                if let Some(result_obj) = results_array.get_mut(0) {
                    if let Some(obj) = result_obj.as_object_mut() {
                        obj.insert("PlayerId".to_string(), serde_json::Value::String(player_id.clone()));
                        obj.insert("nickname".to_string(), serde_json::Value::String(player_name.clone()));
                    }
                }
            }
        }

        FullSaveData {
            rks_result: full_data.rks_result,
            save: full_data.save,
            cloud_summary: modified_summary,
        }
    };

    let rks_result = full_data.rks_result;
    let save = full_data.save;

    // 从完整数据中提取玩家信息
    let summary = &full_data.cloud_summary["results"][0];
    let player_id = summary["PlayerId"]
        .as_str()
        .unwrap_or("external:unknown")
        .to_string();
    let player_name = summary["nickname"]
        .as_str()
        .unwrap_or(&player_id)
        .to_string();

    // 从存档构建 FC Map
    let mut fc_map = HashMap::new();
    if let Some(game_record_map) = &save.game_record {
        for (song_id, difficulties) in game_record_map {
            for (diff_name, record) in difficulties {
                if let Some(true) = record.fc {
                    let key = format!("{song_id}-{diff_name}");
                    fc_map.insert(key, true);
                }
            }
        }
    }

    // 更新数据库中的玩家存档和 RKS
    let archive_service_clone = player_archive_service.clone();
    let player_id_clone = player_id.clone();
    let player_name_clone = player_name.clone();
    let records_clone = rks_result.records.clone();
    let fc_map_clone = fc_map.clone();

    tokio::spawn(async move {
        log::info!("[后台任务] (get_b30) 开始为玩家 {player_name_clone} ({player_id_clone}) 更新数据库存档...");
        let is_external = req.data_source.as_deref() == Some("external");
        match archive_service_clone
            .update_player_scores_from_rks_records(
                &player_id_clone,
                &player_name_clone,
                &records_clone,
                &fc_map_clone,
                is_external,
            )
            .await
        {
            Ok(_) => log::info!("[后台任务] (get_b30) 玩家 {player_name_clone} ({player_id_clone}) 数据库存档更新完成。"),
            Err(e) => log::error!("[后台任务] (get_b30) 更新玩家 {player_name_clone} ({player_id_clone}) 数据库存档失败: {e}"),
        }
    });

    // 计算 B30
    let b30_result = calculate_b30(&save)?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(b30_result),
    }))
}
