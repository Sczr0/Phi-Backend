use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// B30成绩记录结构体
/// 用于在B30列表中显示的单条成绩记录
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct B30Record {
    /// 歌曲ID
    pub song_id: String,
    /// 难度字符串，如 "IN", "AT"
    pub difficulty_str: String,
    /// 分数（可选）
    pub score: Option<f64>,
    /// 准确度（可选）
    pub acc: Option<f64>,
    /// 是否Full Combo（可选）
    pub fc: Option<bool>,
    /// 难度定数（可选）
    pub difficulty: Option<f64>,
    /// RKS值（可选）
    pub rks: Option<f64>,
    /// 是否为All Perfect
    pub is_ap: bool,
}

/// B30计算结果结构体
/// 包含B30计算的最终结果
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct B30Result {
    /// 最终计算出的RKS值
    pub overall_rks: f64,
    /// RKS最高的27个谱面记录
    pub top_27: Vec<B30Record>,
    /// RKS最高的3个AP谱面记录
    pub top_3_ap: Vec<B30Record>,
}
