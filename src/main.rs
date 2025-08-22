use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use env_logger::Env;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
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

use crate::models::user::ApiResponse;
use services::image_service::ImageService;
use services::phigros::PhigrosService;
use services::player_archive_service::PlayerArchiveService;
use services::song::SongService;
use services::user::UserService;
use utils::cover_loader;

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
        controllers::image::get_rks_leaderboard,
        controllers::image::get_cache_stats,
        controllers::status::get_status
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
            ApiResponse<serde_json::Value>,
            controllers::status::StatusResponse,
            controllers::status::MaintenanceResponse
        )
    ),
    tags(
        (name = "phi-backend-rust", description = "Phigros Backend Rust API Endpoints")
    )
)]
struct ApiDoc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 初始化配置
    if let Err(e) = crate::utils::config::init_config() {
        eprintln!("启动失败：无法加载配置: {e}");
        return Err(std::io::Error::other(e.to_string()));
    }
    let app_config = crate::utils::config::get_config().unwrap(); // 在此之后可以安全地unwrap

    // 初始化日志
    env_logger::init_from_env(Env::default().default_filter_or(&app_config.log_level));

    // --- 获取配置 ---
    let database_url = app_config.database_url.clone();
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = app_config.server_port;

    log::info!("应用配置:");
    log::info!("- 数据库URL: {database_url}");
    log::info!("- 服务器地址: {host}:{port}");
    log::info!("- 日志级别: {}", app_config.log_level);
    log::info!("- 页脚文本: {}", app_config.custom_footer_text);

    if let Err(e) = cover_loader::ensure_covers_available() {
        log::error!("初始化曲绘资源失败: {e:?}");
    } else {
        log::info!("曲绘资源检查/准备完成.");
    }

    log::info!("正在连接数据库: {database_url}");

    let connect_options = SqliteConnectOptions::from_str(&database_url)
        .map_err(|e| {
            log::error!("数据库URL格式无效: {e}");
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .map_err(|e| {
            log::error!("无法创建数据库连接池: {e}");
            std::io::Error::other(format!("Failed to create database connection pool: {e}"))
        })?;

    log::info!("正在运行数据库迁移...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| {
            log::error!("数据库迁移失败: {e}");
            std::io::Error::other(format!("Failed to run database migrations: {e}"))
        })?;
    log::info!("数据库迁移完成");

    let archive_config = crate::models::player_archive::ArchiveConfig {
        store_push_acc: true,
        best_n_count: 27,
        history_max_records: 10,
    };
    let player_archive_service = PlayerArchiveService::new(pool.clone(), Some(archive_config));

    log::info!("正在启动服务器 http://{host}:{port}");
    log::info!("API 文档位于 http://{host}:{port}/swagger-ui/");

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
        let image_service = web::Data::new(ImageService::new().with_db_pool(pool.clone()));

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
                SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", openapi.clone()),
            )
            .configure(routes::configure)
    })
    .shutdown_timeout(5)
    .bind((host, port))?
    .run()
    .await
}
