use serde::{Deserialize, Serialize};
// use std::collections::HashMap; // 移除未使用的导入

// Placeholder for predictions module - 现在添加实际定义

#[derive(Debug, Serialize, Deserialize, Clone)] // 添加 Deserialize 用于加载，Serialize 用于响应，Debug 和 Clone 用于方便性
pub struct PredictedConstants {
    pub ez: Option<f32>,
    pub hd: Option<f32>,
    pub inl: Option<f32>, // 注意这里代码中使用的是 inl 而不是 in
    pub at: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)] // 添加 Serialize 用于响应，Debug, Deserialize 和 Clone 用于方便性
pub struct PredictionResponse {
    pub song_id: String,
    pub difficulty: String,
    pub predicted_constant: Option<f32>,
} 