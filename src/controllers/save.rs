use actix_web::{post, web, HttpResponse};
use log::debug;
use utoipa;

use crate::models::save::GameSave;
use crate::models::user::{ApiResponse, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::check_session_token;
use crate::utils::token_helper::resolve_token;
use serde_json::json;
use tokio;

/// 获取云存档（不含难度）
///
/// 获取玩家的原始云存档，并附加玩家昵称。
/// 返回的 `game_record` 被简化，只包含 `score`, `acc`, `fc`。
#[utoipa::path(
    post,
    path = "/get/cloud/saves",
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功获取云存档", body = ApiResponse<serde_json::Value>)
    )
)]
#[post("/get/cloud/saves")]
pub async fn get_cloud_saves(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    let (save_result, profile_result) = if req.data_source.as_deref() == Some("external") {
        // 外部数据源：直接获取存档，不需要profile
        let save_result = phigros_service.get_save_with_source(&req).await;
        (save_result, Ok(crate::models::user::UserProfile {
            object_id: "external".to_string(),
            nickname: req.platform.as_ref()
                .map(|p| format!("{}:{}", p, req.platform_id.as_ref().unwrap_or(&"unknown".to_string())))
                .unwrap_or_else(|| "External User".to_string())
        }))
    } else {
        // 内部数据源：并行获取数据
        let token = resolve_token(&req, &user_service).await?;
        check_session_token(&token)?;

        tokio::join!(
            phigros_service.get_save_with_source(&req),
            phigros_service.get_profile(&token)
        )
    };

    let save_data = save_result?;

    let player_nickname = match profile_result {
        Ok(profile) => Some(profile.nickname),
        Err(e) => {
            log::warn!("获取用户 Profile 失败 (get_cloud_saves): {e}, 将不在响应中包含昵称");
            None
        }
    };

    let mut simplified_game_record = serde_json::Map::new();
    if let Some(game_record_map) = &save_data.game_record {
        for (song_id, difficulties) in game_record_map {
            let mut simplified_difficulties = serde_json::Map::new();
            for (diff_name, record) in difficulties {
                simplified_difficulties.insert(
                    diff_name.clone(),
                    json!({
                        "score": record.score,
                        "acc": record.acc,
                        "fc": record.fc
                    }),
                );
            }
            simplified_game_record.insert(
                song_id.clone(),
                serde_json::Value::Object(simplified_difficulties),
            );
        }
    }

    let mut response_data = json!({
        "game_key": save_data.game_key,
        "game_progress": save_data.game_progress,
        "game_record": if simplified_game_record.is_empty() { None } else { Some(serde_json::Value::Object(simplified_game_record)) },
        "settings": save_data.settings,
        "user": save_data.user
    });

    if let Some(nickname) = player_nickname {
        if let Some(obj) = response_data.as_object_mut() {
            obj.insert("nickname".to_string(), json!(nickname));
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(response_data),
    }))
}

/// 获取带难度定数的云存档
///
/// 获取玩家的完整云存档，其中包含了每首歌每个难度的定数信息。
#[utoipa::path(
    post,
    path = "/get/cloud/saves/with_difficulty",
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功获取带难度定数的云存档", body = ApiResponse<GameSave>)
    )
)]
#[post("/get/cloud/saves/with_difficulty")]
pub async fn get_cloud_saves_with_difficulty(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到获取带难度定数的云存档请求");

    let _token = if req.data_source.as_deref() == Some("external") {
        // 外部数据源：使用占位符token
        "external_placeholder_token".to_string()
    } else {
        // 内部数据源：解析真实token
        resolve_token(&req, &user_service).await?
    };

    let save = phigros_service.get_save_with_difficulty_and_source(&req).await?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(save),
    }))
}
/// 获取原始的云存档元数据 (saveInfo)
///
/// 直接从 Phigros 服务器获取并返回原始的 `saveInfo` JSON 对象。
/// 这个对象包含了存档文件的URL、校验和、更新时间等元数据。
#[utoipa::path(
    post,
    path = "/get/cloud/saveInfo",
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功获取原始云存档元数据", body = ApiResponse<serde_json::Value>)
    )
)]
#[post("/get/cloud/saveInfo")]
pub async fn get_cloud_save_info(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到获取原始云存档元数据 (saveInfo) 的请求");

    let token = resolve_token(&req, &user_service).await?;
    check_session_token(&token)?;

    let save_info = phigros_service.get_cloud_save_info(&token).await?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(save_info),
    }))
}
