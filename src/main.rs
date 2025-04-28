use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use dotenv::dotenv;
use env_logger::Env;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteConnectOptions};
use std::env;
use std::str::FromStr;

mod config;
mod controllers;
mod models;
mod routes;
mod services;
mod utils;

use services::phigros::PhigrosService;
use services::song::SongService;
use services::user::UserService;
use utils::cover_loader;

// 初始化数据库表
async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // 创建用户绑定表 (已存在)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_bindings (
            qq TEXT PRIMARY KEY NOT NULL,
            session_token TEXT NOT NULL, -- 移除 UNIQUE 约束，如果你确认要一个 Token 绑定多个 QQ
            nickname TEXT,
            last_update TEXT
        )
        "#,
    )
    .execute(pool)
    .await?; // 使用 ? 简化错误处理

    // --- 新增代码开始 ---
    // 创建解绑验证码表
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS unbind_verification_codes (
            qq TEXT PRIMARY KEY NOT NULL,
            code TEXT NOT NULL,
            expires_at DATETIME NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?; // 使用 ? 简化错误处理
    // --- 新增代码结束 ---

    log::info!("数据库表初始化检查完成"); // 修改日志信息
    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 加载.env文件 (如果存在)
    // 该方法不会覆盖已经存在的环境变量，
    // 因此Docker环境中设置的环境变量将优先生效
    dotenv().ok();
    
    // 初始化日志
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    
    // --- 获取配置 ---
    // 数据库配置，默认为本地SQLite文件
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:phigros_bindings.db".to_string());
    
    // 数据文件路径，默认为"info"目录
    let info_data_path = env::var("INFO_DATA_PATH")
        .unwrap_or_else(|_| "info".to_string());
    
    // 服务器配置
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    
    // 输出配置信息
    log::info!("应用配置:");
    log::info!("- 数据库URL: {}", database_url);
    log::info!("- 数据文件路径: {}", info_data_path);
    log::info!("- 服务器地址: {}:{}", host, port);
    
    // --- 检查并准备曲绘资源 ---
    if let Err(e) = cover_loader::ensure_covers_available() {
        log::error!("初始化曲绘资源失败: {:?}", e);
        // 根据需要决定是否退出程序
        // return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to initialize cover resources"));
    } else {
        log::info!("曲绘资源检查/准备完成.");
    }
    // --- 曲绘资源检查结束 ---
    
    // --- 数据库初始化 ---
    log::info!("正在连接数据库: {}", database_url);
    
    // 使用 SqliteConnectOptions 配置连接，并设置 create_if_missing
    let connect_options = SqliteConnectOptions::from_str(&database_url)
        .map_err(|e| {
            log::error!("数据库URL格式无效: {}", e);
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?
        .create_if_missing(true);

    // 创建数据库连接池
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .map_err(|e| {
            log::error!("无法创建数据库连接池: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to create database connection pool: {}", e))
        })?;
    
    // 初始化数据库表
    init_db(&pool).await.map_err(|e| {
        log::error!("数据库初始化失败: {}", e);
        std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to initialize database: {}", e))
    })?;
    log::info!("数据库初始化成功");
    // --- 数据库初始化结束 ---
    
    log::info!("正在启动服务器 http://{}:{}", host, port);

    // 创建并启动HTTP服务器
    HttpServer::new(move || {
        // 配置CORS
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        let phigros_service = web::Data::new(PhigrosService::new());
        let song_service = web::Data::new(SongService::new());
        let user_service = web::Data::new(UserService::new(pool.clone()));

        App::new()
            .app_data(phigros_service.clone())
            .app_data(song_service.clone())
            .app_data(user_service.clone())
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .configure(routes::configure)
    })
    .bind((host, port))?
    .run()
    .await
}
