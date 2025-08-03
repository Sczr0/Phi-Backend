use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerArchive {
    pub player_id: String,
    pub player_name: String,
    pub rks: f64,
    pub update_time: DateTime<Utc>,
    pub best_scores: HashMap<String, ChartScore>, // 键: "歌曲ID-难度"
    pub best_n_scores: Vec<ChartScore>, // 排序后的BestN成绩
    pub chart_histories: HashMap<String, Vec<ChartScoreHistory>>, // 键: "歌曲ID-难度"
    pub push_acc_map: Option<HashMap<String, f64>>, // 键: "歌曲ID-难度"，值: 推分ACC
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PlayerBasicInfo {
    pub player_id: String,
    pub player_name: String,
    pub rks: f64,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, FromRow)]
pub struct ChartScore {
    pub song_id: String,
    pub song_name: String,
    pub difficulty: String,
    pub difficulty_value: f64,
    pub score: f64,
    pub acc: f64,
    pub rks: f64,
    pub is_fc: bool,
    pub is_phi: bool,
    pub play_time: DateTime<Utc>,
}

impl ChartScore {
    #[allow(dead_code)]
    pub fn from_rks_record(record: &crate::models::rks::RksRecord, is_fc: bool, _is_phi: bool) -> Self {
        Self {
            song_id: record.song_id.clone(),
            song_name: record.song_name.clone(),
            difficulty: record.difficulty.clone(),
            difficulty_value: record.difficulty_value,
            score: record.score.unwrap_or(0.0),
            acc: record.acc,
            rks: record.rks,
            is_fc,
            is_phi: record.acc >= 100.0,
            play_time: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartScoreHistory {
    pub score: f64,
    pub acc: f64,
    pub rks: f64,
    pub is_fc: bool,
    pub is_phi: bool,
    pub play_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    pub store_push_acc: bool,
    pub best_n_count: u32,
    pub history_max_records: usize,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            store_push_acc: true,
            best_n_count: 27,
            history_max_records: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RKSRankingEntry {
    pub player_id: String,
    pub player_name: String,
    pub rks: f64,
    pub b27_rks: Option<f64>,   // Best 27 平均分
    pub ap3_rks: Option<f64>,   // AP Top 3 平均分
    pub ap_count: Option<usize>, // AP 总数
    pub update_time: DateTime<Utc>,
} 