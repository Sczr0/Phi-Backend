use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use utoipa::ToSchema;

use crate::models::save::SongRecord;

/// RKS记录结构体
/// 包含单首歌曲在特定难度下的RKS相关信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RksRecord {
    /// 歌曲ID
    pub song_id: String,
    /// 歌曲名称
    pub song_name: String,
    /// 难度级别 (EZ, HD, IN, AT)
    pub difficulty: String,
    /// 难度定数
    pub difficulty_value: f64,
    /// 准确度
    pub acc: f64,
    /// 分数（可选）
    pub score: Option<f64>,
    /// RKS值
    pub rks: f64,
    /// 是否Full Combo
    pub is_fc: bool,
}

impl RksRecord {
    /// 创建新的RKS记录
    ///
    /// # 参数
    /// * `song_id` - 歌曲ID
    /// * `song_name` - 歌曲名称
    /// * `difficulty` - 难度级别
    /// * `difficulty_value` - 难度定数
    /// * `record` - 歌曲记录
    pub fn new(
        song_id: String,
        song_name: String,
        difficulty: String,
        difficulty_value: f64,
        record: &SongRecord,
    ) -> Self {
        let acc = record.acc.unwrap_or(0.0);
        let rks = crate::utils::rks_utils::calculate_chart_rks(acc, difficulty_value);
        let is_fc = record.fc.unwrap_or(false);

        Self {
            song_id,
            song_name,
            difficulty,
            difficulty_value,
            acc,
            score: record.score,
            rks,
            is_fc,
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
        self.rks
            .partial_cmp(&other.rks)
            .unwrap_or(Ordering::Equal)
            .reverse()
    }
}

/// RKS结果结构体
/// 包含排序后的RKS记录列表
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RksResult {
    /// RKS记录列表，按RKS值降序排列
    pub records: Vec<RksRecord>,
}

impl RksResult {
    /// 创建新的RKS结果
    ///
    /// # 参数
    /// * `records` - RKS记录列表
    pub fn new(records: Vec<RksRecord>) -> Self {
        let mut all_records = records.clone();
        all_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

        Self {
            records: all_records,
        }
    }
}
