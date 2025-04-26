use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongInfo {
    pub id: String,
    pub song: String,
    pub composer: String,
    pub illustrator: Option<String>,
    #[serde(rename = "EZ")]
    pub ez_charter: Option<String>,
    #[serde(rename = "HD")]
    pub hd_charter: Option<String>,
    #[serde(rename = "IN")]
    pub in_charter: Option<String>,
    #[serde(rename = "AT")]
    pub at_charter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongDifficulty {
    pub id: String,
    #[serde(rename = "EZ")]
    pub ez: Option<f64>,
    #[serde(rename = "HD")]
    pub hd: Option<f64>,
    #[serde(rename = "IN")]
    pub inl: Option<f64>,
    #[serde(rename = "AT")]
    pub at: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongNickname {
    pub id: String,
    pub nicknames: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongQuery {
    pub song_id: Option<String>,
    pub song_name: Option<String>,
    pub nickname: Option<String>,
    pub difficulty: Option<String>,
}

pub type NicknameMap = HashMap<String, Vec<String>>; 