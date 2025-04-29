use actix_web::{web, HttpResponse};
use log::debug;
use std::collections::HashMap;

use crate::models::{ApiResponse, user::IdentifierRequest, SongRecord};
use crate::services::{PhigrosService, SongService, UserService};
use crate::utils::error::AppResult;
use crate::utils::token_helper::resolve_token;

/// 搜索歌曲信息
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

/// 搜索歌曲成绩记录
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