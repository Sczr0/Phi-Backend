use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 预测定数结构体
/// 包含歌曲各难度的预测定数
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PredictedConstants {
    /// EZ难度预测定数（可选）
    pub ez: Option<f32>,
    /// HD难度预测定数（可选）
    pub hd: Option<f32>,
    /// IN难度预测定数（可选）
    pub inl: Option<f32>,
    /// AT难度预测定数（可选）
    pub at: Option<f32>,
}

/// 预测定数响应结构体
/// 用于返回单个歌曲难度的预测定数
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PredictionResponse {
    /// 歌曲ID
    pub song_id: String,
    /// 难度级别
    pub difficulty: String,
    /// 预测定数（可选）
    pub predicted_constant: Option<f32>,
}
