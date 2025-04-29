use actix_web::{web, HttpResponse};
use log::debug;

use crate::models::{ApiResponse, user::IdentifierRequest};
use crate::services::{PhigrosService, UserService};
use crate::utils::error::AppResult;
use crate::utils::token_helper::resolve_token;

/// 计算RKS
pub async fn calculate_rks(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到RKS计算请求");
    
    // 解析token
    let token = resolve_token(&req, &user_service).await?;
    
    // 计算RKS
    let rks_result = phigros_service.get_rks(&token).await?;
    
    // 返回成功响应
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(rks_result),
    }))
}

/// 获取B30成绩
pub async fn get_b30(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到B30查询请求");
    
    // 解析token
    let token = resolve_token(&req, &user_service).await?;
    
    // 计算RKS
    let rks_result = phigros_service.get_rks(&token).await?;
    
    // 取前30条记录
    let b30 = rks_result.records.into_iter().take(30).collect::<Vec<_>>();
    
    // 返回成功响应
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(b30),
    }))
}

/// 获取Bn成绩
pub async fn get_bn(
    n: web::Path<u32>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    let n = n.into_inner();
    debug!("接收到B{}查询请求", n);
    
    // 参数验证
    if n == 0 {
        return Ok(HttpResponse::Ok().json(ApiResponse {
            code: 400,
            status: "ERROR".to_string(),
            message: Some("参数n必须大于0".to_string()),
            data: None::<Vec<()>>,
        }));
    }
    
    // 解析token
    let token = resolve_token(&req, &user_service).await?;
    
    // 计算RKS
    let rks_result = phigros_service.get_rks(&token).await?;
    
    // 取前n条记录
    let bn = rks_result.records.into_iter().take(n as usize).collect::<Vec<_>>();
    
    // 返回成功响应
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(bn),
    }))
} 