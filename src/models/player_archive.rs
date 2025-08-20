use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

/// 玩家存档结构体
/// 包含玩家的所有游戏数据和成绩记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerArchive {
    /// 玩家ID
    pub player_id: String,
    /// 玩家名称
    pub player_name: String,
    /// 玩家RKS值
    pub rks: f64,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 最佳成绩映射，键为"歌曲ID-难度"
    pub best_scores: HashMap<String, ChartScore>,
    /// 排序后的BestN成绩列表
    pub best_n_scores: Vec<ChartScore>,
    /// 谱面历史记录映射，键为"歌曲ID-难度"
    pub chart_histories: HashMap<String, Vec<ChartScoreHistory>>,
    /// 推分ACC映射（可选），键为"歌曲ID-难度"，值为推分ACC
    pub push_acc_map: Option<HashMap<String, f64>>,
}

/// 玩家基本信息结构体
/// 包含玩家的基本信息
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PlayerBasicInfo {
    /// 玩家ID
    pub player_id: String,
    /// 玩家名称
    pub player_name: String,
    /// 玩家RKS值
    pub rks: f64,
    /// 更新时间
    pub update_time: DateTime<Utc>,
}

/// 谱面成绩结构体
/// 包含单个谱面的具体成绩信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, FromRow)]
pub struct ChartScore {
    /// 歌曲ID
    pub song_id: String,
    /// 歌曲名称
    pub song_name: String,
    /// 难度级别
    pub difficulty: String,
    /// 难度定数
    pub difficulty_value: f64,
    /// 分数
    pub score: f64,
    /// 准确度
    pub acc: f64,
    /// RKS值
    pub rks: f64,
    /// 是否Full Combo
    pub is_fc: bool,
    /// 是否Phi (ACC >= 100%)
    pub is_phi: bool,
    /// 游玩时间
    pub play_time: DateTime<Utc>,
}

impl ChartScore {
    /// 从RKS记录创建谱面成绩
    ///
    /// # 参数
    /// * `record` - RKS记录
    /// * `is_fc` - 是否Full Combo
    /// * `_is_phi` - 是否Phi（参数保留但实际通过ACC计算）
    #[allow(dead_code)]
    pub fn from_rks_record(
        record: &crate::models::rks::RksRecord,
        is_fc: bool,
        _is_phi: bool,
    ) -> Self {
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

/// 谱面成绩历史记录结构体
/// 包含谱面成绩的历史变更信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartScoreHistory {
    /// 分数
    pub score: f64,
    /// 准确度
    pub acc: f64,
    /// RKS值
    pub rks: f64,
    /// 是否Full Combo
    pub is_fc: bool,
    /// 是否Phi
    pub is_phi: bool,
    /// 游玩时间
    pub play_time: DateTime<Utc>,
}

/// 存档配置结构体
/// 用于配置存档服务的行为
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    /// 是否存储推分ACC
    pub store_push_acc: bool,
    /// Best N成绩数量
    pub best_n_count: u32,
    /// 历史记录最大数量
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

/// RKS排行榜条目结构体
/// 包含排行榜中单个玩家的信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RKSRankingEntry {
    /// 玩家ID
    pub player_id: String,
    /// 玩家名称
    pub player_name: String,
    /// 玩家RKS值
    pub rks: f64,
    /// Best 27平均分（可选）
    pub b27_rks: Option<f64>,
    /// AP Top 3平均分（可选）
    pub ap3_rks: Option<f64>,
    /// AP总数（可选）
    pub ap_count: Option<usize>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
}
