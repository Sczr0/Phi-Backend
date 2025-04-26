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
    // 加载.env文件
    dotenv().ok();
    
    // 初始化日志
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    
    // --- 数据库初始化 ---
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:phigros_bindings.db".to_string());
    
    log::info!("Connecting to database: {}", database_url);
    
    // 使用 SqliteConnectOptions 配置连接，并设置 create_if_missing
    let connect_options = SqliteConnectOptions::from_str(&database_url)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?
        .create_if_missing(true); 

    // 创建数据库连接池
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .expect("Failed to create database connection pool");
    
    // 初始化数据库表
    init_db(&pool).await.expect("Failed to initialize database");
    log::info!("Database initialized successfully");
    // --- 数据库初始化结束 ---
    
    // 获取配置
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string()).parse::<u16>().unwrap();

    log::info!("Starting server at http://{}:{}", host, port);

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
