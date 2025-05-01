use actix_web::{post, web, HttpResponse};
use chrono::Utc;
use log::debug;
use serde_json::Value;

use crate::models::{ApiResponse, PhigrosUser, BindRequest, IdentifierRequest, UnbindInitiateResponse};
use crate::services::user::UserService;
use crate::utils::error::{AppResult, AppError};

/// 绑定用户接口
#[post("/bind")]
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

/// 解绑用户接口 (统一处理)
#[post("/unbind")]
pub async fn unbind_user(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到解绑请求: {:?}", req);

    let qq = req.qq.as_ref().ok_or_else(|| AppError::BadRequest("必须提供QQ参数".to_string()))?;

    // 1. 尝试使用 QQ + Token 解绑
    if let Some(token) = &req.token {
        debug!("尝试使用 QQ + Token 解绑: qq={}", qq);
        let existing_user = user_service.get_user_by_qq(qq).await?; // 检查用户是否存在
        
        if existing_user.session_token == *token {
            // Token 匹配，执行删除
            user_service.delete_user(qq).await?;
            let response = ApiResponse::<Value> {
                code: 200,
                status: "OK".to_string(),
                message: Some(format!("成功通过 Token 解绑QQ {}", qq)),
                data: Some(serde_json::json!({ "unbind_method": "token" })),
            };
            return Ok(HttpResponse::Ok().json(response));
        } else {
            // Token 不匹配
            return Err(AppError::BadRequest("提供的 Token 与绑定的 Token 不匹配".to_string()));
        }
    }

    // 2. 尝试使用 QQ + verification_code 解绑
    if let Some(verification_code) = &req.verification_code {
        debug!("尝试使用验证码解绑: qq={}, code={}", qq, verification_code);
        // 验证码验证并消耗
        user_service.validate_and_consume_verification_code(qq, verification_code).await?;
        
        // 删除用户绑定
        user_service.delete_user(qq).await?;
        
        // 返回成功响应
        let response = ApiResponse::<Value> {
            code: 200,
            status: "OK".to_string(),
            message: Some(format!("成功通过验证码解绑QQ {}", qq)),
            data: Some(serde_json::json!({ "unbind_method": "verification_code" })),
        };
        return Ok(HttpResponse::Ok().json(response));
    }

    // 3. 如果只提供了 QQ，则认为是初始化验证码流程
    debug!("只提供了 QQ，初始化验证码流程: qq={}", qq);
    // 检查QQ是否存在
    user_service.get_user_by_qq(qq).await?; 
    
    // 生成验证码
    let verification = user_service.generate_and_store_verification_code(qq).await?;
    
    // 计算过期秒数
    let now = Utc::now();
    let expires_in_seconds = (verification.expires_at - now).num_seconds().max(0) as u64;
    
    // 构建响应
    let response_data = UnbindInitiateResponse {
        verification_code: verification.code,
        expires_in_seconds,
        message: format!("请在 {} 秒内将游戏内简介修改为该验证码，然后携带验证码再次请求 /unbind", expires_in_seconds),
    };
    
    let response = ApiResponse {
        code: 200,
        status: "verification_initiated".to_string(), // 状态表示需要下一步
        message: Some("验证码已生成，请修改简介并携带验证码再次请求".to_string()),
        data: Some(response_data),
    };
    Ok(HttpResponse::Ok().json(response))
} 