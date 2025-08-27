use actix_web::{get, HttpResponse, Responder};

/// 健康检查端点
///
/// 用于检查服务是否正在运行并能够响应请求。
/// 主要供外部监控系统（如 systemd, Kubernetes）使用。
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "服务健康", body = String, example = json!("OK"))
    )
)]
#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}