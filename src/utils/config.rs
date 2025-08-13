use hex;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::utils::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub database_url: String,
    pub server_port: u16,
    pub log_level: String,
    pub aes_key: String,
    pub token_secret: String,
    pub custom_footer_text: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite:data.db".to_string(),
            server_port: 8080,
            log_level: "info".to_string(),
            aes_key: "0123456789abcdef0123456789abcdef".to_string(),
            token_secret: "phigros_secret_key_example".to_string(),
            custom_footer_text: "Powered by Phi-Backend".to_string(),
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:phigros_bindings.db".to_string());
        let server_port = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .unwrap_or(8080);
        let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
        let custom_footer_text = std::env::var("CUSTOM_FOOTER_TEXT")
            .unwrap_or_else(|_| "Powered by Phi-Backend".to_string());

        Self {
            database_url,
            server_port,
            log_level,
            aes_key: "0123456789abcdef0123456789abcdef".to_string(), // 保持不变
            token_secret: "phigros_secret_key_example".to_string(),  // 保持不变
            custom_footer_text,
        }
    }

    #[allow(dead_code)]
    pub fn from_file<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let mut file = File::open(path)
            .map_err(|e| AppError::ConfigError(format!("无法打开配置文件: {e}")))?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|e| AppError::ConfigError(format!("读取配置文件失败: {e}")))?;

        serde_json::from_str(&contents)
            .map_err(|e| AppError::ConfigError(format!("解析配置文件失败: {e}")))
    }

    #[allow(dead_code)]
    pub fn get_aes_key_bytes(&self) -> AppResult<Vec<u8>> {
        let key_bytes = hex::decode(&self.aes_key)
            .map_err(|e| AppError::ConfigError(format!("解析AES密钥失败: {e}")))?;

        if key_bytes.len() != 16 {
            return Err(AppError::ConfigError(format!(
                "AES密钥长度错误，需要16字节，实际为{}字节",
                key_bytes.len()
            )));
        }

        Ok(key_bytes)
    }
}

// 全局配置实例
#[allow(dead_code)]
static mut CONFIG: Option<AppConfig> = None;

#[allow(dead_code)]
pub fn init_config() -> AppResult<()> {
    let config = AppConfig::from_env();

    unsafe {
        CONFIG = Some(config);
    }

    Ok(())
}

#[allow(dead_code)]
#[allow(static_mut_refs)]
pub fn get_config() -> AppResult<AppConfig> {
    unsafe {
        CONFIG
            .clone()
            .ok_or_else(|| AppError::ConfigError("配置未初始化".to_string()))
    }
}
