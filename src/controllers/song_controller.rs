use actix_web::{get, post, web, HttpResponse};
use log::debug;
use serde::Deserialize;
use std::collections::HashMap;

use crate::models::{ApiResponse, user::IdentifierRequest, SongRecord, SongInfo};
use crate::services::phigros::PhigrosService;
use crate::services::song::SongService;
use crate::services::user::UserService;
use crate::utils::error::{AppResult, AppError};
use crate::utils::token_helper::resolve_token;

/// 搜索歌曲信息 (推荐)
#[get("/song/search")]
pub async fn search_song(
    query: web::Query<HashMap<String, String>>,
    song_service: web::Data<SongService>,
) -> AppResult<HttpResponse> {
    // 获取查询参数
    let q = query.get("q").ok_or_else(|| {
        crate::utils::error::AppError::BadRequest("缺少查询参数q".to_string())
    })?;
    
    debug!("接收到歌曲搜索请求: q={}", q);
    
    // 搜索歌曲
    let song_info = song_service.search_song(q)?;
    
    // 返回成功响应
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_info),
    }))
}

/// 搜索歌曲成绩记录 (推荐)
#[post("/song/search/record")]
pub async fn search_song_record(
    query: web::Query<HashMap<String, String>>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    song_service: web::Data<SongService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    // 获取查询参数
    let q = query.get("q").ok_or_else(|| {
        crate::utils::error::AppError::BadRequest("缺少查询参数q".to_string())
    })?;
    
    let difficulty = query.get("difficulty").map(|s| s.as_str());
    
    debug!("接收到歌曲记录搜索请求: q={}, difficulty={:?}", q, difficulty);
    
    // 获取歌曲ID
    let song_id = song_service.get_song_id(q)?;
    
    // 解析token
    let token = resolve_token(&req, &user_service).await?;
    
    // 获取歌曲记录
    let song_records = phigros_service.get_song_record(&token, &song_id, difficulty).await?;
    
    // 返回成功响应
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_records),
    }))
}

// --- 旧版兼容接口 ---

#[derive(Deserialize, Debug)]
pub struct SongInfoQuery {
    song_id: Option<String>,
    song_name: Option<String>,
    nickname: Option<String>,
}

/// 获取歌曲信息 (旧版)
#[get("/song/info")]
pub async fn get_song_info(
    query: web::Query<SongInfoQuery>,
    song_service: web::Data<SongService>,
) -> AppResult<HttpResponse> {
    debug!("接收到旧版歌曲信息请求: {:?}", query);

    let song_info: SongInfo = if let Some(id) = &query.song_id {
        song_service.get_song_by_id(id)?
    } else if let Some(name) = &query.song_name {
        song_service.search_song_by_name(name)?
    } else if let Some(nick) = &query.nickname {
        song_service.search_song_by_nickname(nick)?
    } else {
        return Err(AppError::BadRequest("必须提供 song_id, song_name 或 nickname 中的至少一个参数".to_string()));
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_info),
    }))
}

#[derive(Deserialize, Debug)]
pub struct SongRecordQuery {
    song_id: Option<String>,
    song_name: Option<String>,
    nickname: Option<String>,
    difficulty: Option<String>,
}

/// 获取特定歌曲的成绩记录 (旧版)
#[post("/song/record")]
pub async fn get_song_record(
    query: web::Query<SongRecordQuery>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    song_service: web::Data<SongService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    debug!("接收到旧版歌曲记录请求: {:?}", query);

    let song_id: String = if let Some(id) = &query.song_id {
        // 直接使用 ID
        id.clone()
    } else if let Some(name) = &query.song_name {
        song_service.get_song_id_by_name(name)?
    } else if let Some(nick) = &query.nickname {
        song_service.get_song_id_by_nickname(nick)?
    } else {
        return Err(AppError::BadRequest("必须提供 song_id, song_name 或 nickname 中的至少一个参数".to_string()));
    };

    let difficulty = query.difficulty.as_deref();

    let token = resolve_token(&req, &user_service).await?;
    let song_records = phigros_service.get_song_record(&token, &song_id, difficulty).await?;

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(song_records),
    }))
} 