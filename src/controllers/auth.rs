use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use qrcode::QrCode;
use base64::{engine::general_purpose, Engine as _};
use uuid::Uuid;
use crate::services::taptap::{TapTapService, TapTapQrCodeResponse};
use lazy_static::lazy_static;
use utoipa::{ToSchema, path};

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
            store.insert(qr_id.clone(), QrCodeState {
                device_code: qr_code_data.device_code.clone(),
                device_id: device_id.clone(),
                status: "pending".to_string(),
                session_token: None,
                created_at: chrono::Utc::now(),
            });

            let qr_id_clone = qr_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                let mut store = QR_CODE_STORE.lock().unwrap();
                store.remove(&qr_id_clone);
                log::info!("QR code {} expired and removed from store.", qr_id_clone);
            });

            let code = QrCode::new(&qr_code_data.qrcode_url).unwrap();
            let width = code.width();
            let image_data = code.to_colors();
            let mut img_buf = image::ImageBuffer::new(width as u32, width as u32);
            for (x, y, pixel) in img_buf.enumerate_pixels_mut() {
                let index = y as usize * width + x as usize;
                *pixel = image::Luma([if image_data[index] == qrcode::Color::Dark { 0 } else { 255 }]);
            }
            let mut bytes: Vec<u8> = Vec::new();
            image::DynamicImage::ImageLuma8(img_buf).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageOutputFormat::Png).unwrap();
            let qr_code_image = format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&bytes));

            HttpResponse::Ok().json(GenerateQrCodeResponse { qr_id, qr_code_image })
        },
        Err(e) => {
            log::error!("Error generating QR code: {:?}", e);
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
    let mut store = QR_CODE_STORE.lock().unwrap();
    let stored_data_option = store.get(&qr_id).cloned();

    if stored_data_option.is_none() {
        return HttpResponse::NotFound().json(CheckQrStatusResponse {
            status: "expired".to_string(),
            session_token: None,
            message: Some("QR Code not found or expired.".to_string()),
        });
    }

    let mut stored_data = stored_data_option.unwrap();

    if stored_data.status == "success" {
        return HttpResponse::Ok().json(CheckQrStatusResponse {
            status: "success".to_string(),
            session_token: stored_data.session_token.clone(),
            message: None,
        });
    }

    if (chrono::Utc::now() - stored_data.created_at).num_seconds() > 300 {
        store.remove(&qr_id);
        return HttpResponse::NotFound().json(CheckQrStatusResponse {
            status: "expired".to_string(),
            session_token: None,
            message: Some("QR Code expired.".to_string()),
        });
    }

    let taptap_service = TapTapService::new();

    match taptap_service.check_qr_code_result(&stored_data.device_code, &stored_data.device_id).await {
        Ok(result) => {
            if let Some(session_token) = result.get("sessionToken").and_then(|v| v.as_str()) {
                stored_data.status = "success".to_string();
                stored_data.session_token = Some(session_token.to_string());
                store.insert(qr_id.clone(), stored_data);
                HttpResponse::Ok().json(CheckQrStatusResponse {
                    status: "success".to_string(),
                    session_token: Some(session_token.to_string()),
                    message: None,
                })
            } else if result.get("error").and_then(|v| v.as_str()) == Some("authorization_waiting") {
                stored_data.status = "scanned".to_string();
                store.insert(qr_id.clone(), stored_data);
                HttpResponse::Ok().json(CheckQrStatusResponse {
                    status: "scanned".to_string(),
                    session_token: None,
                    message: None,
                })
            } else if result.get("error").and_then(|v| v.as_str()) == Some("authorization_pending") {
                HttpResponse::Ok().json(CheckQrStatusResponse {
                    status: "pending".to_string(),
                    session_token: None,
                    message: None,
                })
            } else {
                log::error!("Unknown error from TapTap during QR check: {:?}", result);
                let error_description = result.get("error_description").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                HttpResponse::BadRequest().json(CheckQrStatusResponse {
                    status: "error".to_string(),
                    session_token: None,
                    message: Some(error_description.to_string()),
                })
            }
        },
        Err(e) => {
            log::error!("Error checking QR status with TapTap: {:?}", e);
            HttpResponse::InternalServerError().json(CheckQrStatusResponse {
                status: "error".to_string(),
                session_token: None,
                message: Some(format!("Error checking QR status with TapTap: {}", e)),
            })
        }
    }
}
