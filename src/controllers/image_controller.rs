use actix_web::{post, web, HttpResponse, Result};
use crate::models::user::IdentifierRequest;
use crate::services::image_service;
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::AppError;

#[post("/bn/{n}")]
pub async fn generate_bn_image(
    path: web::Path<u32>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>
) -> Result<HttpResponse, AppError> {
    let n = path.into_inner();
    
    // 验证 n 的有效性
    if n == 0 {
        return Err(AppError::BadRequest("N must be greater than 0".to_string()));
    }

    let identifier = req.into_inner();
    
    // 调用服务时传递注入的服务实例
    let image_bytes = image_service::generate_bn_image(n, identifier, phigros_service, user_service).await?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .body(image_bytes))
} 