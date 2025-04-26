use actix_web::{get, post, web, HttpResponse};

use crate::models::{ApiResponse, SongQuery, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::song::SongService;
use crate::services::user::UserService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::check_session_token;
use crate::utils::token_helper::resolve_token;

// 统一搜索接口 - 获取歌曲信息
#[get("/song/search")]
pub async fn search_song(
    query: web::Query<SongUnifiedQuery>,
    song_service: web::Data<SongService>,
) -> AppResult<HttpResponse> {
    // 检查是否提供了有效的查询参数
    if query.q.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse {
            code: 400,
            status: "error".to_string(),
            message: Some("请提供查询参数 q".to_string()),
            data: None::<()>,
        }));
    }

    log::info!("收到统一歌曲查询: q='{}'", query.q);
    
    // 使用统一搜索找到歌曲
    let song_info = song_service.search_song(&query.q)?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(song_info),
    }))
}

// 统一搜索接口 - 获取歌曲成绩
#[post("/song/search/record")]
pub async fn search_song_record(
    req: web::Json<IdentifierRequest>,
    query: web::Query<SongUnifiedQuery>,
    phigros_service: web::Data<PhigrosService>,
    song_service: web::Data<SongService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    // 检查是否提供了有效的查询参数
    if query.q.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse {
            code: 400,
            status: "error".to_string(),
            message: Some("请提供查询参数 q".to_string()),
            data: None::<()>,
        }));
    }

    log::info!("收到统一歌曲成绩查询: q='{}', difficulty={:?}", query.q, query.difficulty);
    
    // 解析并获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;
    
    // 检查会话令牌
    check_session_token(&token)?;
    
    // 查找歌曲
    let song_info = song_service.search_song(&query.q)?;
    
    // 查询歌曲成绩
    let record = phigros_service.get_song_record(&token, &song_info.id, query.difficulty.as_deref()).await?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(record),
    }))
}

// 原有函数保留兼容性，实现简化

#[post("/song/record")]
pub async fn get_song_record(
    req: web::Json<IdentifierRequest>,
    song_query: web::Query<SongQuery>,
    phigros_service: web::Data<PhigrosService>,
    song_service: web::Data<SongService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    // 打印查询参数
    log::info!("收到歌曲记录查询: song_id={:?}, song_name={:?}, nickname={:?}, difficulty={:?}", 
               song_query.song_id, song_query.song_name, song_query.nickname, song_query.difficulty);

    // 确定查询参数
    let query = if let Some(id) = &song_query.song_id {
        id.clone()
    } else if let Some(name) = &song_query.song_name {
        name.clone()
    } else if let Some(nickname) = &song_query.nickname {
        nickname.clone()
    } else {
        return Ok(HttpResponse::BadRequest().json(ApiResponse {
            code: 400,
            status: "error".to_string(),
            message: Some("请提供歌曲ID、名称或别名".to_string()),
            data: None::<()>,
        }));
    };

    // 解析并获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;
    
    // 检查会话令牌
    check_session_token(&token)?;
    
    // 使用统一搜索找到歌曲
    let song_info = song_service.search_song(&query)?;
    
    // 查询歌曲成绩
    let record = phigros_service.get_song_record(&token, &song_info.id, song_query.difficulty.as_deref()).await?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(record),
    }))
}

#[get("/song/info")]
pub async fn get_song_info(
    song_query: web::Query<SongQuery>,
    song_service: web::Data<SongService>,
) -> AppResult<HttpResponse> {
    // 打印查询参数
    log::info!("收到歌曲信息查询: song_id={:?}, song_name={:?}, nickname={:?}, difficulty={:?}", 
               song_query.song_id, song_query.song_name, song_query.nickname, song_query.difficulty);

    // 确定查询参数
    let query = if let Some(id) = &song_query.song_id {
        id.clone()
    } else if let Some(name) = &song_query.song_name {
        name.clone()
    } else if let Some(nickname) = &song_query.nickname {
        nickname.clone()
    } else {
        return Ok(HttpResponse::BadRequest().json(ApiResponse {
            code: 400,
            status: "error".to_string(),
            message: Some("请提供歌曲ID、名称或别名".to_string()),
            data: None::<()>,
        }));
    };
    
    // 使用统一搜索找到歌曲
    let info = song_service.search_song(&query)?;
    
    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(info),
    }))
}

// 统一查询参数结构
#[derive(Debug, serde::Deserialize)]
pub struct SongUnifiedQuery {
    pub q: String,
    pub difficulty: Option<String>,
} 