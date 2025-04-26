use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use super::SongRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RksRecord {
    pub song_id: String,
    pub song_name: String,
    pub difficulty: String,
    pub difficulty_value: f64,
    pub acc: f64,
    pub score: Option<f64>,
    pub rks: f64,
}

impl RksRecord {
    pub fn new(
        song_id: String,
        song_name: String,
        difficulty: String,
        difficulty_value: f64,
        record: &SongRecord,
    ) -> Self {
        let acc = record.acc.unwrap_or(0.0);
        let rks = if acc >= 70.0 {
            ((acc - 55.0) / 45.0).powf(2.0) * difficulty_value
        } else {
            0.0
        };

        Self {
            song_id,
            song_name,
            difficulty,
            difficulty_value,
            acc,
            score: record.score,
            rks,
        }
    }
}

impl PartialEq for RksRecord {
    fn eq(&self, other: &Self) -> bool {
        self.rks == other.rks
    }
}

impl Eq for RksRecord {}

impl PartialOrd for RksRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RksRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rks.partial_cmp(&other.rks).unwrap_or(Ordering::Equal).reverse()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RksResult {
    pub records: Vec<RksRecord>,
}

impl RksResult {
    pub fn new(records: Vec<RksRecord>) -> Self {
        let mut all_records = records.clone();
        all_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));
        
        Self {
            records: all_records,
        }
    }
} 