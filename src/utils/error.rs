use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AppError {
    #[error("AES错误: {0}")]
    AesError(String),
    
    #[error("无效的会话令牌")]
    InvalidSessionToken,
    
    #[error("无效的存档大小: {0}字节")]
    InvalidSaveSize(usize),
    
    #[error("存档校验和不匹配: 期望 {expected}, 实际 {actual}")]
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    
    #[error("找不到歌曲: {0}")]
    SongNotFound(String),
    
    #[error("查询匹配到多个歌曲: {0}")]
    AmbiguousSongName(String),
    
    #[error("未找到用户绑定: {0}")]
    UserBindingNotFound(String),
    
    #[error("未找到用户: {0}")]
    UserNotFound(String),
    
    #[error("绑定已存在: {0}")]
    BindingAlreadyExists(String),
    
    #[error("简介验证失败: {0}")]
    ProfileVerificationFailed(String),
    
    #[error("Token验证失败: {0}")]
    TokenVerificationFailed(String),
    
    #[error("验证码已过期")]
    VerificationCodeExpired,
    
    #[error("无效的验证码")]
    VerificationCodeInvalid,
    
    #[error("未找到待处理的验证请求")]
    VerificationCodeNotFound,
    
    #[error("数据库错误: {0}")]
    DatabaseError(String),
    
    #[error("错误的请求: {0}")]
    BadRequest(String),
    
    #[error("解码错误: {0}")]
    DecodeError(#[from] base64::DecodeError),
    
    #[error("ZIP错误: {0}")]
    ZipError(#[from] zip::result::ZipError),
    
    #[error("IO错误: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("HTTP请求错误: {0}")]
    ReqwestError(#[from] reqwest::Error),
    
    #[error("Serde JSON错误: {0}")]
    SerdeJsonError(#[from] serde_json::Error),
    
    #[error("Serde YAML错误: {0}")]
    SerdeYamlError(#[from] serde_yaml::Error),
    
    #[error("CSV错误: {0}")]
    CsvError(#[from] csv::Error),
    
    #[error("其他错误: {0}")]
    Other(String),
    
    #[error("数据库错误: {0}")]
    DbError(sqlx::Error),
    
    #[error("认证错误: {0}")]
    AuthError(String),
    
    #[error("存档解密错误: {0}")]
    SaveDecryptError(String),
    
    #[error("配置错误: {0}")]
    ConfigError(String),
    
    #[error("验证错误: {0}")]
    ValidationError(String),
    
    #[error("内部错误: {0}")]
    InternalError(String),
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (status_code, error_type) = match self {
            AppError::AesError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "aes_error"),
            AppError::InvalidSessionToken => (actix_web::http::StatusCode::UNAUTHORIZED, "invalid_session_token"),
            AppError::InvalidSaveSize(_) => (actix_web::http::StatusCode::BAD_REQUEST, "invalid_save_size"),
            AppError::ChecksumMismatch { .. } => (actix_web::http::StatusCode::BAD_REQUEST, "checksum_mismatch"),
            AppError::SongNotFound(_) => (actix_web::http::StatusCode::NOT_FOUND, "song_not_found"),
            AppError::AmbiguousSongName(_) => (actix_web::http::StatusCode::BAD_REQUEST, "ambiguous_song_name"),
            AppError::UserBindingNotFound(_) => (actix_web::http::StatusCode::NOT_FOUND, "user_binding_not_found"),
            AppError::UserNotFound(_) => (actix_web::http::StatusCode::NOT_FOUND, "user_not_found"),
            AppError::BindingAlreadyExists(_) => (actix_web::http::StatusCode::CONFLICT, "binding_already_exists"),
            AppError::ProfileVerificationFailed(_) => (actix_web::http::StatusCode::BAD_REQUEST, "profile_verification_failed"),
            AppError::TokenVerificationFailed(_) => (actix_web::http::StatusCode::UNAUTHORIZED, "token_verification_failed"),
            AppError::VerificationCodeExpired => (actix_web::http::StatusCode::BAD_REQUEST, "verification_code_expired"),
            AppError::VerificationCodeInvalid => (actix_web::http::StatusCode::BAD_REQUEST, "verification_code_invalid"),
            AppError::VerificationCodeNotFound => (actix_web::http::StatusCode::NOT_FOUND, "verification_code_not_found"),
            AppError::DatabaseError(_) | AppError::DbError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
            AppError::BadRequest(_) => (actix_web::http::StatusCode::BAD_REQUEST, "bad_request"),
            AppError::DecodeError(_) => (actix_web::http::StatusCode::BAD_REQUEST, "decode_error"),
            AppError::ZipError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "zip_error"),
            AppError::IoError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "io_error"),
            AppError::ReqwestError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "request_error"),
            AppError::SerdeJsonError(_) => (actix_web::http::StatusCode::BAD_REQUEST, "serialization_error"),
            AppError::SerdeYamlError(_) => (actix_web::http::StatusCode::BAD_REQUEST, "serialization_error"),
            AppError::CsvError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "csv_error"),
            AppError::Other(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "other_error"),
            AppError::AuthError(_) => (actix_web::http::StatusCode::UNAUTHORIZED, "authentication_error"),
            AppError::SaveDecryptError(_) => (actix_web::http::StatusCode::BAD_REQUEST, "decryption_error"),
            AppError::ConfigError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "configuration_error"),
            AppError::ValidationError(_) => (actix_web::http::StatusCode::BAD_REQUEST, "validation_error"),
            AppError::InternalError(_) => (actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        HttpResponse::build(status_code)
            .json(ErrorResponse {
                error: error_type.to_string(),
                message: self.to_string(),
            })
    }
} 