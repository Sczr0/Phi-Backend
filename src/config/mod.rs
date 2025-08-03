use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub cors_allowed_origins: Vec<String>,
    pub info_data_path: String,
    pub difficulty_file: String,
    pub info_file: String,
    pub nicklist_file: String,
    pub taptap_client_id: String,
    pub leancloud_app_key: String,
    pub leancloud_client_id: String,
    pub leancloud_base_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            cors_allowed_origins: vec!["*".to_string()],
            info_data_path: env::var("INFO_DATA_PATH").unwrap_or_else(|_| "../info".to_string()),
            difficulty_file: env::var("DIFFICULTY_FILE").unwrap_or_else(|_| "difficulty.csv".to_string()),
            info_file: env::var("INFO_FILE").unwrap_or_else(|_| "info.csv".to_string()),
            nicklist_file: env::var("NICKLIST_FILE").unwrap_or_else(|_| "nicklist.yaml".to_string()),
            taptap_client_id: env::var("TAPTAP_CLIENT_ID").unwrap_or_else(|_| "rAK3FfdieFob2Nn8Am".to_string()),
            leancloud_app_key: env::var("LEANCLOUD_APP_KEY").unwrap_or_else(|_| "Qr9AEqtuoSVS3zeD6iVbM4ZC0AtkJcQ89tywVyi0".to_string()),
            leancloud_client_id: env::var("LEANCLOUD_CLIENT_ID").unwrap_or_else(|_| "rAK3FfdieFob2Nn8Am".to_string()),
            leancloud_base_url: env::var("LEANCLOUD_BASE_URL").unwrap_or_else(|_| "https://rak3ffdi.cloud.tds1.tapapis.cn/1.1".to_string()),
        }
    }
}

lazy_static! {
    pub static ref CONFIG: Arc<AppConfig> = Arc::new(AppConfig::default());
}

// AES加密配置
pub const AES_KEY_BASE64: &str = "6Jaa0qVAJZuXkZCLiOa/Ax5tIZVu+taKUN1V1nqwkks=";
pub const AES_IV_BASE64: &str = "Kk/wisgNYwcAV8WVGMgyUw==";

// 存档文件列表
#[allow(dead_code)]
pub const SAVE_FILE_LIST: [&str; 5] = [
    "gameKey",
    "gameProgress",
    "gameRecord",
    "settings",
    "user",
];
