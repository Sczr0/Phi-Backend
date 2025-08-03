use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// 用于在B30列表中显示的单条成绩记录
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct B30Record {
    pub song_id: String,
    pub difficulty_str: String, // e.g., "IN", "AT"
    pub score: Option<f64>,
    pub acc: Option<f64>,
    pub fc: Option<bool>,
    pub difficulty: Option<f64>,
    pub rks: Option<f64>,
    pub is_ap: bool,
}

// B30计算结果
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct B30Result {
    pub overall_rks: f64,       // 最终计算出的 RKS
    pub top_27: Vec<B30Record>, // RKS最高的27个谱面记录
    pub top_3_ap: Vec<B30Record>, // RKS最高的3个AP谱面记录
}