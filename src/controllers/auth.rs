use crate::services::taptap::{TapTapQrCodeResponse, TapTapService};
use crate::utils::image_renderer;
use actix_web::{web, HttpResponse, Responder};
use base64::{engine::general_purpose, Engine as _};
use lazy_static::lazy_static;
use qrcode::{render::svg, QrCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use utoipa::ToSchema;
use uuid::Uuid;

lazy_static! {
    static ref QR_CODE_STORE: Mutex<HashMap<String, QrCodeState>> = Mutex::new(HashMap::new());
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QrCodeState {
    #[serde(rename = "deviceCode")]
    pub device_code: String,
    #[serde(rename = "deviceId")]
    pub device_id: String,
    pub status: String, // pending, scanned, success, expired
    #[serde(rename = "sessionToken")]
    pub session_token: Option<String>,
    #[serde(skip)]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GenerateQrCodeResponse {
    #[serde(rename = "qrId")]
    pub qr_id: String,
    #[serde(rename = "qrCodeImage")]
    pub qr_code_image: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CheckQrStatusResponse {
    pub status: String,
    #[serde(rename = "sessionToken", skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 生成用于扫码登录的二维码
///
/// 返回一个唯一的 qr_id 和一个 base64 编码的 PNG 图片。
#[utoipa::path(
    get,
    path = "/auth/qrcode",
    responses(
        (status = 200, description = "成功生成二维码", body = GenerateQrCodeResponse),
        (status = 500, description = "生成二维码失败")
    )
)]
pub async fn generate_qr_code() -> impl Responder {
    let taptap_service = TapTapService::new();
    let device_id = Uuid::new_v4().to_string().replace("-", "");
    match taptap_service.request_login_qr_code(&device_id).await {
        Ok(data) => {
            let qr_code_data: TapTapQrCodeResponse = serde_json::from_value(data).unwrap();
            let qr_id = Uuid::new_v4().to_string();
            let mut store = QR_CODE_STORE.lock().unwrap();
            store.insert(
                qr_id.clone(),
                QrCodeState {
                    device_code: qr_code_data.device_code.clone(),
                    device_id: device_id.clone(),
                    status: "pending".to_string(),
                    session_token: None,
                    created_at: chrono::Utc::now(),
                },
            );

            let qr_id_clone = qr_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                let mut store = QR_CODE_STORE.lock().unwrap();
                store.remove(&qr_id_clone);
                log::info!("QR code {qr_id_clone} expired and removed from store.");
            });

            // 1. 创建二维码数据
            let code = QrCode::new(&qr_code_data.qrcode_url).unwrap();

            // 2. 将二维码渲染成SVG字符串
            let svg_str = code
                .render()
                .min_dimensions(256, 256) // 设置最小尺寸为256x256
                .dark_color(svg::Color("#000000")) // 黑色模块
                .light_color(svg::Color("#FFFFFF")) // 白色背景
                .build();

            // 3. 使用 image_renderer 将SVG转换为PNG字节
            let png_bytes = match image_renderer::render_svg_to_png(svg_str) {
                Ok(bytes) => bytes,
                Err(e) => {
                    log::error!("Failed to render QR code SVG to PNG: {e:?}");
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to render QR code",
                        "details": e.to_string()
                    }));
                }
            };

            // 4. 将PNG字节流编码为Base64字符串
            let qr_code_image = format!(
                "data:image/png;base64,{}",
                general_purpose::STANDARD.encode(&png_bytes)
            );

            // 5. 返回响应
            HttpResponse::Ok().json(GenerateQrCodeResponse {
                qr_id,
                qr_code_image,
            })
        }
        Err(e) => {
            log::error!("Error generating QR code: {e:?}");
            HttpResponse::InternalServerError().json(serde_json::json!({ "error": "Failed to generate QR code", "details": e.to_string() }))
        }
    }
}

