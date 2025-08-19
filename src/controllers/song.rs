use actix_web::{get, post, web, HttpResponse};
use log::debug;
use serde::Deserialize;
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

use crate::models::{
    predictions::PredictionResponse,
    save::SongRecord,
    song::SongInfo,
    user::{ApiResponse, IdentifierRequest},
};
use crate::services::phigros::PhigrosService;
use crate::services::song::SongService;
use crate::services::user::UserService;
use crate::utils::data_loader::get_predicted_constant;
use crate::utils::error::{AppError, AppResult};
use crate::utils::token_helper::resolve_token;

#[derive(Deserialize, Debug, IntoParams)]
#[allow(dead_code)]
struct SongSearchQuery {
    /// 歌曲的名称、ID或别名
    q: String,
    /// 可选的难度过滤器 (EZ, HD, IN, AT)
    difficulty: Option<String>,
}

/// 搜索歌曲信息 (推荐)
///
/// 根据提供的查询字符串（可以是歌曲名称、ID或别名）来搜索歌曲的详细信息。
#[utoipa::path(
    get,
    path = "/song/search",
    params(SongSearchQuery),
    responses(
        (status = 200, description = "成功找到歌曲信息", body = ApiResponse<SongInfo>)
    )
)]
#[get("/song/search")]
pub async fn search_song(
    query: web::Query<HashMap<String, String>>,
    song_service: web::Data<SongService>,
) -> AppResult<HttpResponse> {
    let q = query
        .get("q")
        .ok_or_else(|| crate::utils::error::AppError::BadRequest("缺少查询参数q".to_string()))?;

    debug!("接收到歌曲搜索请求: q={q}");

    let song_info = song_service.search_song(q)?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_info),
    }))
}

/// 搜索歌曲成绩记录 (推荐)
///
/// 根据提供的查询字符串和用户身份，搜索特定歌曲的成绩记录。
#[utoipa::path(
    post,
    path = "/song/search/record",
    params(SongSearchQuery),
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功找到歌曲成绩记录", body = ApiResponse<SongRecord>)
    )
)]
#[post("/song/search/record")]
pub async fn search_song_record(
    query: web::Query<HashMap<String, String>>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    song_service: web::Data<SongService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    let q = query
        .get("q")
        .ok_or_else(|| crate::utils::error::AppError::BadRequest("缺少查询参数q".to_string()))?;
    let difficulty = query.get("difficulty").map(|s| s.as_str());
    debug!("接收到歌曲记录搜索请求: q={q}, difficulty={difficulty:?}");

    let song_id = song_service.get_song_id(q)?;
    let token = resolve_token(&req, &user_service).await?;
    let song_records = phigros_service
        .get_song_record(&token, &song_id, difficulty)
        .await?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_records),
    }))
}

// --- 旧版兼容接口 ---

#[derive(Deserialize, Debug, ToSchema, IntoParams)]
pub struct SongInfoQuery {
    song_id: Option<String>,
    song_name: Option<String>,
    nickname: Option<String>,
}

/// 获取歌曲信息 (旧版)
#[utoipa::path(
    get,
    path = "/song/info",
    params(SongInfoQuery),
    responses(
        (status = 200, description = "成功找到歌曲信息", body = ApiResponse<SongInfo>)
    )
)]
#[get("/song/info")]
pub async fn get_song_info(
    query: web::Query<SongInfoQuery>,
    song_service: web::Data<SongService>,
) -> AppResult<HttpResponse> {
    debug!("接收到旧版歌曲信息请求: {query:?}");

    let song_info: SongInfo = if let Some(id) = &query.song_id {
        song_service.get_song_by_id(id)?
    } else if let Some(name) = &query.song_name {
        song_service.search_song_by_name(name)?
    } else if let Some(nick) = &query.nickname {
        song_service.search_song_by_nickname(nick)?
    } else {
        return Err(AppError::BadRequest(
            "必须提供 song_id, song_name 或 nickname 中的至少一个参数".to_string(),
        ));
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_info),
    }))
}

#[derive(Deserialize, Debug, ToSchema, IntoParams)]
pub struct SongRecordQuery {
    song_id: Option<String>,
    song_name: Option<String>,
    nickname: Option<String>,
    difficulty: Option<String>,
}

/// 获取特定歌曲的成绩记录 (旧版)
#[utoipa::path(
    post,
    path = "/song/record",
    params(SongRecordQuery),
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功找到歌曲成绩", body = ApiResponse<SongRecord>)
    )
)]
#[post("/song/record")]
pub async fn get_song_record(
    query: web::Query<SongRecordQuery>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    song_service: web::Data<SongService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到旧版歌曲记录请求: {query:?}");

    let song_id: String = if let Some(id) = &query.song_id {
        id.clone()
    } else if let Some(name) = &query.song_name {
        song_service.get_song_id_by_name(name)?
    } else if let Some(nick) = &query.nickname {
        song_service.get_song_id_by_nickname(nick)?
    } else {
        return Err(AppError::BadRequest(
            "必须提供 song_id, song_name 或 nickname 中的至少一个参数".to_string(),
        ));
    };

    let difficulty = query.difficulty.as_deref();
    let token = resolve_token(&req, &user_service).await?;
    let song_records = phigros_service
        .get_song_record(&token, &song_id, difficulty)
        .await?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_records),
    }))
}

/// 搜索歌曲预测常数
#[utoipa::path(
    get,
    path = "/song/search/predictions",
    params(SongSearchQuery),
    responses(
        (status = 200, description = "成功获取预测定数", body = ApiResponse<Vec<PredictionResponse>>)
    )
)]
#[get("/song/search/predictions")]
pub async fn search_song_predictions(
    query: web::Query<HashMap<String, String>>,
    song_service: web::Data<SongService>,
) -> AppResult<HttpResponse> {
    let q = query
        .get("q")
        .ok_or_else(|| crate::utils::error::AppError::BadRequest("缺少查询参数q".to_string()))?;
    let difficulty = query.get("difficulty").map(|s| s.as_str());
    debug!("接收到歌曲预测常数搜索请求: q={q}, difficulty={difficulty:?}");

    let song_id = song_service.get_song_id(q)?;

    let result = match difficulty {
        Some(diff) => {
            let predicted_constant = get_predicted_constant(&song_id, diff);
            vec![PredictionResponse {
                song_id: song_id.clone(),
                difficulty: diff.to_string(),
                predicted_constant,
            }]
        }
        None => {
            let difficulties = vec!["EZ", "HD", "IN", "AT"];
            let mut results = Vec::new();
            for diff in difficulties {
                let predicted_constant = get_predicted_constant(&song_id, diff);
                results.push(PredictionResponse {
                    song_id: song_id.clone(),
                    difficulty: diff.to_string(),
                    predicted_constant,
                });
            }
            results
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(result),
    }))
}
