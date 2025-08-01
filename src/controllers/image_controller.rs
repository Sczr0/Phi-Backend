use actix_web::{post, get, web, HttpResponse, Result};
use serde::{Deserialize/*, Serialize*/};
use crate::models::user::IdentifierRequest;
use crate::services::image_service::{ImageService};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::services::song::SongService;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::utils::error::AppError;

#[derive(Deserialize, Debug)]
pub struct SongImageQuery {
    q: String,
}

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    pub limit: Option<usize>,
}

#[post("/bn/{n}")]
pub async fn generate_bn_image(
    path: web::Path<u32>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    player_archive_service: web::Data<PlayerArchiveService>,
    image_service: web::Data<ImageService>, // 新增
) -> Result<HttpResponse, AppError> {
    let n = path.into_inner();
    
    // 验证 n 的有效性
    if n == 0 {
        return Err(AppError::BadRequest("N must be greater than 0".to_string()));
    }
    
    // 调用服务时传递注入的服务实例，直接传递 req
    let image_bytes = image_service.generate_bn_image(
        n,
        req, // Pass req directly
        phigros_service,
        user_service,
        player_archive_service
    ).await?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .body(image_bytes))
}

#[post("/song")]
pub async fn generate_song_image(
    query: web::Query<SongImageQuery>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    song_service: web::Data<SongService>,
    player_archive_service: web::Data<PlayerArchiveService>,
    image_service: web::Data<ImageService>, // 新增
) -> Result<HttpResponse, AppError> {
    let song_query = query.into_inner().q;

    // 调用新的服务函数来生成图片，直接传递 req
    let image_bytes = image_service.generate_song_image(
        song_query,
        req, // Pass req directly
        phigros_service,
        user_service,
        song_service,
        player_archive_service
    )
    .await?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .body(image_bytes))
}

/// RKS排行榜图片
#[get("/leaderboard/rks")]
pub async fn get_rks_leaderboard(
    query: web::Query<LeaderboardQuery>,
    player_archive_service: web::Data<PlayerArchiveService>,
    image_service: web::Data<ImageService>, // 新增
) -> Result<HttpResponse, AppError> {
    let result = image_service.generate_rks_leaderboard_image(
        query.limit,
        player_archive_service,
    )
    .await?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .body(result))
} 