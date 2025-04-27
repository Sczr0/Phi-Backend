use std::collections::HashMap;
use reqwest::Client;
use crate::models::{GameSave, RksResult, SongRecord};
use crate::utils::error::{AppError, AppResult};
use crate::utils::save_parser::{parse_save, parse_save_with_difficulty};

// 导入UserProfile模型
use crate::models::UserProfile;

// Phigros API相关的常量
const BASE_URL: &str = "https://rak3ffdi.cloud.tds1.tapapis.cn/1.1/";
const LC_ID: &str = "rAK3FfdieFob2Nn8Am";
const LC_KEY: &str = "Qr9AEqtuoSVS3zeD6iVbM4ZC0AtkJcQ89tywVyi0";
const USER_AGENT: &str = "LeanCloud-CSharp-SDK/1.0.3";

// Phigros服务，管理与Phigros API交互、存档解析等
#[derive(Clone)]
pub struct PhigrosService {
    client: Client,
}

impl PhigrosService {
    // 创建新的Phigros服务
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
    
    // 获取存档数据并解析
    pub async fn get_save(&self, token: &str) -> AppResult<GameSave> {
        // 获取游戏存档
        let save_data = self.fetch_save(token).await?;
        
        // 解析存档
        parse_save(&save_data)
    }
    
    // 获取存档数据并解析，添加难度和RKS信息
    pub async fn get_save_with_difficulty(&self, token: &str) -> AppResult<GameSave> {
        // 获取游戏存档
        let save_data = self.fetch_save(token).await?;
        
        // 解析存档并添加难度和RKS信息
        parse_save_with_difficulty(&save_data)
    }
    
    // 获取RKS计算结果
    pub async fn get_rks(&self, token: &str) -> AppResult<RksResult> {
        log::debug!("进入 get_rks 服务函数");
        // 获取带难度信息的存档
        let save = self.get_save_with_difficulty(token).await?;
        log::debug!("get_rks: 已获取带难度信息的存档");
        
        // --- 从存档记录中提取并计算 RKS 记录 --- 
        let game_record = save.game_record.as_ref()
            .ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;
        log::debug!("get_rks: 从存档中获取 GameRecord，包含 {} 首歌曲", game_record.len());
        
        let mut rks_records = Vec::new();
        
        // 遍历所有歌曲记录
        log::debug!("get_rks: 开始遍历 GameRecord 并创建 RksRecord 列表...");
        for (song_id, difficulties) in game_record {
            let song_name = crate::utils::data_loader::get_song_name_by_id(song_id)
                .unwrap_or_else(|| song_id.clone());
            
            // 遍历所有难度记录
            for (diff_name, record) in difficulties {
                // 确保有 acc 和 difficulty 值
                if let (Some(acc), Some(difficulty)) = (record.acc, record.difficulty) {
                    if acc >= 70.0 && difficulty > 0.0 { // 只考虑有效成绩
                         log::trace!("get_rks: 为 '{}' - '{}' 创建 RksRecord", song_id, diff_name);
                        let rks_record = crate::models::RksRecord::new(
                            song_id.clone(),
                            song_name.clone(),
                            diff_name.clone(),
                            difficulty,
                            record, // 传递 SongRecord 引用
                        );
                        rks_records.push(rks_record);
                    }
                }
            }
        }
        log::debug!("get_rks: 共创建了 {} 条 RksRecord", rks_records.len());
        // --- 结束 RKS 记录提取 ---

        // 使用 RksResult::new 来排序并包装记录
        log::debug!("get_rks: 调用 RksResult::new 进行排序和包装...");
        let result = RksResult::new(rks_records);
        log::debug!("get_rks: RksResult 创建完成，包含 {} 条记录", result.records.len());
        Ok(result)
    }
    
    // 获取特定歌曲的成绩
    pub async fn get_song_record(&self, token: &str, song_id: &str, difficulty: Option<&str>) -> AppResult<HashMap<String, SongRecord>> {
        // 获取带难度信息的存档
        let save = self.get_save_with_difficulty(token).await?;
        
        // 从存档中查找歌曲成绩
        let game_record = save.game_record.ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;
        
        let song_records = game_record.get(song_id)
            .ok_or_else(|| AppError::SongNotFound(song_id.to_string()))?;
        
        // 如果指定了难度，只返回该难度的成绩
        if let Some(diff) = difficulty {
            let mut result = HashMap::new();
            
            // 尝试获取指定难度的成绩
            let record = song_records.get(diff)
                .ok_or_else(|| AppError::Other(format!("没有找到歌曲 {} 的 {} 难度记录", song_id, diff)))?;
            
            result.insert(diff.to_string(), record.clone());
            return Ok(result);
        }
        
        // 否则返回所有难度的成绩
        Ok(song_records.clone())
    }
    
