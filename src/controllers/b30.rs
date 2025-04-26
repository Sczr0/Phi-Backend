use actix_web::{post, web, HttpResponse};

use crate::models::{ApiResponse, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::{check_session_token, calculate_b30};
use crate::utils::token_helper::resolve_token;

#[post("/b30")]
pub async fn get_b30(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    // 解析并获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;
    
    // 检查会话令牌
    check_session_token(&token)?;
    
    // 获取带难度信息的存档
    let save = phigros_service.get_save_with_difficulty(&token).await?;
    
    // 计算 B30
    let b30_result = calculate_b30(&save)?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(b30_result),
    }))
} 