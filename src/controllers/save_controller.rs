use actix_web::{web, HttpResponse};
use log::debug;

use crate::models::{ApiResponse, user::IdentifierRequest};
use crate::services::{PhigrosService, UserService};
use crate::utils::error::AppResult;
use crate::utils::token_helper::resolve_token;

/// 获取原始云存档（不含难度定数和RKS）
pub async fn get_cloud_saves(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到获取云存档请求");
    
    // 解析token
    let token = resolve_token(&req, &user_service).await?;
    
    // 获取存档
    let save = phigros_service.get_save(&token).await?;
    
    // 返回成功响应
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(save),
    }))
}

/// 获取带难度定数的云存档
pub async fn get_cloud_saves_with_difficulty(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到获取带难度定数的云存档请求");
    
    // 解析token
    let token = resolve_token(&req, &user_service).await?;
    
    // 获取带难度信息的存档
    let save = phigros_service.get_save_with_difficulty(&token).await?;
    
    // 返回成功响应
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(save),
    }))
} 