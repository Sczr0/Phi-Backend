use crate::models::cloud_save::FullSaveData;
use crate::models::rks::RksResult;
use crate::models::save::{GameSave, SongRecord};
use crate::models::user::UserProfile;
use crate::utils::error::{AppError, AppResult};
use crate::utils::save_parser::{parse_save, parse_save_with_difficulty};
use reqwest::Client;
use std::time::Duration;
use std::collections::HashMap;
use serde_json::json;

// Phigros API相关的常量
const BASE_URL: &str = "https://rak3ffdi.cloud.tds1.tapapis.cn/1.1/";
const LC_ID: &str = "rAK3FfdieFob2Nn8Am";
const LC_KEY: &str = "Qr9AEqtuoSVS3zeD6iVbM4ZC0AtkJcQ89tywVyi0";
const USER_AGENT: &str = "LeanCloud-CSharp-SDK/1.0.3";

// 外部数据源API常量
const EXTERNAL_API_URL: &str = "http://phib19.top:8080/get/cloud/saves";

// Phigros服务，管理与Phigros API交互、存档解析等
#[derive(Clone)]
pub struct PhigrosService {
    client: Client,
}

impl PhigrosService {
    // 创建新的Phigros服务
    pub fn new() -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(12))
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(8)
            .build()
            .unwrap_or_else(|e| {
                log::warn!("构建 HTTP 客户端失败，回退默认设置: {e}");
                Client::new()
            });
        Self { client }
    }

    // 获取存档数据并解析
    pub async fn get_save(&self, token: &str) -> AppResult<GameSave> {
        let save_data = self.fetch_save(token).await?;
        parse_save(&save_data)
    }

    // 增强版：根据数据源获取存档数据并解析
    pub async fn get_save_with_source(&self, request: &crate::models::user::IdentifierRequest) -> AppResult<GameSave> {
        match request.data_source.as_deref() {
            Some("external") => {
                // 使用外部数据源
                let request_data = Self::build_external_request_data(request)?;
                let (_, save_data) = self.get_external_save_data(request_data).await?;
                parse_save(&save_data)
            },
            _ => {
                // 使用内部数据源（默认）
                let token = request.token.as_ref()
                    .ok_or_else(|| AppError::Other("内部数据源需要token".to_string()))?;
                let save_data = self.fetch_save(token).await?;
                parse_save(&save_data)
            }
        }
    }

    // 获取存档数据并解析，添加难度和RKS信息
    pub async fn get_save_with_difficulty(&self, token: &str) -> AppResult<GameSave> {
        let save_data = self.fetch_save(token).await?;
        parse_save_with_difficulty(&save_data)
    }

    // 增强版：根据数据源获取带难度定数的存档数据
    pub async fn get_save_with_difficulty_and_source(&self, request: &crate::models::user::IdentifierRequest) -> AppResult<GameSave> {
        match request.data_source.as_deref() {
            Some("external") => {
                // 使用外部数据源
                let request_data = Self::build_external_request_data(request)?;
                let (_, save_data) = self.get_external_save_data(request_data).await?;
                parse_save_with_difficulty(&save_data)
            },
            _ => {
                // 使用内部数据源（默认）
                let token = request.token.as_ref()
                    .ok_or_else(|| AppError::Other("内部数据源需要token".to_string()))?;
                let save_data = self.fetch_save(token).await?;
                parse_save_with_difficulty(&save_data)
            }
        }
    }

    // (优化后) 获取RKS计算结果，并同时返回用于计算的GameSave
    pub async fn get_rks(&self, token: &str) -> AppResult<(RksResult, GameSave)> {
        log::debug!("进入 get_rks 服务函数 (优化版)");
        let save = self.get_save_with_difficulty(token).await?;
        log::debug!("get_rks: 已获取带难度信息的存档");

        let result = self.calculate_rks_from_save(&save)?;
        log::debug!(
            "get_rks: RksResult 创建完成，包含 {} 条记录",
            result.records.len()
        );

        Ok((result, save))
    }

    // 增强版：根据数据源获取RKS计算结果
    pub async fn get_rks_with_source(
        &self,
        request: &crate::models::user::IdentifierRequest,
    ) -> AppResult<(RksResult, GameSave, String, String)> {
        log::debug!("进入 get_rks_with_source (重构版) 服务函数");

        let full_data = self.get_full_save_data_with_source(request).await?;

        let (player_id, player_name) = match request.data_source.as_deref() {
            Some("external") => {
                let summary = &full_data.cloud_summary["results"][0];
                let pid = summary["PlayerId"]
                    .as_str()
                    .unwrap_or("external:unknown")
                    .to_string();
                let pname = summary["nickname"].as_str().unwrap_or(&pid).to_string();
                (pid, pname)
            }
            _ => {
                // Internal
                let token = request
                    .token
                    .as_ref()
                    .ok_or_else(|| AppError::Other("内部数据源需要token".to_string()))?;
                let profile = self.get_profile(token).await?;
                (profile.object_id, profile.nickname)
            }
        };

        Ok((full_data.rks_result, full_data.save, player_id, player_name))
    }

    // 获取特定歌曲的成绩
    pub async fn get_song_record(
        &self,
        token: &str,
        song_id: &str,
        difficulty: Option<&str>,
    ) -> AppResult<HashMap<String, SongRecord>> {
        let save = self.get_save_with_difficulty(token).await?;

        let game_record = save
            .game_record
            .ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;

        let song_records = game_record
            .get(song_id)
            .ok_or_else(|| AppError::SongNotFound(song_id.to_string()))?;

        if let Some(diff) = difficulty {
            let mut result = HashMap::new();

            let record = song_records.get(diff).ok_or_else(|| {
                AppError::Other(format!("没有找到歌曲 {song_id} 的 {diff} 难度记录"))
            })?;

            result.insert(diff.to_string(), record.clone());
            return Ok(result);
        }

        Ok(song_records.clone())
    }

    // 增强版：根据数据源获取特定歌曲的成绩
    pub async fn get_song_record_with_source(
        &self,
        request: &crate::models::user::IdentifierRequest,
        song_id: &str,
        difficulty: Option<&str>,
    ) -> AppResult<HashMap<String, SongRecord>> {
        let save = self.get_save_with_difficulty_and_source(request).await?;

        let game_record = save
            .game_record
            .ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;

        let song_records = game_record
            .get(song_id)
            .ok_or_else(|| AppError::SongNotFound(song_id.to_string()))?;

        if let Some(diff) = difficulty {
            let mut result = HashMap::new();

            let record = song_records.get(diff).ok_or_else(|| {
                AppError::Other(format!("没有找到歌曲 {song_id} 的 {diff} 难度记录"))
            })?;

            result.insert(diff.to_string(), record.clone());
            return Ok(result);
        }

        Ok(song_records.clone())
    }

    // 从Phigros云端获取存档数据
    async fn fetch_save(&self, token: &str) -> AppResult<Vec<u8>> {
        log::debug!("开始获取存档摘要...");
        let summary = self.fetch_summary(token).await?;
        log::debug!("成功获取存档摘要");
        self.fetch_save_from_summary(&summary).await
    }

    // 新增的辅助函数，用于从已获取的摘要中下载并校验存档
    async fn fetch_save_from_summary(&self, summary: &serde_json::Value) -> AppResult<Vec<u8>> {
        let url = summary["results"][0]["gameFile"]["url"]
            .as_str()
            .ok_or_else(|| AppError::Other("无法获取存档URL".to_string()))?;
        log::debug!("获取到存档 URL: {url}");

        let expected_checksum = summary["results"][0]["gameFile"]["metaData"]["_checksum"]
            .as_str()
            .ok_or_else(|| AppError::Other("无法获取存档校验和".to_string()))?;
        log::debug!("获取到预期校验和: {expected_checksum}");

        log::debug!("开始下载存档数据...");
        let save_data = self.download_save(url).await?;
        log::debug!("成功下载存档数据，大小: {} 字节", save_data.len());

        if save_data.len() <= 30 {
            log::error!(
                "存档大小不足 30 字节 ({})，可能已损坏或获取失败",
                save_data.len()
            );
            return Err(AppError::InvalidSaveSize(save_data.len()));
        }

        let actual_checksum = self.calculate_checksum(&save_data);
        log::debug!("计算出的实际校验和: {actual_checksum}");
        if expected_checksum != actual_checksum {
            log::error!("存档校验和不匹配！预期: {expected_checksum}, 实际: {actual_checksum}");
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
        let response = self
            .client
            .get(format!("{BASE_URL}classes/_GameSave?limit=1"))
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
        let response = self.client.get(url).send().await?;

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
        use md5::{Digest, Md5};

        let mut hasher = Md5::new();
        hasher.update(data);
        let result = hasher.finalize();
        format!("{result:x}")
    }

    // 获取用户Profile信息
    pub async fn get_profile(&self, token: &str) -> AppResult<UserProfile> {
        log::debug!("开始获取用户 Profile 信息...");
        let response = self
            .client
            .get(format!("{BASE_URL}users/me"))
            .header("X-LC-Id", LC_ID)
            .header("X-LC-Key", LC_KEY)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .header("X-LC-Session", token)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "无法读取错误信息".to_string());
            log::error!("获取 Profile 失败: HTTP {status}, 响应: {error_text}");
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(AppError::AuthError("Token 无效或已过期".to_string()));
            }
            return Err(AppError::Other(format!("获取 Profile 失败: HTTP {status}")));
        }

        match response.json::<UserProfile>().await {
            Ok(profile) => {
                log::debug!("成功获取 Profile: {}", profile.nickname);
                Ok(profile)
            }
            Err(e) => {
                log::error!("解析 Profile JSON 失败: {e}");
                Err(AppError::Other(format!("解析 Profile 响应失败: {e}")))
            }
        }
    }
    // 新增函数：直接获取云端存档的元数据 (saveInfo)
    pub async fn get_cloud_save_info(&self, token: &str) -> AppResult<serde_json::Value> {
        log::debug!("开始获取云端存档元数据 (saveInfo)...");
        let summary = self.fetch_summary(token).await?;
        log::debug!("成功获取云端存档元数据。");
        Ok(summary)
    }

    // 获取存档的校验和，用于作为缓存键的一部分
    pub async fn get_save_checksum(&self, token: &str) -> AppResult<String> {
        let summary = self.fetch_summary(token).await?;
        let checksum = summary["results"][0]["gameFile"]["metaData"]["_checksum"]
            .as_str()
            .ok_or_else(|| AppError::Other("无法获取存档校验和".to_string()))?
            .to_string();
        Ok(checksum)
    }

    // 调用外部数据源API - 支持多种认证方式
    // 返回完整的外部API响应数据和存档文件数据
    pub async fn get_external_save_data(&self, request_data: serde_json::Value) -> AppResult<(serde_json::Value, Vec<u8>)> {
        log::debug!("开始调用外部API获取存档数据，请求数据: {}", request_data);

        let response = self
            .client
            .post(EXTERNAL_API_URL)
            .json(&request_data)
            .send()
            .await
            .map_err(|e| AppError::Other(format!("外部API请求失败: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("外部API返回错误状态: HTTP {status}, 响应: {error_text}");

            if status == reqwest::StatusCode::BAD_REQUEST {
                return Err(AppError::AuthError("外部API鉴权失败".to_string()));
            }
            return Err(AppError::Other(format!("外部API错误: HTTP {status}")));
        }

        let external_response: serde_json::Value = response.json().await
            .map_err(|e| AppError::Other(format!("解析外部API响应失败: {e}")))?;

        log::debug!("成功从外部API获取数据");

        // 从响应中提取存档URL并下载
        let save_url = external_response["data"]["saveUrl"]
            .as_str()
            .ok_or_else(|| AppError::Other("外部API响应中没有saveUrl".to_string()))?;

        let save_data = self.download_save(save_url).await?;
        Ok((external_response, save_data))
    }

    // 智能构建外部API请求数据
    pub fn build_external_request_data(request: &crate::models::user::IdentifierRequest) -> AppResult<serde_json::Value> {
        // 认证方式优先级：平台认证 > API认证 > Token认证
        if let (Some(platform), Some(platform_id)) = (&request.platform, &request.platform_id) {
            // 平台认证 - 最佳选择用于图片渲染
            log::debug!("使用平台认证: platform={}, platform_id={}", platform, platform_id);
            return Ok(json!({
                "platform": platform,
                "platform_id": platform_id
            }));
        }

        if let Some(api_user_id) = &request.api_user_id {
            // API认证 (api_token 是可选的)
            log::debug!("使用API认证: api_user_id={}", api_user_id);
            let mut request_data = serde_json::Map::new();
            request_data.insert("api_user_id".to_string(), json!(api_user_id));
            if let Some(api_token) = &request.api_token {
                request_data.insert("api_token".to_string(), json!(api_token));
            }
            return Ok(serde_json::Value::Object(request_data));
        }

        if let Some(token) = &request.token {
            // Token认证
            log::debug!("使用Token认证");
            return Ok(json!({ "token": token }));
        }

        Err(AppError::Other("外部数据源需要认证信息 (platform+platform_id, api_user_id+api_token, 或 token)".to_string()))
    }

    // 从请求中提取PlayerId
    fn extract_player_id_from_request(request: &crate::models::user::IdentifierRequest) -> AppResult<String> {
        // 认证方式优先级：平台认证 > API认证 > Token认证
        if let (Some(platform), Some(platform_id)) = (&request.platform, &request.platform_id) {
            // 平台认证 - 生成格式为 "平台:平台ID" 的PlayerId
            return Ok(format!("{}:{}", platform, platform_id));
        }

        if let Some(token) = &request.token {
            // Token认证 - 使用token的前8位作为PlayerId
            return Ok(format!("token:{}", &token[..std::cmp::min(8, token.len())]));
        }

        Err(AppError::Other("无法从请求中提取PlayerId".to_string()))
    }
    // 新增：获取完整的存档数据，包括云端元数据
    pub async fn get_full_save_data(&self, token: &str) -> AppResult<FullSaveData> {
        log::debug!("开始获取完整的存档数据...");

        // 1. 获取云端摘要 (只进行一次网络请求)
        let summary = self.fetch_summary(token).await?;
        log::debug!("成功获取云端摘要");

        // 2. 从摘要中下载并校验存档
        let save_data = self.fetch_save_from_summary(&summary).await?;
        log::debug!("成功获取并校验存档二进制数据");

        // 3. 解析存档并添加难度信息
        let save = parse_save_with_difficulty(&save_data)?;
        log::debug!("成功解析存档并添加难度信息");

        // 4. 从解析后的存档计算RKS (复用get_rks的逻辑)
        let rks_result = self.calculate_rks_from_save(&save)?;
        log::debug!("成功计算RKS结果");

        // 5. 封装并返回 FullSaveData
        Ok(FullSaveData {
            rks_result,
            save,
            cloud_summary: summary,
        })
    }

    // 增强版：根据数据源获取完整的存档数据
    pub async fn get_full_save_data_with_source(&self, request: &crate::models::user::IdentifierRequest) -> AppResult<FullSaveData> {
        log::debug!("开始获取完整的存档数据 (数据源: {:?})...", request.data_source);

        match request.data_source.as_deref() {
            Some("external") => {
                // 使用外部数据源
                let request_data = Self::build_external_request_data(request)?;
                let (external_response, save_data) = self.get_external_save_data(request_data).await?;
                log::debug!("成功从外部数据源获取存档二进制数据和完整响应");

                // 解析存档并添加难度信息
                let save = parse_save_with_difficulty(&save_data)?;
                log::debug!("成功解析外部存档并添加难度信息");

                // 从解析后的存档计算RKS
                let rks_result = self.calculate_rks_from_save(&save)?;
                log::debug!("成功计算外部存档的RKS结果");

                // 从外部API响应中提取玩家名称和PlayerId
                let player_name = external_response["data"]["saveInfo"]["nickname"]
                    .as_str()
                    .unwrap_or("external:unknown")
                    .to_string();
                let player_id = external_response["data"]["saveInfo"]["PlayerId"]
                    .as_str()
                    .or_else(|| external_response["data"]["apiId"].as_str())
                    .unwrap_or("external:unknown")
                    .to_string();

                log::debug!("从外部API响应中提取到玩家名称: {}, PlayerId: {}", player_name, player_id);

                // 构造云端摘要，包含从外部API获取的真实数据
                let updated_at = external_response["data"]["saveInfo"]["modifiedAt"]["iso"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                let cloud_summary = json!({
                    "results": [{
                        "gameFile": {
                            "url": external_response["data"]["saveUrl"],
                            "metaData": {
                                "_checksum": "external_data"
                            }
                        },
                        "updatedAt": updated_at,
                        "PlayerId": player_id,
                        "nickname": player_name
                    }]
                });

                Ok(FullSaveData {
                    rks_result,
                    save,
                    cloud_summary,
                })
            },
            _ => {
                // 使用内部数据源（默认）
                let token = request.token.as_ref()
                    .ok_or_else(|| AppError::Other("内部数据源需要token".to_string()))?;
                self.get_full_save_data(token).await
            }
        }
    }

    // 辅助函数：从已解析的GameSave中计算RKS
    fn calculate_rks_from_save(&self, save: &GameSave) -> AppResult<RksResult> {
        let game_record = save
            .game_record
            .as_ref()
            .ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;

        let mut rks_records = Vec::new();
        for (song_id, difficulties) in game_record {
            let song_name = crate::utils::data_loader::get_song_name_by_id(song_id)
                .unwrap_or_else(|| song_id.clone());
            for (diff_name, record) in difficulties {
                if let (Some(acc), Some(difficulty)) = (record.acc, record.difficulty) {
                    if acc >= 70.0 && difficulty > 0.0 {
                        let rks_record = crate::models::rks::RksRecord::new(
                            song_id.clone(),
                            song_name.clone(),
                            diff_name.clone(),
                            difficulty,
                            record,
                        );
                        rks_records.push(rks_record);
                    }
                }
            }
        }
        Ok(RksResult::new(rks_records))
    }
}
