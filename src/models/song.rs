use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// 歌曲信息结构体
/// 包含歌曲的基本信息，如ID、名称、作曲者、插画师和各难度的谱师
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SongInfo {
    /// 歌曲ID
    pub id: String,
    /// 歌曲名称
    pub song: String,
    /// 作曲者
    pub composer: String,
    /// 插画师（可选）
    pub illustrator: Option<String>,
    /// EZ难度谱师（可选）
    #[serde(rename = "EZ")]
    pub ez_charter: Option<String>,
    /// HD难度谱师（可选）
    #[serde(rename = "HD")]
    pub hd_charter: Option<String>,
    /// IN难度谱师（可选）
    #[serde(rename = "IN")]
    pub in_charter: Option<String>,
    /// AT难度谱师（可选）
    #[serde(rename = "AT")]
    pub at_charter: Option<String>,
}

/// 歌曲难度信息结构体
/// 包含歌曲各难度的定数信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SongDifficulty {
    /// 歌曲ID
    pub id: String,
    /// EZ难度定数（可选）
    #[serde(rename = "EZ")]
    pub ez: Option<f64>,
    /// HD难度定数（可选）
    #[serde(rename = "HD")]
    pub hd: Option<f64>,
    /// IN难度定数（可选）
    #[serde(rename = "IN")]
    pub inl: Option<f64>,
    /// AT难度定数（可选）
    #[serde(rename = "AT")]
    pub at: Option<f64>,
}

/// 歌曲昵称结构体
/// 包含歌曲的别名信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SongNickname {
    /// 歌曲ID
    pub id: String,
    /// 歌曲的昵称列表
    pub nicknames: Vec<String>,
}

/// 歌曲查询参数结构体
/// 用于搜索歌曲时的查询条件
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SongQuery {
    /// 歌曲ID（可选）
    pub song_id: Option<String>,
    /// 歌曲名称（可选）
    pub song_name: Option<String>,
    /// 歌曲昵称（可选）
    pub nickname: Option<String>,
    /// 难度级别（可选）
    pub difficulty: Option<String>,
}

/// 歌曲昵称映射类型
/// 用于存储歌曲名称到其昵称列表的映射
pub type NicknameMap = HashMap<String, Vec<String>>;
