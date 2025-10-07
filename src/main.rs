use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use env_logger::Env;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteJournalMode};
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

// systemd 通知与看门狗集成
#[cfg(target_os = "linux")]
fn setup_systemd_notify(health_url: String) {
    // 发送 READY=1，告知 systemd 服务已就绪
    if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]) {
        log::debug!("发送 systemd READY 通知失败: {e}");
    } else {
        log::info!("已向 systemd 发送 READY=1");
    }

    // 如果启用了 Watchdog，则周期性喂狗（间隔的一半）
    let mut usec: u64 = 0;
    if sd_notify::watchdog_enabled(false, &mut usec) {
        let interval = std::time::Duration::from_micros(usec);
        let period = interval / 2;
        log::info!("侦测到 systemd Watchdog 已启用，周期: {:?}", interval);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(period);
            loop {
                ticker.tick().await;
                if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]) {
                    log::warn!("发送 systemd WATCHDOG 喂狗失败: {e}");
                }
            }
        });
    } else {
        log::info!("systemd Watchdog 未启用（未设置 WATCHDOG_USEC 或 PID 不匹配）");
    }
}

#[cfg(not(target_os = "linux"))]
fn setup_systemd_notify(_health_url: String) {}

// 新增：基于 /health 的 systemd Watchdog 喂狗
#[cfg(target_os = "linux")]
fn setup_systemd_watchdog_health(health_url: String) {
    if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]) {
        log::debug!("发送 systemd READY 通知失败: {e}");
    } else {
        log::info!("已向 systemd 发送 READY=1");
    }

    let mut usec: u64 = 0;
    if sd_notify::watchdog_enabled(false, &mut usec) {
        let interval = std::time::Duration::from_micros(usec);
        let period = interval / 2;
        log::info!("侦测到 systemd Watchdog 已启用，周期: {:?}", interval);
        tokio::spawn(async move {
            let health_check_timeout = std::time::Duration::from_millis((period.as_millis() as u64 * 7 / 10).max(2000));
            let client = match reqwest::Client::builder()
                .timeout(health_check_timeout)
                .pool_max_idle_per_host(2)
                .build() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("创建健康检查 HTTP 客户端失败: {e}");
                    return;
                }
            };
            let mut ticker = tokio::time::interval(period);
            loop {
                ticker.tick().await;
                let check_timeout = period * 7 / 10;
                let ok = match tokio::time::timeout(check_timeout, client.get(&health_url).send()).await {
                    Ok(Ok(resp)) => resp.status().is_success(),
                    Ok(Err(e)) => {
                        log::warn!("Watchdog 健康检查请求失败: {e}");
                        false
                    }
                    Err(_) => {
                        log::warn!("Watchdog 健康检查超时: {}", &health_url);
                        false
                    }
                };
                if ok {
                    if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]) {
                        log::warn!("发送 systemd WATCHDOG 喂狗失败: {e}");
                    }
                } else {
                    log::error!("健康检查失败，跳过本次 Watchdog 心跳");
                }
            }
        });
    } else {
        log::info!("systemd Watchdog 未启用（未设置 WATCHDOG_USEC 或 PID 不匹配）");
    }
}

#[cfg(not(target_os = "linux"))]
fn setup_systemd_watchdog_health(_health_url: String) {}

#[cfg(target_os = "linux")]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");

    tokio::select! {
        _ = sigterm.recv() => {
            log::info!("收到 SIGTERM, 准备优雅关闭...");
        }
        _ = sigint.recv() => {
            log::info!("收到 SIGINT, 准备优雅关闭...");
        }
    }
}

#[cfg(not(target_os = "linux"))]
async fn wait_for_shutdown_signal() {
    // 非 Linux 平台：仅监听 Ctrl+C
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c signal");
    log::info!("收到关闭信号, 正在关闭...");
}

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
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .idle_timeout(Some(std::time::Duration::from_secs(600)))
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

    // 1. 构建服务器实例，但不立即 .await 它
    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        let phigros_service = web::Data::new(PhigrosService::new());
        let song_service = web::Data::new(SongService::new());
        let user_service = web::Data::new(UserService::new(pool.clone()));
        let player_archive_service = web::Data::new(player_archive_service.clone());
        // 从环境变量读取并发限制，如果未设置则使用CPU核心数的一半作为默认值
        let max_renders = env::var("MAX_CONCURRENT_RENDERS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or_else(|| (num_cpus::get() / 2).max(1)); // 至少为1
        log::info!("图片渲染并发限制设置为: {max_renders}");

        let image_service =
            web::Data::new(ImageService::new(max_renders).with_db_pool(pool.clone()));

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
    .run();

    // 2. 获取服务器的句柄，以便稍后我们可以向它发送停止命令
    let server_handle = server.handle();

    // 3. 将服务器的 future 放到一个单独的 Tokio 任务中运行
    //    这样我们的 main 函数就不会被阻塞，可以继续执行下面的代码
    let mut server_task = tokio::spawn(server);

    // 启动后向 systemd 报告 READY，并基于 /health 定期喂狗（若启用）
    let health_url = format!("http://127.0.0.1:{}/health", port);
    setup_systemd_watchdog_health(health_url);

    // 4. 等待关闭信号 (Ctrl+C 或 SIGTERM on Linux)，或服务器异常退出
    tokio::select! {
        _ = wait_for_shutdown_signal() => {
            log::info!("开始优雅停止 Actix 服务器...");
        }
        res = &mut server_task => {
            match res {
                Ok(Ok(())) => {
                    log::warn!("Actix 服务器已主动退出");
                }
                Ok(Err(e)) => {
                    log::error!("Actix 服务器运行错误: {e}");
                }
                Err(e) => {
                    log::error!("Actix 服务器 Join 失败: {e}");
                }
            }
        }
    }

    // 通知 systemd 正在停止（Type=notify 下可选）
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]) {
            log::debug!("发送 systemd STOPPING 通知失败: {e}");
        }
    }

    // 5. 使用句柄来优雅地停止服务器
    //    stop(true) 表示 graceful shutdown
    server_handle.stop(true).await;

    log::info!("服务器已停止。");

    // 等待服务器任务真正结束，避免悬挂
    if let Err(e) = server_task.await {
        log::error!("等待服务器任务结束失败: {e}");
    }

    // 可以在这里添加任何其他的清理代码
    // pool.close().await;
    // log::info!("数据库连接池已关闭。");

    log::info!("程序退出。");
    Ok(())
}
