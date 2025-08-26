use crate::models::rks::RksResult;
use crate::models::save::GameSave;

/// 封装了来自云端的完整存档信息
///
/// 这个结构体不仅包含了从存档文件解析出的 `GameSave`，
/// 还包含了计算出的 `RksResult`，以及从LeanCloud API
/// 获取的原始 `cloud_summary` 元数据。
#[derive(Debug, Clone)]
pub struct FullSaveData {
    pub rks_result: RksResult,
    pub save: GameSave,
    pub cloud_summary: serde_json::Value,
}