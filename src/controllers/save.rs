use actix_web::{post, web, HttpResponse};

use crate::models::{ApiResponse, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::check_session_token;
use crate::utils::token_helper::resolve_token;
use serde_json::json; // 引入json宏

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
    
    // 获取并解析存档
    let save = phigros_service.get_save(&token).await?;
    
    // --- 构建包含 score, acc, fc 的 game_record --- 
    let mut simplified_game_record = serde_json::Map::new();
    if let Some(game_record_map) = &save.game_record {
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
    
    // 将原始 GameSave 结构体中的 game_record 替换为 Value::Null 或其他标记，避免默认序列化
    // 或者创建一个新的响应结构体只包含需要的字段。这里我们直接修改返回的JSON。
    let response_data = json!({
        "game_key": save.game_key,
        "game_progress": save.game_progress,
        "game_record": if simplified_game_record.is_empty() { None } else { Some(serde_json::Value::Object(simplified_game_record)) }, // 使用简化后的 game_record
        "settings": save.settings,
        "user": save.user
    });

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(response_data), // 返回手动构建的 JSON 数据
    }))
}

#[post("/get/cloud/saves/with_difficulty")]
pub async fn get_cloud_saves_with_difficulty(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    // 解析并获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;
    
    // 检查会话令牌
    check_session_token(&token)?;
    
    // 获取并解析存档，带有难度和RKS信息
    let save = phigros_service.get_save_with_difficulty(&token).await?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(save),
    }))
}