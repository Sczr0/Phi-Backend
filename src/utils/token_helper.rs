use crate::models::user::IdentifierRequest;
use crate::services::user::UserService;
use crate::utils::error::{AppError, AppResult};
use actix_web::web;

/// 从请求中解析出SessionToken
/// 优先使用请求体中的token字段
/// 如果token字段不存在，尝试使用platform和platform_id字段查询数据库获取绑定的token
pub async fn resolve_token(
    req: &web::Json<IdentifierRequest>,
    user_service: &web::Data<UserService>,
) -> AppResult<String> {
    if let Some(token) = &req.token {
        if !token.trim().is_empty() {
            log::debug!("从请求体 token 字段解析到 Token");
            return Ok(token.clone());
        }
    }

    if let (Some(platform), Some(platform_id)) = (&req.platform, &req.platform_id) {
        if !platform.trim().is_empty() && !platform_id.trim().is_empty() {
            log::debug!("尝试通过请求体平台 '{platform}' 的 ID '{platform_id}' 查询 Token");
            match user_service
                .get_binding_by_platform_id(platform, platform_id)
                .await
            {
                Ok(binding) => {
                    log::debug!(
                        "通过平台ID查询到绑定，获取 Token: {}",
                        binding.session_token
                    );
                    return Ok(binding.session_token);
                }
                Err(AppError::UserBindingNotFound(_)) => {
                    log::warn!("平台 '{platform}' 的 ID '{platform_id}' 未找到或未绑定 Token");
                    return Err(AppError::UserBindingNotFound(format!(
                        "平台 {platform} 的 ID {platform_id} 未找到或未绑定"
                    )));
                }
                Err(e) => {
                    log::error!("通过平台ID查询绑定时出错: {e}");
                    return Err(e);
                }
            }
        }
    }

    log::error!("请求体中未提供有效的 'token' 或 'platform'+'platform_id' 字段");
    Err(AppError::Other(
        "请在请求中提供 'token' 或 'platform'+'platform_id' 字段".to_string(),
    ))
}

/// 从请求中获取内部用户ID
/// 首先尝试解析token获取平台绑定，然后返回关联的内部ID
#[allow(dead_code)]
pub async fn resolve_internal_id(
    req: &web::Json<IdentifierRequest>,
    user_service: &web::Data<UserService>,
) -> AppResult<String> {
    // 先尝试获取token
    let token = match &req.token {
        Some(t) if !t.trim().is_empty() => {
            log::debug!("从请求体 token 字段解析到 Token");
            t.clone()
        }
        _ => {
            if let (Some(platform), Some(platform_id)) = (&req.platform, &req.platform_id) {
                if !platform.trim().is_empty() && !platform_id.trim().is_empty() {
                    log::debug!("尝试通过请求体平台 '{platform}' 的 ID '{platform_id}' 查询 Token");
                    match user_service
                        .get_binding_by_platform_id(platform, platform_id)
                        .await
                    {
                        Ok(binding) => binding.session_token,
                        Err(e) => return Err(e),
                    }
                } else {
                    return Err(AppError::BadRequest("提供的平台或平台ID为空".to_string()));
                }
            } else {
                return Err(AppError::BadRequest(
                    "请在请求中提供 'token' 或 'platform'+'platform_id' 字段".to_string(),
                ));
            }
        }
    };

    // 使用token获取绑定信息
    match user_service.get_binding_by_token(&token).await {
        Ok(binding) => {
            log::debug!("通过Token获取到内部ID: {}", binding.internal_id);
            Ok(binding.internal_id)
        }
        Err(e) => Err(e),
    }
}
