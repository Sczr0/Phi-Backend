use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImageCounter {
    pub id: Option<i64>,
    pub image_type: String,
    pub count: i64,
    pub last_updated: String,
}

impl ImageCounter {
    pub fn new(image_type: String, count: i64) -> Self {
        Self {
            id: None,
            image_type,
            count,
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }
}