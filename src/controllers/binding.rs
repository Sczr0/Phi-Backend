use actix_web::{post, web, HttpResponse};
use chrono::Utc;

use crate::models::{ApiResponse, BindRequest, IdentifierRequest, PhigrosUser, UnbindInitiateResponse};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::{AppError, AppResult};
use crate::utils::save_parser::check_session_token;

#[post("/bind")]
pub async fn bind_user(
    bind_req: web::Json<BindRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    // 检查会话令牌格式
    check_session_token(&bind_req.token)?;
    
    // 检查QQ是否已绑定
    if user_service.is_qq_bound(&bind_req.qq).await? {
        return Err(AppError::BindingAlreadyExists(format!("QQ {} 已被绑定", bind_req.qq)));
    }
    
    // (移除 Token 是否已绑定的检查)
    // if user_service.is_token_bound(&bind_req.token).await? {
    //     return Err(AppError::BindingAlreadyExists(format!("此 SessionToken 已被绑定")));
    // }
    
    // 创建用户绑定
    let user = PhigrosUser {
        qq: bind_req.qq.clone(),
        session_token: bind_req.token.clone(),
        nickname: None, // 可以在这里尝试获取并填充 nickname
        last_update: Some(Utc::now().to_rfc3339()), // 记录绑定时间
    };
    
    // 保存用户绑定
    user_service.save_user(user).await?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "success".to_string(),
        message: Some("绑定成功".to_string()),
        data: None::<()>,
    }))
}

#[post("/unbind")]
pub async fn unbind_user(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
    phigros_service: web::Data<PhigrosService>,
) -> AppResult<HttpResponse> {
    match (&req.qq, &req.token, &req.verification_code) {
        // --- Mode 1: QQ + Token provided --- 
        (Some(qq), Some(token), None) => {
            log::info!("尝试通过 QQ '{}' 和 Token 解绑", qq);
            check_session_token(token)?;
            let existing_user = user_service.get_user_by_qq(qq).await?;
            if existing_user.session_token != *token {
                return Err(AppError::BadRequest("QQ号与SessionToken不匹配".to_string()));
            }
            user_service.delete_user(qq).await?;
            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some("解绑成功 (QQ+Token验证)".to_string()),
                data: None::<()>,
            }))
        }
        
        // --- Mode 2, Step 1: QQ provided, initiate profile verification --- 
        (Some(qq), None, None) => {
            log::info!("尝试通过 QQ '{}' 发起简介验证解绑", qq);
            // Check if QQ is actually bound first
            let _ = user_service.get_user_by_qq(qq).await?; 
            
            let code_details = user_service.generate_and_store_verification_code(qq).await?;
            let expires_in = (code_details.expires_at - Utc::now()).num_seconds();
            let response = UnbindInitiateResponse {
                verification_code: code_details.code,
                expires_in_seconds: expires_in.max(0) as u64,
                message: format!("请在 {} 秒内将您的 Phigros 简介修改为此验证码，然后再次调用此接口并附带 code 参数进行确认。", expires_in.max(0)),
            };
            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200, // Use 200 OK for initiation
                status: "verification_initiated".to_string(),
                message: Some(response.message.clone()),
                data: Some(response),
            }))
        }
        
        // --- Mode 2, Step 2: QQ + Verification Code provided --- 
        (Some(qq), None, Some(code)) => {
            log::info!("尝试通过 QQ '{}' 和验证码 '{}' 确认解绑", qq, code);
            
            // 1. Validate and consume the verification code first
            user_service.validate_and_consume_verification_code(qq, code).await?;
            
            // If code is valid, proceed to check profile
            // 2. Get stored token
            let stored_token = match user_service.get_user_by_qq(qq).await {
                 Ok(user) => user.session_token,
                 // This shouldn't happen if validation passed, but handle defensively
                 Err(e) => return Err(e), 
            };
            
            // 3. Get save using stored token
            log::debug!("使用存储的 Token 获取 QQ '{}' 的存档进行简介核对", qq);
            match phigros_service.get_save(&stored_token).await {
                Ok(save) => {
                    // 4. Verify profile intro against the *provided* code
                    let user_intro: Option<String> = save.user
                        .as_ref() // 获取 Option<&HashMap> 
                        .and_then(|user_map| user_map.get("selfIntro")) // 获取 Option<&Value>
                        .and_then(|value| value.as_str()) // 获取 Option<&str>
                        .map(|s| s.to_string()); // 转换为 Option<String>

                    if let Some(intro) = user_intro {
                         if intro.trim() == code.trim() {
                             log::info!("简介内容与验证码匹配成功 for QQ '{}'", qq);
                             // 5. Execute unbind
                             user_service.delete_user(qq).await?;
                             Ok(HttpResponse::Ok().json(ApiResponse {
                                 code: 200,
                                 status: "success".to_string(),
                                 message: Some("解绑成功 (简介验证)".to_string()),
                                 data: None::<()>,
                             }))
                         } else {
                             log::warn!("简介验证失败 for QQ '{}'. Expected code '{}', got intro '{}'", qq, code.trim(), intro.trim());
                             Err(AppError::ProfileVerificationFailed("简介内容与提供的验证码不匹配".to_string()))
                         }
                    } else {
                         log::warn!("简介验证失败 for QQ '{}': 简介为空或类型不正确", qq);
                         Err(AppError::ProfileVerificationFailed("简介为空或无法读取，无法验证".to_string()))
                    }
                }
                Err(AppError::InvalidSessionToken) => {
                     log::warn!("存储的 Token 无效，无法获取存档核对简介 QQ '{}'", qq);
                     Err(AppError::TokenVerificationFailed("无法获取存档核对简介 (Token已失效)，请稍后再试或使用QQ+有效Token解绑".to_string()))
                }
                 Err(AppError::ReqwestError(e)) => {
                     log::error!("获取存档时网络错误 for QQ '{}': {}", qq, e);
                     Err(AppError::Other(format!("获取存档时网络错误: {}", e)))
                 }
                Err(e) => {
                     log::error!("获取存档时发生意外错误 for QQ '{}': {}", qq, e);
                     Err(e)
                }
            }
        }
        
        // --- Invalid Combinations --- 
        _ => {
             log::warn!("无效的解绑请求参数组合: qq={:?}, token={:?}, code={:?}", req.qq, req.token, req.verification_code);
             Err(AppError::BadRequest("无效的请求参数组合。请提供: (QQ和Token) 或 (仅QQ发起验证) 或 (QQ和验证码确认)".to_string()))
        }
    }
} 