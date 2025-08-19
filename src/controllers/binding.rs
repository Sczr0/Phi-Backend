use actix_web::{post, web, HttpResponse};
use chrono::Utc;
use serde_json::json;
use utoipa;

use crate::models::user::{
    ApiResponse, BindRequest, IdentifierRequest, PlatformBinding, TokenListResponse,
    UnbindInitiateResponse,
};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::{AppError, AppResult};
use crate::utils::save_parser::check_session_token;

/// 绑定平台账号
///
/// 将一个 Phigros 平台账号（由 platform 和 platform_id 标识）与一个 Session Token 绑定。
/// 如果 Token 已被其他账号绑定，则将此平台账号加入该 Token 的绑定列表。
/// 如果平台账号已被其他 Token 绑定，则会进行更新。
#[utoipa::path(
    post,
    path = "/bind",
    request_body = BindRequest,
    responses(
        (status = 200, description = "绑定成功或已更新", body = ApiResponse<serde_json::Value>)
    )
)]
#[post("/bind")]
pub async fn bind_user(
    bind_req: web::Json<BindRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    check_session_token(&bind_req.token)?;

    let platform = bind_req.platform.to_lowercase();
    let platform_id = bind_req.platform_id.clone();

    let internal_id: String;

    if user_service
        .is_platform_id_bound(&platform, &platform_id)
        .await?
    {
        let existing_binding = user_service
            .get_binding_by_platform_id(&platform, &platform_id)
            .await?;
        internal_id = existing_binding.internal_id.clone();

        if existing_binding.session_token != bind_req.token {
            user_service
                .update_platform_binding_token(&platform, &platform_id, &bind_req.token)
                .await?;

            return Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!("已更新平台 {platform} 的 ID {platform_id} 的Token")),
                data: Some(json!({ "internal_id": internal_id })),
            }));
        } else {
            return Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!(
                    "平台 {platform} 的 ID {platform_id} 已绑定到同一Token"
                )),
                data: Some(json!({ "internal_id": internal_id })),
            }));
        }
    }

    match user_service.get_binding_by_token(&bind_req.token).await {
        Ok(existing_binding) => {
            internal_id = existing_binding.internal_id.clone();
            let binding = PlatformBinding::new(
                internal_id.clone(),
                platform.clone(),
                platform_id.clone(),
                bind_req.token.clone(),
            );
            user_service.save_platform_binding(&binding).await?;

            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!(
                    "平台 {platform} 的 ID {platform_id} 已绑定到现有内部用户"
                )),
                data: Some(json!({ "internal_id": internal_id })),
            }))
        }
        Err(AppError::UserBindingNotFound(_)) => {
            internal_id = user_service
                .get_or_create_internal_id_by_token(&bind_req.token, &platform, &platform_id)
                .await?;

            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some(format!("平台 {platform} 的 ID {platform_id} 已成功绑定")),
                data: Some(json!({ "internal_id": internal_id })),
            }))
        }
        Err(e) => Err(e),
    }
}

/// 列出所有绑定的Token
///
/// 根据提供的任一标识（Token 或 平台+平台ID），找出其所属的内部用户，并列出该内部用户绑定的所有平台账号信息。
#[utoipa::path(
    post,
    path = "/token/list",
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功获取Token列表", body = ApiResponse<TokenListResponse>)
    )
)]
#[post("/token/list")]
pub async fn list_tokens(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    let platform = req.platform.as_ref().map(|p| p.to_lowercase());

    let internal_id = match (&req.token, &platform, &req.platform_id) {
        (Some(token), _, _) => {
            let binding = user_service.get_binding_by_token(token).await?;
            binding.internal_id
        }
        (_, Some(platform), Some(platform_id)) => {
            let binding = user_service
                .get_binding_by_platform_id(platform, platform_id)
                .await?;
            binding.internal_id
        }
        _ => {
            return Err(AppError::BadRequest("请提供token或平台信息".to_string()));
        }
    };

    let token_list = user_service.get_token_list(&internal_id).await?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "success".to_string(),
        message: Some("获取Token列表成功".to_string()),
        data: Some(token_list),
    }))
}

