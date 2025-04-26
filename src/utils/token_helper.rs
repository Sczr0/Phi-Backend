use actix_web::web;
use crate::models::IdentifierRequest;
use crate::services::user::UserService;
use crate::utils::error::{AppError, AppResult};

/// 从请求中解析出SessionToken
/// 优先使用请求体中的token字段
/// 如果token字段不存在，尝试使用qq字段查询数据库获取绑定的token
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

    if let Some(qq) = &req.qq {
        if !qq.trim().is_empty() {
            log::debug!("尝试通过请求体 qq 字段 ('{}') 查询 Token", qq);
            match user_service.get_user_by_qq(qq).await {
                Ok(user) => {
                    log::debug!("通过 QQ 查询到用户，获取 Token: {}", user.session_token);
                    return Ok(user.session_token);
                }
                Err(AppError::UserBindingNotFound(_)) => {
                    log::warn!("QQ '{}' 未找到或未绑定 Token", qq);
                    return Err(AppError::UserBindingNotFound(format!("QQ {} 未找到或未绑定", qq)));
                }
                Err(e) => {
                    log::error!("通过 QQ 查询用户时出错: {}", e);
                    return Err(e);
                }
            }
        }
    }
    
    log::error!("请求体中未提供有效的 'token' 或 'qq' 字段");
    Err(AppError::Other("请在请求中提供 'token' 或 'qq' 字段".to_string()))
} 