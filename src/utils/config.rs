use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use hex;

use crate::utils::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub database_url: String,
    pub server_port: u16,
    pub log_level: String,
    pub aes_key: String,
    pub token_secret: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite:data.db".to_string(),
            server_port: 8080,
            log_level: "info".to_string(),
            aes_key: "0123456789abcdef0123456789abcdef".to_string(),  // 示例key，生产环境应当更改
            token_secret: "phigros_secret_key_example".to_string(),    // 示例secret，生产环境应当更改
        }
    }
}

impl AppConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let mut file = File::open(path).map_err(|e| {
            AppError::ConfigError(format!("无法打开配置文件: {}", e))
        })?;
        
        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            AppError::ConfigError(format!("读取配置文件失败: {}", e))
        })?;
        
        serde_json::from_str(&contents).map_err(|e| {
            AppError::ConfigError(format!("解析配置文件失败: {}", e))
        })
    }
    
    pub fn get_aes_key_bytes(&self) -> AppResult<Vec<u8>> {
        let key_bytes = hex::decode(&self.aes_key).map_err(|e| {
            AppError::ConfigError(format!("解析AES密钥失败: {}", e))
        })?;
        
        if key_bytes.len() != 16 {
            return Err(AppError::ConfigError(
                format!("AES密钥长度错误，需要16字节，实际为{}字节", key_bytes.len())
            ));
        }
        
        Ok(key_bytes)
    }
}

// 全局配置实例
static mut CONFIG: Option<AppConfig> = None;

pub fn init_config<P: AsRef<Path>>(path: Option<P>) -> AppResult<()> {
    let config = match path {
        Some(p) => AppConfig::from_file(p)?,
        None => AppConfig::default(),
    };
    
    unsafe {
        CONFIG = Some(config);
    }
    
    Ok(())
}

pub fn get_config() -> AppResult<AppConfig> {
    unsafe {
        CONFIG.clone().ok_or_else(|| AppError::ConfigError("配置未初始化".to_string()))
    }
} 