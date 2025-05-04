use actix_web::{post, web, HttpResponse};

use crate::models::{ApiResponse, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::services::song::SongService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::check_session_token;
use crate::utils::token_helper::resolve_token;
use serde_json::json; // 引入json宏
use tokio; // 引入 tokio

#[post("/get/cloud/saves")]
pub async fn get_cloud_saves(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    // 解析并获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;
    
    // 检查会话令牌
    check_session_token(&token)?;
    
    // 并行获取存档和 Profile
    let (save_result, profile_result) = tokio::join!(
        phigros_service.get_save(&token),
        phigros_service.get_profile(&token)
    );

    // 处理存档结果 (必须成功)
    let save_result = save_result?;
    let _save = save_result; // Prefix unused variable 'save'

    // 处理 Profile 结果 (获取失败则昵称为 None)
    let player_nickname = match profile_result {
        Ok(profile) => Some(profile.nickname),
        Err(e) => {
            log::warn!("获取用户 Profile 失败 (get_cloud_saves): {}, 将不在响应中包含昵称", e);
            None
        }
    };
    
    // --- 构建包含 score, acc, fc 的 game_record --- 
    let mut simplified_game_record = serde_json::Map::new();
    if let Some(game_record_map) = &_save.game_record {
        for (song_id, difficulties) in game_record_map {
            let mut simplified_difficulties = serde_json::Map::new();
            for (diff_name, record) in difficulties {
                simplified_difficulties.insert(diff_name.clone(), json!({
                    "score": record.score,
                    "acc": record.acc,
                    "fc": record.fc
                    // 不包含 difficulty 和 rks
                }));
            }
            simplified_game_record.insert(song_id.clone(), serde_json::Value::Object(simplified_difficulties));
        }
    }
    
    // 使用 _save 变量构建响应
    let mut response_data = json!({
        "game_key": _save.game_key,
        "game_progress": _save.game_progress,
        "game_record": if simplified_game_record.is_empty() { None } else { Some(serde_json::Value::Object(simplified_game_record)) },
        "settings": _save.settings,
        "user": _save.user
    });

    // 如果获取到昵称，添加到响应中
    if let Some(nickname) = player_nickname {
        if let Some(obj) = response_data.as_object_mut() {
            obj.insert("nickname".to_string(), json!(nickname));
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(response_data), // 返回包含昵称（如果成功获取）的 JSON 数据
    }))
}

#[post("/get/cloud/saves/with_difficulty")]
pub async fn get_cloud_saves_with_difficulty(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    _song_service: web::Data<SongService>, // Mark as unused if needed
) -> AppResult<HttpResponse> {
    let token = resolve_token(&req, &user_service).await?;
    // Call get_save_with_difficulty with only the token
    let save_result = phigros_service.get_save_with_difficulty(&token).await;
    let _save = save_result?; // Prefix unused variable
    // The actual save data is now inside _save, need to return it
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "success".to_string(),
        message: Some("成功获取并解析带定数的云存档".to_string()),
        data: Some(_save), // Return the save data
    }))
}