use actix_web::{get, post, web, HttpResponse, Result};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::models::user::IdentifierRequest;
use crate::services::image_service::ImageService;
use crate::services::phigros::PhigrosService;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::services::song::SongService;
use crate::services::user::UserService;
use crate::utils::error::AppError;

#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derive(Default, ToSchema)]
pub enum Theme {
    #[default]
    Black,
    White,
}

#[derive(Deserialize, Debug, ToSchema, IntoParams)]
pub struct BnImageQuery {
    #[serde(default)]
    pub theme: Theme,
}

#[derive(Deserialize, Debug, ToSchema, IntoParams)]
pub struct SongImageQuery {
    /// 歌曲的名称、ID或别名
    q: String,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct LeaderboardQuery {
    /// 返回的排行榜条目数量，默认为10
    pub limit: Option<usize>,
}

/// 生成Best N成绩图片
///
/// 根据用户的RKS计算结果，生成一张包含其最好N项成绩的图片。
#[utoipa::path(
    post,
    path = "/bn/{n}",
    params(
        ("n" = u32, Path, description = "要生成的Best N图片"),
        BnImageQuery
    ),
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功生成图片", content_type = "image/png", body = Vec<u8>)
    )
)]
#[post("/bn/{n}")]
pub async fn generate_bn_image(
    path: web::Path<u32>,
    query: web::Query<BnImageQuery>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    player_archive_service: web::Data<PlayerArchiveService>,
    image_service: web::Data<ImageService>,
) -> Result<HttpResponse, AppError> {
    let n = path.into_inner();

    if n == 0 {
        return Err(AppError::BadRequest("N must be greater than 0".to_string()));
    }

    let image_bytes = image_service
        .generate_bn_image(
            n,
            req,
            &query.theme,
            phigros_service,
            user_service,
            player_archive_service,
        )
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .body(image_bytes))
}

/// 生成单曲成绩图片
///
/// 根据用户成绩和歌曲信息，生成一张包含单曲成绩详情的图片。
#[utoipa::path(
    post,
    path = "/song",
    params(SongImageQuery),
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功生成图片", content_type = "image/png", body = Vec<u8>)
    )
)]
#[post("/song")]
pub async fn generate_song_image(
    query: web::Query<SongImageQuery>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    song_service: web::Data<SongService>,
    player_archive_service: web::Data<PlayerArchiveService>,
    image_service: web::Data<ImageService>,
) -> Result<HttpResponse, AppError> {
    let song_query = query.into_inner().q;

    let image_bytes = image_service
        .generate_song_image(
            song_query,
            req,
            phigros_service,
            user_service,
            song_service,
            player_archive_service,
        )
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .body(image_bytes))
}

/// RKS排行榜图片
///
/// 生成一张包含全服玩家RKS排行榜的图片。
#[utoipa::path(
    get,
    path = "/leaderboard/rks",
    params(LeaderboardQuery),
    responses(
        (status = 200, description = "成功生成排行榜图片", content_type = "image/png", body = Vec<u8>)
    )
)]
#[get("/leaderboard/rks")]
pub async fn get_rks_leaderboard(
    query: web::Query<LeaderboardQuery>,
    player_archive_service: web::Data<PlayerArchiveService>,
    image_service: web::Data<ImageService>,
) -> Result<HttpResponse, AppError> {
    let result = image_service
        .generate_rks_leaderboard_image(query.limit, player_archive_service)
        .await?;

    Ok(HttpResponse::Ok().content_type("image/png").body(result))
}