/// 解绑平台账号
///
/// 提供两种解绑模式:
/// 1.  **Token验证**: 提供 `platform`, `platform_id` 和 `token`。如果三者匹配，则直接解绑。
/// 2.  **简介验证**:
///     - **步骤1**: 只提供 `platform` 和 `platform_id`，会返回一个验证码。
///     - **步骤2**: 将游戏内简介修改为该验证码，然后再次调用此接口，并附带 `verification_code` 参数进行确认解绑。
#[utoipa::path(
    post,
    path = "/unbind",
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "操作成功（解绑或发起验证）", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "请求参数错误"),
        (status = 401, description = "验证失败")
    )
)]
#[post("/unbind")]
pub async fn unbind_user(
    req: web::Json<IdentifierRequest>,
    user_service: web::Data<UserService>,
    phigros_service: web::Data<PhigrosService>,
) -> AppResult<HttpResponse> {
    let platform = req.platform.as_ref().map(|p| p.to_lowercase());
    let (platform, platform_id) = match (&platform, &req.platform_id) {
        (Some(p), Some(id)) => (p.clone(), id.clone()),
        _ => return Err(AppError::BadRequest("必须提供平台和平台ID".to_string())),
    };

    match (&req.token, &req.verification_code) {
        (Some(token), None) => {
            log::info!("尝试通过平台 '{platform}' 的 ID '{platform_id}' 和 Token 解绑");
            check_session_token(token)?;

            let binding = user_service
                .get_binding_by_platform_id(&platform, &platform_id)
                .await?;
            if binding.session_token != *token {
                return Err(AppError::BadRequest(
                    "平台ID与SessionToken不匹配".to_string(),
                ));
            }

            let internal_id = user_service
                .delete_platform_binding(&platform, &platform_id)
                .await?;

            Ok(HttpResponse::Ok().json(ApiResponse {
                code: 200,
                status: "success".to_string(),
                message: Some("解绑成功 (平台ID+Token验证)".to_string()),
                data: Some(json!({ "internal_id": internal_id })),
            }))
        }

        (None, None) => {
            log::info!("尝试通过平台 '{platform}' 的 ID '{platform_id}' 发起简介验证解绑");

            let binding = user_service
                .get_binding_by_platform_id(&platform, &platform_id)
                .await?;
            let internal_id = binding.internal_id.clone();

            let code_details = user_service
                .generate_and_store_verification_code(&platform, &platform_id)
                .await?;
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
        }

        (None, Some(code)) => {
            log::info!(
                "尝试通过平台 '{platform}' 的 ID '{platform_id}' 和验证码 '{code}' 确认解绑"
            );

            let binding = user_service
                .get_binding_by_platform_id(&platform, &platform_id)
                .await?;
            let internal_id = binding.internal_id.clone();
            let stored_token = binding.session_token.clone();

            user_service
                .validate_and_consume_verification_code(&platform, &platform_id, code)
                .await?;

            log::debug!(
                "使用存储的 Token 获取平台 '{platform}' 的 ID '{platform_id}' 的存档进行简介核对"
            );
            match phigros_service.get_save(&stored_token).await {
                Ok(save) => {
                    let user_intro: Option<String> = save
                        .user
                        .as_ref()
                        .and_then(|user_map| user_map.get("selfIntro"))
                        .and_then(|value| value.as_str())
                        .map(|s| s.to_string());

                    if let Some(intro) = user_intro {
                        if intro.trim() == code.trim() {
                            log::info!("简介内容与验证码匹配成功 for 平台 '{platform}' 的 ID '{platform_id}'");

                            user_service
                                .delete_platform_binding(&platform, &platform_id)
                                .await?;

                            Ok(HttpResponse::Ok().json(ApiResponse {
                                code: 200,
                                status: "success".to_string(),
                                message: Some("解绑成功 (简介验证)".to_string()),
                                data: Some(json!({ "internal_id": internal_id })),
                            }))
                        } else {
                            log::warn!("简介验证失败 for 平台 '{}' 的 ID '{}'. Expected code '{}', got intro '{}'",
                                platform, platform_id, code.trim(), intro.trim());
                            Err(AppError::ProfileVerificationFailed(
                                "简介内容与提供的验证码不匹配".to_string(),
                            ))
                        }
                    } else {
                        log::warn!("简介验证失败 for 平台 '{platform}' 的 ID '{platform_id}': 简介为空或类型不正确");
                        Err(AppError::ProfileVerificationFailed(
                            "简介为空或无法读取，无法验证".to_string(),
                        ))
                    }
                }
                Err(AppError::InvalidSessionToken) => {
                    log::warn!("存储的 Token 无效，无法获取存档核对简介 平台 '{platform}' 的 ID '{platform_id}'");
                    Err(AppError::TokenVerificationFailed(
                        "无法获取存档核对简介 (Token已失效)，请稍后再试或使用平台ID+有效Token解绑"
                            .to_string(),
                    ))
                }
                Err(AppError::ReqwestError(e)) => {
                    log::error!(
                        "获取存档时网络错误 for 平台 '{platform}' 的 ID '{platform_id}': {e}"
                    );
                    Err(AppError::Other(format!("获取存档时网络错误: {e}")))
                }
                Err(e) => {
                    log::error!(
                        "获取存档时发生意外错误 for 平台 '{platform}' 的 ID '{platform_id}': {e}"
                    );
                    Err(e)
                }
            }
        }

        _ => {
            log::warn!(
                "无效的解绑请求参数组合: platform={:?}, platform_id={:?}, token={:?}, code={:?}",
                platform,
                req.platform_id,
                req.token,
                req.verification_code
            );
            Err(AppError::BadRequest("无效的请求参数组合。请提供: (平台+平台ID+Token) 或 (平台+平台ID发起验证) 或 (平台+平台ID+验证码确认)".to_string()))
        }
    }
}
