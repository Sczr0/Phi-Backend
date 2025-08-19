use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// 游戏存档结构体
/// 包含游戏的各种数据，如密钥、进度、记录、设置和用户信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GameSave {
    /// 游戏密钥数据
    pub game_key: Option<HashMap<String, serde_json::Value>>,
    /// 游戏进度数据
    pub game_progress: Option<HashMap<String, serde_json::Value>>,
    /// 游戏记录数据，包含每首歌的成绩记录
    pub game_record: Option<HashMap<String, HashMap<String, SongRecord>>>,
    /// 游戏设置数据
    pub settings: Option<HashMap<String, serde_json::Value>>,
    /// 用户信息数据
    pub user: Option<HashMap<String, serde_json::Value>>,
}

/// 歌曲记录结构体
/// 包含单首歌曲在特定难度下的成绩信息
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct SongRecord {
    /// 分数
    pub score: Option<f64>,
    /// 准确度
    pub acc: Option<f64>,
    /// 是否Full Combo
    pub fc: Option<bool>,
    /// 难度定数（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<f64>,
    /// RKS值（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rks: Option<f64>,
}

/// 存档摘要结构体
/// 包含存档的元数据信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SaveSummary {
    /// 存档校验和
    pub checksum: String,
    /// 更新时间
    pub update_at: String,
    /// 存档下载URL
    pub url: String,
    /// 存档版本
    pub save_version: u8,
    /// 挑战模式等级
    pub challenge: u16,
    /// RKS值
    pub rks: f32,
    /// 游戏版本
    pub game_version: u8,
    /// 头像
    pub avatar: String,
    /// EZ难度统计数据 [游玩次数, 完成次数, Phi次数]
    pub ez: [u16; 3],
    /// HD难度统计数据 [游玩次数, 完成次数, Phi次数]
    pub hd: [u16; 3],
    /// IN难度统计数据 [游玩次数, 完成次数, Phi次数]
    pub inl: [u16; 3],
    /// AT难度统计数据 [游玩次数, 完成次数, Phi次数]
    pub at: [u16; 3],
}
