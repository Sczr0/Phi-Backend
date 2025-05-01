use actix_web::{post, web, HttpResponse};
use chrono::Utc;
use serde_json::json;

use crate::models::{ApiResponse, BindRequest, IdentifierRequest, PlatformBinding, TokenListResponse, UnbindInitiateResponse};
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
    
    // 标准化平台名称为小写，以确保大小写不敏感
    let platform = bind_req.platform.to_lowercase();
    let platform_id = bind_req.platform_id.clone();
    
    // 创建变量存储绑定结果的内部ID
    let internal_id: String;
    
    // 检查平台ID是否已绑定到其他Token
    if user_service.is_platform_id_bound(&platform, &platform_id).await? {
        // 获取当前绑定信息
        let existing_binding = user_service.get_binding_by_platform_id(&platform, &platform_id).await?;
        internal_id = existing_binding.internal_id.clone();
        
        // 如果提供了不同的Token，更新绑定
        if existing_binding.session_token != bind_req.token {
            user_service.update_platform_binding_token(&platform, &platform_id, &bind_req.token).await?;
            
            return Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!("已更新平台 {} 的 ID {} 的Token", platform, platform_id)),
                data: Some(json!({
                    "internal_id": internal_id
                })),
            }));
        } else {
            return Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!("平台 {} 的 ID {} 已绑定到同一Token", platform, platform_id)),
                data: Some(json!({
                    "internal_id": internal_id
                })),
            }));
        }
    }
    
    // 检查token是否已绑定
    match user_service.get_binding_by_token(&bind_req.token).await {
        // 如果token已绑定到其他平台账号
        Ok(existing_binding) => {
            // 将当前平台ID也绑定到相同的内部ID
            internal_id = existing_binding.internal_id.clone();
            let binding = PlatformBinding::new(
                internal_id.clone(),
                platform.clone(),
                platform_id.clone(),
                bind_req.token.clone()
            );
            user_service.save_platform_binding(&binding).await?;
            
            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!("平台 {} 的 ID {} 已绑定到现有内部用户", platform, platform_id)),
                data: Some(json!({
                    "internal_id": internal_id
                })),
            }))
        },
        // 如果token未绑定
        Err(AppError::UserBindingNotFound(_)) => {
            // 创建新的内部用户
            internal_id = user_service.get_or_create_internal_id_by_token(
                &bind_req.token, 
                &platform, 
                &platform_id
            ).await?;
            
            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!("平台 {} 的 ID {} 已成功绑定", platform, platform_id)),
                data: Some(json!({
                    "internal_id": internal_id
                })),
            }))
        },
        Err(e) => Err(e)
    }
}

#[post("/token/list")]
pub async fn list_tokens(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    let platform = req.platform.as_ref().map(|p| p.to_lowercase());
    
    let internal_id = match (&req.token, &platform, &req.platform_id) {
        (Some(token), _, _) => {
            // 通过token获取内部ID
            let binding = user_service.get_binding_by_token(token).await?;
            binding.internal_id
        },
        (_, Some(platform), Some(platform_id)) => {
            // 通过平台和平台ID获取内部ID
            let binding = user_service.get_binding_by_platform_id(platform, platform_id).await?;
            binding.internal_id
        },
        _ => {
            return Err(AppError::BadRequest("请提供token或平台信息".to_string()));
        }
    };
    
    // 获取内部ID的所有绑定信息
    let token_list = user_service.get_token_list(&internal_id).await?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "success".to_string(),
        message: Some("获取Token列表成功".to_string()),
        data: Some(token_list),
    }))
}

