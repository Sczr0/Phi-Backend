use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// Placeholder for predictions module - 现在添加实际定义

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PredictedConstants {
    pub ez: Option<f32>,
    pub hd: Option<f32>,
    pub inl: Option<f32>,
    pub at: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PredictionResponse {
    pub song_id: String,
    pub difficulty: String,
    pub predicted_constant: Option<f32>,
}
