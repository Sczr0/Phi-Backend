use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    #[serde(rename = "objectId")]
    pub object_id: String,
    pub nickname: String,
    // 可以根据需要添加其他从 /users/me 返回的字段
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InternalUser {
    pub internal_id: String,
    pub nickname: Option<String>,
    pub update_time: String,
}

impl InternalUser {
    pub fn new(nickname: Option<String>) -> Self {
        Self {
            internal_id: Uuid::new_v4().to_string(),
            nickname,
            update_time: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PlatformBinding {
    pub id: Option<i64>,
    pub internal_id: String,
    pub platform: String,
    pub platform_id: String,
    pub session_token: String,
    pub bind_time: String,
}

impl PlatformBinding {
    pub fn new(internal_id: String, platform: String, platform_id: String, session_token: String) -> Self {
        Self {
            id: None,
            internal_id,
            platform: platform.to_lowercase(),
            platform_id,
            session_token,
            bind_time: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRequest {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindRequest {
    pub platform: String,
    pub platform_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifierRequest {
    pub token: Option<String>,
    pub platform: Option<String>,
    pub platform_id: Option<String>,
    pub verification_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnbindInitiateResponse {
    pub verification_code: String,
    pub expires_in_seconds: u64,
    pub message: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct UnbindVerificationCode {
    pub platform: String,
    pub platform_id: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenListResponse {
    pub internal_id: String,
    pub bindings: Vec<PlatformBindingInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformBindingInfo {
    pub platform: String,
    pub platform_id: String,
    pub session_token: String,
    pub bind_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub code: u32,
    pub status: String,
    pub message: Option<String>,
    pub data: Option<T>,
} 