#[post("/unbind")]
pub async fn unbind_user(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
    phigros_service: web::Data<PhigrosService>,
) -> AppResult<HttpResponse> {
    // 需要同时提供平台和平台ID
    let platform = req.platform.as_ref().map(|p| p.to_lowercase());
    let (platform, platform_id) = match (&platform, &req.platform_id) {
        (Some(p), Some(id)) => (p.clone(), id.clone()),
        _ => return Err(AppError::BadRequest("必须提供平台和平台ID".to_string())),
    };
    
    match (&req.token, &req.verification_code) {
        // --- Mode 1: Platform + ID + Token provided --- 
        (Some(token), None) => {
            log::info!("尝试通过平台 '{}' 的 ID '{}' 和 Token 解绑", platform, platform_id);
            check_session_token(token)?;
            
            // 验证token是否与平台ID绑定匹配
            let binding = user_service.get_binding_by_platform_id(&platform, &platform_id).await?;
            if binding.session_token != *token {
                return Err(AppError::BadRequest("平台ID与SessionToken不匹配".to_string()));
            }
            
            // 解绑并返回关联的内部ID
            let internal_id = user_service.delete_platform_binding(&platform, &platform_id).await?;
            
            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some("解绑成功 (平台ID+Token验证)".to_string()),
                data: Some(json!({
                    "internal_id": internal_id
                })),
            }))
        },
        
        // --- Mode 2, Step 1: Platform + ID provided, initiate profile verification --- 
        (None, None) => {
            log::info!("尝试通过平台 '{}' 的 ID '{}' 发起简介验证解绑", platform, platform_id);
            
            // 检查平台ID是否已绑定
            let binding = user_service.get_binding_by_platform_id(&platform, &platform_id).await?;
            let internal_id = binding.internal_id.clone();
            
            // 生成验证码
            let code_details = user_service.generate_and_store_verification_code(&platform, &platform_id).await?;
            let expires_in = (code_details.expires_at - Utc::now()).num_seconds();
            let response = UnbindInitiateResponse {
                verification_code: code_details.code,
                expires_in_seconds: expires_in.max(0) as u64,
                message: format!("请在 {} 秒内将您的 Phigros 简介修改为此验证码，然后再次调用此接口并附带 verification_code 参数进行确认。", expires_in.max(0)),
            };
            
            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "verification_initiated".to_string(),
                message: Some(response.message.clone()),
                data: Some(json!({
                    "verification": response,
                    "internal_id": internal_id
                })),
            }))
        },
        
        // --- Mode 2, Step 2: Platform + ID + Verification Code provided --- 
        (None, Some(code)) => {
            log::info!("尝试通过平台 '{}' 的 ID '{}' 和验证码 '{}' 确认解绑", platform, platform_id, code);
            
            // 1. 获取绑定的内部ID (在验证前先获取，以便解绑后返回)
            let binding = user_service.get_binding_by_platform_id(&platform, &platform_id).await?;
            let internal_id = binding.internal_id.clone();
            let stored_token = binding.session_token.clone();
            
            // 2. 验证并消费验证码
            user_service.validate_and_consume_verification_code(&platform, &platform_id, code).await?;
            
            // 3. 用存储的token获取存档进行简介核对
            log::debug!("使用存储的 Token 获取平台 '{}' 的 ID '{}' 的存档进行简介核对", platform, platform_id);
            match phigros_service.get_save(&stored_token).await {
                Ok(save) => {
                    // 4. 验证简介内容是否与验证码匹配
                    let user_intro: Option<String> = save.user
                        .as_ref()
                        .and_then(|user_map| user_map.get("selfIntro"))
                        .and_then(|value| value.as_str())
                        .map(|s| s.to_string());

                    if let Some(intro) = user_intro {
                         if intro.trim() == code.trim() {
                            log::info!("简介内容与验证码匹配成功 for 平台 '{}' 的 ID '{}'", platform, platform_id);
                            
                            // 5. 解绑
                            user_service.delete_platform_binding(&platform, &platform_id).await?;
                            
                             Ok(HttpResponse::Ok().json(ApiResponse {
                                 code: 200,
                                 status: "success".to_string(),
                                 message: Some("解绑成功 (简介验证)".to_string()),
                                data: Some(json!({
                                    "internal_id": internal_id
                                })),
                             }))
                         } else {
                            log::warn!("简介验证失败 for 平台 '{}' 的 ID '{}'. Expected code '{}', got intro '{}'", 
                                platform, platform_id, code.trim(), intro.trim());
                             Err(AppError::ProfileVerificationFailed("简介内容与提供的验证码不匹配".to_string()))
                         }
                    } else {
                        log::warn!("简介验证失败 for 平台 '{}' 的 ID '{}': 简介为空或类型不正确", platform, platform_id);
                         Err(AppError::ProfileVerificationFailed("简介为空或无法读取，无法验证".to_string()))
                    }
                },
                Err(AppError::InvalidSessionToken) => {
                    log::warn!("存储的 Token 无效，无法获取存档核对简介 平台 '{}' 的 ID '{}'", platform, platform_id);
                    Err(AppError::TokenVerificationFailed("无法获取存档核对简介 (Token已失效)，请稍后再试或使用平台ID+有效Token解绑".to_string()))
                },
                 Err(AppError::ReqwestError(e)) => {
                    log::error!("获取存档时网络错误 for 平台 '{}' 的 ID '{}': {}", platform, platform_id, e);
                     Err(AppError::Other(format!("获取存档时网络错误: {}", e)))
                },
                Err(e) => {
                    log::error!("获取存档时发生意外错误 for 平台 '{}' 的 ID '{}': {}", platform, platform_id, e);
                     Err(e)
                }
            }
        },
        
        // --- Invalid Combinations --- 
        _ => {
            log::warn!("无效的解绑请求参数组合: platform={:?}, platform_id={:?}, token={:?}, code={:?}", 
                platform, platform_id, req.token, req.verification_code);
            Err(AppError::BadRequest("无效的请求参数组合。请提供: (平台+平台ID+Token) 或 (平台+平台ID发起验证) 或 (平台+平台ID+验证码确认)".to_string()))
        }
    }
} 