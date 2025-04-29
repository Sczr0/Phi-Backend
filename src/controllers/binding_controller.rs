use actix_web::{web, HttpResponse};
use chrono::Utc;
use log::debug;
use serde_json::Value;

use crate::models::{ApiResponse, PhigrosUser, BindRequest, IdentifierRequest, UnbindInitiateResponse};
use crate::services::UserService;
use crate::utils::error::{AppResult, AppError};

/// 绑定用户接口
pub async fn bind_user(
    req: web::Json<BindRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到用户绑定请求: qq={}", req.qq);
    
    // 创建用户绑定记录
    let user = PhigrosUser {
        qq: req.qq.clone(),
        session_token: req.token.clone(),
        nickname: None,  // 可以从Phigros API获取，但这里暂不实现
        last_update: Some(Utc::now().to_rfc3339()),
    };
    
    // 保存用户绑定
    user_service.save_user(user).await?;
    
    // 返回成功响应
    let response = ApiResponse::<()> {
        code: 200,
        status: "OK".to_string(),
        message: Some(format!("成功绑定QQ {} 的 SessionToken", req.qq)),
        data: None,
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// 解绑用户初始化
pub async fn unbind_initiate(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到解绑初始化请求");
    
    // 验证请求参数
    let qq = req.qq.as_ref().ok_or_else(|| 
        AppError::BadRequest("必须提供QQ参数".to_string())
    )?;
    
    // 检查QQ是否存在
    user_service.get_user_by_qq(qq).await?;
    
    // 生成验证码
    let verification = user_service.generate_and_store_verification_code(qq).await?;
    
    // 计算过期秒数
    let now = Utc::now();
    let expires_in_seconds = (verification.expires_at - now).num_seconds().max(0) as u64;
    
    // 构建响应
    let response = UnbindInitiateResponse {
        verification_code: verification.code,
        expires_in_seconds,
        message: format!("请在 {} 秒内使用该验证码完成解绑", expires_in_seconds),
    };
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: Some("验证码已生成".to_string()),
        data: Some(response),
    }))
}

/// 解绑用户完成
pub async fn unbind_complete(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到解绑完成请求");
    
    // 验证请求参数
    let qq = req.qq.as_ref().ok_or_else(|| 
        AppError::BadRequest("必须提供QQ参数".to_string())
    )?;
    
    let verification_code = req.verification_code.as_ref().ok_or_else(|| 
        AppError::BadRequest("必须提供验证码".to_string())
    )?;
    
    // 验证码验证
    user_service.validate_and_consume_verification_code(qq, verification_code).await?;
    
    // 删除用户绑定
    user_service.delete_user(qq).await?;
    
    // 返回成功响应
    let response = ApiResponse::<Value> {
        code: 200,
        status: "OK".to_string(),
        message: Some(format!("成功解绑QQ {}", qq)),
        data: Some(serde_json::json!({
            "unbind_success": true
        })),
    };
    
    Ok(HttpResponse::Ok().json(response))
} 