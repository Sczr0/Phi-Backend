use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GameSave {
    pub game_key: Option<HashMap<String, serde_json::Value>>,
    pub game_progress: Option<HashMap<String, serde_json::Value>>,
    pub game_record: Option<HashMap<String, HashMap<String, SongRecord>>>,
    pub settings: Option<HashMap<String, serde_json::Value>>,
    pub user: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct SongRecord {
    pub score: Option<f64>,
    pub acc: Option<f64>,
    pub fc: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rks: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SaveSummary {
    pub checksum: String,
    pub update_at: String,
    pub url: String,
    pub save_version: u8,
    pub challenge: u16,
    pub rks: f32,
    pub game_version: u8,
    pub avatar: String,
    pub ez: [u16; 3],
    pub hd: [u16; 3],
    pub inl: [u16; 3],
    pub at: [u16; 3],
}