// use std::collections::HashMap;

use crate::models::{SongInfo, SongDifficulty};
use crate::utils::data_loader::{SONG_INFO, DIFFICULTY_MAP, SONG_NICKNAMES};
use crate::utils::error::{AppError, AppResult};

// 歌曲服务，提供歌曲信息查询
#[derive(Clone)]
pub struct SongService {}

impl SongService {
    // 创建新的歌曲服务
    pub fn new() -> Self {
        Self {}
    }

    // 统一搜索函数：自动判断输入是ID、歌曲名还是别名
    pub fn search_song(&self, query: &str) -> AppResult<SongInfo> {
        if query.is_empty() {
            return Err(AppError::SongNotFound("输入为空".to_string()));
        }

        let query = query.trim();
        log::info!("统一搜索歌曲: '{}'", query);
        
        // 调试输出
        log::debug!("SONG_INFO 包含 {} 条记录", SONG_INFO.len());
        log::debug!("SONG_NICKNAMES 包含 {} 条记录", SONG_NICKNAMES.len());

        // 1. 首先尝试作为歌曲ID直接查找 (精确匹配)
        for info in SONG_INFO.iter() {
            if info.id == query {
                log::info!("通过ID精确匹配找到歌曲: {}", info.song);
                return Ok(info.clone());
            }
        }

        // 2. 尝试作为歌曲名称查找 (忽略大小写)
        let query_lower = query.to_lowercase();
        for info in SONG_INFO.iter() {
            if info.song.to_lowercase() == query_lower {
                log::info!("通过歌曲名精确匹配找到歌曲: {}", info.song);
                return Ok(info.clone());
            }
        }

        // 3. 尝试作为别名查找 (忽略大小写)
        let mut found_by_nickname = None;
        for (song_name, nicknames) in SONG_NICKNAMES.iter() {
            for nickname in nicknames {
                if nickname.to_lowercase() == query_lower {
                    log::info!("通过别名精确匹配找到歌曲: {} (别名: {})", song_name, nickname);
                    
                    // 根据歌曲名查找对应的SongInfo
                    for info in SONG_INFO.iter() {
                        // 完全匹配
                        if info.song == *song_name {
                            log::info!("在SONG_INFO中通过完全匹配找到: {}", info.song);
                            return Ok(info.clone());
                        }
                        
                        // 忽略大小写匹配
                        if info.song.to_lowercase() == song_name.to_lowercase() {
                            log::info!("在SONG_INFO中通过忽略大小写匹配找到: {}", info.song);
                            return Ok(info.clone());
                        }
                        
                        // 尝试匹配ID的第一部分（Phigros歌曲ID格式通常为"歌名.作者"）
                        if let Some(id_song_part) = info.id.split('.').next() {
                            if id_song_part == *song_name {
                                log::info!("在SONG_INFO中通过ID前缀匹配找到: {} (ID: {})", info.song, info.id);
                                return Ok(info.clone());
                            }
                        }
                        
                        // 尝试匹配ID
                        if info.id == *song_name || info.id.to_lowercase() == song_name.to_lowercase() {
                            log::info!("在SONG_INFO中通过ID匹配找到: {}", info.song);
                            return Ok(info.clone());
                        }
                    }
                    
                    // 如果找到别名但未找到匹配的SongInfo，记录下来
                    found_by_nickname = Some(song_name.clone());
                    break;
                }
            }
            if found_by_nickname.is_some() {
                break;
            }
        }
        
        // 4. 尝试歌曲名模糊匹配 (包含关系，忽略大小写)
        let mut name_matches = Vec::new();
        for info in SONG_INFO.iter() {
            if info.song.to_lowercase().contains(&query_lower) {
                name_matches.push(info.clone());
            }
        }

        if name_matches.len() == 1 {
            log::info!("通过歌曲名模糊匹配找到歌曲: {}", name_matches[0].song);
            return Ok(name_matches[0].clone());
        }

        // 5. 尝试别名模糊匹配 (包含关系，忽略大小写)
        let mut nickname_matches = Vec::new();
        for (song_name, nicknames) in SONG_NICKNAMES.iter() {
            for nickname in nicknames {
                if nickname.to_lowercase().contains(&query_lower) {
                    // 查找对应的SongInfo（与精确匹配部分相同的逻辑）
                    let mut found = false;
                    for info in SONG_INFO.iter() {
                        // 完全匹配
                        if info.song == *song_name {
                            nickname_matches.push((info.clone(), nickname.clone()));
                            found = true;
                            break;
                        }
                        
                        // 忽略大小写匹配
                        if info.song.to_lowercase() == song_name.to_lowercase() {
                            nickname_matches.push((info.clone(), nickname.clone()));
                            found = true;
                            break;
                        }
                        
                        // 尝试匹配ID的第一部分
                        if let Some(id_song_part) = info.id.split('.').next() {
                            if id_song_part == *song_name {
                                nickname_matches.push((info.clone(), nickname.clone()));
                                found = true;
                                break;
                            }
                        }
                        
                        // 尝试匹配ID
                        if info.id == *song_name || info.id.to_lowercase() == song_name.to_lowercase() {
                            nickname_matches.push((info.clone(), nickname.clone()));
                            found = true;
                            break;
                        }
                    }
                    
                    if !found {
                        log::warn!("在别名列表中找到 '{}' 别名 '{}'，但无法在SONG_INFO中找到对应的歌曲", song_name, nickname);
                    }
                    break;
                }
            }
        }

        if nickname_matches.len() == 1 {
            let (info, nickname) = &nickname_matches[0];
            log::info!("通过别名模糊匹配找到歌曲: {} (别名: {})", info.song, nickname);
            return Ok(info.clone());
        }

        // 处理多个匹配的情况
        if !name_matches.is_empty() {
            let matches_str = name_matches
                .iter()
                .map(|info| info.song.clone())
                .collect::<Vec<_>>()
                .join(", ");
            log::info!("歌曲名模糊匹配找到多个歌曲: {}", matches_str);
            return Err(AppError::AmbiguousSongName(matches_str));
        }

        if !nickname_matches.is_empty() {
            let matches_str = nickname_matches
                .iter()
                .map(|(info, nickname)| format!("{} (别名: {})", info.song, nickname))
                .collect::<Vec<_>>()
                .join(", ");
            log::info!("别名模糊匹配找到多个歌曲: {}", matches_str);
            return Err(AppError::AmbiguousSongName(matches_str));
        }

        // 处理找到别名但未找到匹配SongInfo的情况
        if let Some(song_name) = found_by_nickname {
            log::warn!("在别名列表中找到歌曲 '{}'，但无法在歌曲信息中找到对应记录", song_name);
            
            // 尝试通过歌曲ID的部分匹配找到相关记录
            // 这通常用于处理 "歌名.作者" 格式的ID
            for info in SONG_INFO.iter() {
                if info.id.starts_with(&format!("{}.", song_name)) || info.id == song_name {
                    log::info!("通过ID开头匹配找到歌曲: {} (ID: {})", info.song, info.id);
                    return Ok(info.clone());
                }
            }
            
            // 还可以尝试：如果song_name格式为"歌名.作者"，提取歌名部分再次尝试匹配
            if let Some(song_part) = song_name.split('.').next() {
                if song_part != song_name {  // 确保有分割
                    for info in SONG_INFO.iter() {
                        if info.song == song_part {
                            log::info!("通过提取歌名部分找到: {} (原始名: {})", info.song, song_name);
                            return Ok(info.clone());
                        }
                    }
                }
            }
            
            // 最后才创建临时的SongInfo
            log::warn!("无法找到匹配的记录，创建临时信息");
            let temp_info = SongInfo {
                id: song_name.clone(),
                song: song_name.clone(),
                composer: "未知作曲家".to_string(),
                illustrator: None,
                ez_charter: None,
                hd_charter: None,
                in_charter: None,
                at_charter: None,
            };
            log::warn!("创建临时歌曲信息: {:?}", temp_info);
            return Ok(temp_info);
        }

        // 所有尝试都失败，返回错误
        log::info!("找不到匹配查询 '{}' 的歌曲", query);
        Err(AppError::SongNotFound(query.to_string()))
    }
    
