use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use dotenv::dotenv;
use env_logger::Env;
use sqlx::sqlite::{SqlitePoolOptions, SqliteConnectOptions};
use std::env;
use std::str::FromStr;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod config;
mod controllers;
mod models;
mod routes;
mod services;
mod utils;

use services::phigros::PhigrosService;
use services::song::SongService;
use services::user::UserService;
use services::player_archive_service::PlayerArchiveService;
use services::image_service::ImageService;
use utils::cover_loader;
use crate::models::user::ApiResponse;

#[derive(OpenApi)]
#[openapi(
    paths(
        controllers::auth::generate_qr_code,
        controllers::auth::check_qr_status,
        controllers::binding::bind_user,
        controllers::binding::unbind_user,
        controllers::binding::list_tokens,
        controllers::b30::get_b30,
        controllers::rks::get_rks,
        controllers::rks::get_bn,
        controllers::save::get_cloud_saves,
        controllers::save::get_cloud_saves_with_difficulty,
        controllers::song::search_song,
        controllers::song::search_song_record,
        controllers::song::search_song_predictions,
        controllers::song::get_song_info,
        controllers::song::get_song_record,
        controllers::image::generate_bn_image,
        controllers::image::generate_song_image,
        controllers::image::get_rks_leaderboard
    ),
    components(
        schemas(
            models::user::IdentifierRequest,
            models::user::TokenListResponse,
            models::user::PlatformBindingInfo,
            models::rks::RksResult,
            models::b30::B30Result,
            models::save::GameSave,
            models::song::SongInfo,
            models::predictions::PredictionResponse,
            ApiResponse<serde_json::Value>
        )
    ),
    tags(
        (name = "phi-backend-rust", description = "Phigros Backend Rust API Endpoints")
    )
)]
struct ApiDoc;


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 加载.env文件 (如果存在)
    dotenv().ok();
    
    // 初始化日志
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    
    // --- 获取配置 ---
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:phigros_bindings.db".to_string());
    
    let info_data_path = env::var("INFO_DATA_PATH")
        .unwrap_or_else(|_| "info".to_string());
    
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    
    log::info!("应用配置:");
    log::info!("- 数据库URL: {}", database_url);
    log::info!("- 数据文件路径: {}", info_data_path);
    log::info!("- 服务器地址: {}:{}", host, port);
    
    if let Err(e) = cover_loader::ensure_covers_available() {
        log::error!("初始化曲绘资源失败: {:?}", e);
    } else {
        log::info!("曲绘资源检查/准备完成.");
    }
    
    log::info!("正在连接数据库: {}", database_url);
    
    let connect_options = SqliteConnectOptions::from_str(&database_url)
        .map_err(|e| {
            log::error!("数据库URL格式无效: {}", e);
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .map_err(|e| {
            log::error!("无法创建数据库连接池: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to create database connection pool: {}", e))
        })?;

    log::info!("正在运行数据库迁移...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| {
            log::error!("数据库迁移失败: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to run database migrations: {}", e))
        })?;
    log::info!("数据库迁移完成");

    let archive_config = crate::models::player_archive::ArchiveConfig {
        store_push_acc: true,
        best_n_count: 27,
        history_max_records: 10,
    };
    let player_archive_service = PlayerArchiveService::new(pool.clone(), Some(archive_config));

    log::info!("正在启动服务器 http://{}:{}", host, port);
    log::info!("API 文档位于 http://{}:{}/swagger-ui/", host, port);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        let phigros_service = web::Data::new(PhigrosService::new());
        let song_service = web::Data::new(SongService::new());
        let user_service = web::Data::new(UserService::new(pool.clone()));
        let player_archive_service = web::Data::new(player_archive_service.clone());
        let image_service = web::Data::new(ImageService::new());

        let openapi = ApiDoc::openapi();

        App::new()
            .app_data(phigros_service.clone())
            .app_data(song_service.clone())
            .app_data(user_service.clone())
            .app_data(player_archive_service.clone())
            .app_data(image_service.clone())
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-docs/openapi.json", openapi.clone()),
            )
            .configure(routes::configure)
    })
    .bind((host, port))?
    .run()
    .await
}