/// 检查二维码扫码状态
///
/// 客户端应轮询此接口以检查登录状态。
/// 状态可能为: pending, scanned, success, expired。
#[utoipa::path(
    get,
    path = "/auth/qrcode/{qrId}/status",
    params(
        ("qrId" = String, Path, description = "由 /auth/qrcode 返回的唯一ID")
    ),
    responses(
        (status = 200, description = "成功获取状态", body = CheckQrStatusResponse),
        (status = 404, description = "QR Code 不存在或已过期")
    )
)]
pub async fn check_qr_status(path: web::Path<String>) -> impl Responder {
    let qr_id = path.into_inner();

    // --- 第1步：缩小锁的作用域，只用于读取 ---
    // 我们只在这里读取一次，然后立即释放锁
    let stored_data = {
        // 使用花括号创建一个新的作用域
        let store = QR_CODE_STORE.lock().unwrap();
        store.get(&qr_id).cloned() // 克隆数据，这样我们就可以在锁外使用它
    }; // store 在这里被 drop，锁被释放

    // 如果二维码不存在，直接返回过期
    let mut stored_data = match stored_data {
        Some(data) => data,
        None => {
            return HttpResponse::NotFound().json(CheckQrStatusResponse {
                status: "expired".to_string(),
                session_token: None,
                message: Some("QR Code not found or has already been used.".to_string()),
            });
        }
    };

    // --- 第2步：处理已成功的状态 ---
    // 如果状态已经是 "success"，我们返回成功信息，并从存储中删除它
    if stored_data.status == "success" {
        // 再次获取锁以执行删除操作
        let mut store = QR_CODE_STORE.lock().unwrap();
        store.remove(&qr_id); // 清理已成功的条目

        return HttpResponse::Ok().json(CheckQrStatusResponse {
            status: "success".to_string(),
            session_token: stored_data.session_token,
            message: None,
        });
    }

    // --- 第3步：处理过期 ---
    // 检查时间是否已超过5分钟 (300秒)
    if (chrono::Utc::now() - stored_data.created_at).num_seconds() > 300 {
        // 获取锁以执行删除操作
        let mut store = QR_CODE_STORE.lock().unwrap();
        store.remove(&qr_id); // 清理过期的条目

        return HttpResponse::NotFound().json(CheckQrStatusResponse {
            status: "expired".to_string(),
            session_token: None,
            message: Some("QR Code expired.".to_string()),
        });
    }

    // --- 第4步：执行网络请求 (现在我们没有持有任何锁) ---
    let taptap_service = TapTapService::new();
    let check_result = taptap_service
        .check_qr_code_result(&stored_data.device_code, &stored_data.device_id)
        .await;

    // --- 第5步：根据网络请求结果，再次获取锁来更新状态 ---
    match check_result {
        Ok(result) => {
            // 再次获取锁来更新或删除 HashMap 中的数据
            let mut store = QR_CODE_STORE.lock().unwrap();

            if let Some(session_token) = result.get("sessionToken").and_then(|v| v.as_str()) {
                // 登录成功！返回token并立即从store中删除
                store.remove(&qr_id);
                HttpResponse::Ok().json(CheckQrStatusResponse {
                    status: "success".to_string(),
                    session_token: Some(session_token.to_string()),
                    message: None,
                })
            } else if result.get("error").and_then(|v| v.as_str()) == Some("authorization_waiting")
            {
                // 用户已扫码，更新状态
                stored_data.status = "scanned".to_string();
                store.insert(qr_id, stored_data);
                HttpResponse::Ok().json(CheckQrStatusResponse {
                    status: "scanned".to_string(),
                    session_token: None,
                    message: None,
                })
            } else if result.get("error").and_then(|v| v.as_str()) == Some("authorization_pending")
            {
                // 状态未变，什么都不做，只返回响应
                HttpResponse::Ok().json(CheckQrStatusResponse {
                    status: "pending".to_string(),
                    session_token: None,
                    message: None,
                })
            } else {
                // 其他错误情况
                let error_description = result
                    .get("error_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error");
                HttpResponse::BadRequest().json(CheckQrStatusResponse {
                    status: "error".to_string(),
                    session_token: None,
                    message: Some(error_description.to_string()),
                })
            }
        }
        Err(e) => {
            log::error!("Error checking QR status with TapTap: {e:?}");
            HttpResponse::InternalServerError().json(CheckQrStatusResponse {
                status: "error".to_string(),
                session_token: None,
                message: Some(format!("Error checking QR status with TapTap: {e}")),
            })
        }
    }
}
