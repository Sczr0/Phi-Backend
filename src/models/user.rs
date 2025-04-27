use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    #[serde(rename = "objectId")]
    pub object_id: String,
    pub nickname: String,
    // 可以根据需要添加其他从 /users/me 返回的字段
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PhigrosUser {
    pub qq: String,
    pub session_token: String,
    pub nickname: Option<String>,
    pub last_update: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRequest {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindRequest {
    pub qq: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifierRequest {
    pub token: Option<String>,
    pub qq: Option<String>,
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
    pub qq: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub code: u32,
    pub status: String,
    pub message: Option<String>,
    pub data: Option<T>,
} 