    // 从Phigros云端获取存档数据
    async fn fetch_save(&self, token: &str) -> AppResult<Vec<u8>> {
        // 获取存档摘要，以获取存档URL
        log::debug!("开始获取存档摘要...");
        let summary = self.fetch_summary(token).await?;
        log::debug!("成功获取存档摘要");
        
        // 获取存档URL和校验和
        let url = summary["results"][0]["gameFile"]["url"].as_str()
            .ok_or_else(|| AppError::Other("无法获取存档URL".to_string()))?;
        log::debug!("获取到存档 URL: {}", url);
        
        let expected_checksum = summary["results"][0]["gameFile"]["metaData"]["_checksum"].as_str()
            .ok_or_else(|| AppError::Other("无法获取存档校验和".to_string()))?;
        log::debug!("获取到预期校验和: {}", expected_checksum);
        
        // 下载存档数据
        log::debug!("开始下载存档数据...");
        let save_data = self.download_save(url).await?;
        log::debug!("成功下载存档数据，大小: {} 字节", save_data.len());
        
        // 检查存档大小
        if save_data.len() <= 30 {
            log::error!("存档大小不足 30 字节 ({})，可能已损坏或获取失败", save_data.len());
            return Err(AppError::InvalidSaveSize(save_data.len()));
        }
        
        // 计算并验证校验和
        let actual_checksum = self.calculate_checksum(&save_data);
        log::debug!("计算出的实际校验和: {}", actual_checksum);
        if expected_checksum != actual_checksum {
            log::error!("存档校验和不匹配！预期: {}, 实际: {}", expected_checksum, actual_checksum);
            return Err(AppError::ChecksumMismatch {
                expected: expected_checksum.to_string(),
                actual: actual_checksum,
            });
        }
        log::debug!("存档校验和匹配成功");
        
        Ok(save_data)
    }
    
    // 获取存档摘要信息
    async fn fetch_summary(&self, token: &str) -> AppResult<serde_json::Value> {
        let response = self.client.get(format!("{}classes/_GameSave?limit=1", BASE_URL))
            .header("X-LC-Id", LC_ID)
            .header("X-LC-Key", LC_KEY)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .header("X-LC-Session", token)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(AppError::Other(format!(
                "获取存档摘要失败: HTTP {}",
                response.status()
            )));
        }
        
        let summary = response.json::<serde_json::Value>().await?;
        Ok(summary)
    }
    
    // 下载存档数据
    async fn download_save(&self, url: &str) -> AppResult<Vec<u8>> {
        let response = self.client.get(url)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(AppError::Other(format!(
                "下载存档失败: HTTP {}",
                response.status()
            )));
        }
        
        let save_data = response.bytes().await?.to_vec();
        Ok(save_data)
    }
    
    // 计算存档的MD5校验和
    fn calculate_checksum(&self, data: &[u8]) -> String {
        use md5::{Md5, Digest};
        
        let mut hasher = Md5::new();
        hasher.update(data);
        let result = hasher.finalize();
        format!("{:x}", result)
    }

    // 获取用户Profile信息
    pub async fn get_profile(&self, token: &str) -> AppResult<UserProfile> {
        log::debug!("开始获取用户 Profile 信息...");
        let response = self.client.get(format!("{}users/me", BASE_URL))
            .header("X-LC-Id", LC_ID)
            .header("X-LC-Key", LC_KEY)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .header("X-LC-Session", token)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "无法读取错误信息".to_string());
            log::error!("获取 Profile 失败: HTTP {}, 响应: {}", status, error_text);
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(AppError::AuthError("Token 无效或已过期".to_string()));
            }
            return Err(AppError::Other(format!(
                "获取 Profile 失败: HTTP {}",
                status
            )));
        }

        match response.json::<UserProfile>().await {
            Ok(profile) => {
                log::debug!("成功获取 Profile: {}", profile.nickname);
                Ok(profile)
            }
            Err(e) => {
                log::error!("解析 Profile JSON 失败: {}", e);
                Err(AppError::Other(format!("解析 Profile 响应失败: {}", e)))
            }
        }
    }
} 