    // 根据统一查询找ID
    pub fn get_song_id(&self, query: &str) -> AppResult<String> {
        if query.is_empty() {
            return Err(AppError::SongNotFound("输入为空".to_string()));
        }
        
        let song_info = self.search_song(query)?;
        Ok(song_info.id.clone())
    }

    // 获取歌曲难度信息
    pub fn get_song_difficulty(&self, id: &str) -> AppResult<SongDifficulty> {
        DIFFICULTY_MAP
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SongNotFound(id.to_string()))
    }

    // 获取所有歌曲信息
    pub fn get_all_songs(&self) -> Vec<SongInfo> {
        SONG_INFO.as_ref().clone()
    }
    
    // ===================== 以下为兼容性函数，使用新的统一搜索实现 =====================
    
    // 根据歌曲名称获取歌曲ID (支持模糊匹配、别名和忽略大小写)
    pub fn get_song_id_by_name(&self, name_or_alias: &str) -> AppResult<String> {
        self.get_song_id(name_or_alias)
    }

    // 根据歌曲别名获取歌曲ID (保留向后兼容)
    pub fn get_song_id_by_nickname(&self, nickname: &str) -> AppResult<String> {
        self.get_song_id(nickname)
    }

    // 获取歌曲信息
    pub fn get_song_info(&self, id: &str) -> AppResult<SongInfo> {
        self.search_song(id)
    }
} 