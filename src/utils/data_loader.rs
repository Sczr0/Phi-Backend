use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::config::CONFIG;
use crate::models::{SongDifficulty, SongInfo, NicknameMap};
use crate::utils::error::AppResult;

lazy_static! {
    pub static ref SONG_INFO: Arc<Vec<SongInfo>> = Arc::new({
        match load_song_info() {
            Ok(info) => {
                log::info!("已加载 {} 条歌曲信息", info.len());
                // 输出一些示例歌曲信息以便调试
                if !info.is_empty() {
                    log::debug!("歌曲信息示例 (前3条):");
                    for (i, song) in info.iter().take(3).enumerate() {
                        log::debug!(" {}: ID={}, 歌曲名={}, 作曲家={}", i + 1, song.id, song.song, song.composer);
                    }
                }
                info
            }
            Err(e) => {
                // Log the error if loading fails
                log::error!("加载歌曲信息失败: {}", e);
                // Return an empty vector as before, but after logging the error
                Vec::new() 
            }
        }
    });
    pub static ref SONG_DIFFICULTY: Arc<Vec<SongDifficulty>> = Arc::new({
        match load_song_difficulty() {
            Ok(difficulty) => {
                log::info!("已加载 {} 条歌曲难度信息", difficulty.len());
                // 输出一些示例难度信息以便调试
                if !difficulty.is_empty() {
                    log::debug!("歌曲难度示例 (前3条):");
                    for (i, diff) in difficulty.iter().take(3).enumerate() {
                        log::debug!(" {}: ID={}, EZ={:?}, HD={:?}, IN={:?}, AT={:?}", 
                            i + 1, diff.id, diff.ez, diff.hd, diff.inl, diff.at);
                    }
                }
                difficulty
            }
            Err(e) => {
                log::error!("加载歌曲难度信息失败: {}", e);
                Vec::new()
            }
        }
    });
    pub static ref SONG_NICKNAMES: Arc<NicknameMap> = Arc::new({
        match load_song_nicknames() {
            Ok(nicknames) => {
                log::info!("已加载 {} 条歌曲别名信息", nicknames.len());
                nicknames
            }
            Err(e) => {
                log::error!("加载歌曲别名信息失败: {}", e);
                HashMap::new()
            }
        }
    });
    // 索引方便查询
    pub static ref SONG_ID_TO_NAME: Arc<HashMap<String, String>> = Arc::new({
        let mut map = HashMap::new();
        for info in SONG_INFO.iter() {
            map.insert(info.id.clone(), info.song.clone());
        }
        log::info!("已创建 ID->歌曲名 映射，共 {} 条", map.len());
        // 输出一些示例映射以便调试
        if !map.is_empty() {
            log::debug!("ID->歌曲名映射示例 (前3条):");
            for (i, (id, name)) in map.iter().take(3).enumerate() {
                log::debug!(" {}: {}=>{}", i + 1, id, name);
            }
        }
        map
    });
    pub static ref SONG_NAME_TO_ID: Arc<HashMap<String, String>> = Arc::new({
        let mut map = HashMap::new();
        for info in SONG_INFO.iter() {
            map.insert(info.song.clone(), info.id.clone());
        }
        log::info!("已创建 歌曲名->ID 映射，共 {} 条", map.len());
        map
    });
    pub static ref DIFFICULTY_MAP: Arc<HashMap<String, SongDifficulty>> = Arc::new({
        let mut map = HashMap::new();
        for diff in SONG_DIFFICULTY.iter() {
            map.insert(diff.id.clone(), diff.clone());
        }
        log::info!("已创建 ID->难度 映射，共 {} 条", map.len());
        map
    });
}

// 加载歌曲信息
fn load_song_info() -> AppResult<Vec<SongInfo>> {
    let path = Path::new(&CONFIG.info_data_path).join(&CONFIG.info_file);
    log::debug!("正在加载歌曲信息，路径: {}", path.display());
    let mut rdr = csv::Reader::from_path(path)?;
    let mut songs = Vec::new();

    for result in rdr.deserialize() {
        let record: SongInfo = result?;
        songs.push(record);
    }

    log::debug!("歌曲信息加载完成，共 {} 条", songs.len());
    Ok(songs)
}

// 加载歌曲难度
fn load_song_difficulty() -> AppResult<Vec<SongDifficulty>> {
    let path = Path::new(&CONFIG.info_data_path).join(&CONFIG.difficulty_file);
    log::debug!("正在加载歌曲难度，路径: {}", path.display());
    let mut rdr = csv::Reader::from_path(path)?;
    let mut difficulties = Vec::new();

    for (index, result) in rdr.deserialize().enumerate() {
        let line_num = index + 2; // +1 for header, +1 for 1-based index
        log::trace!("处理 difficulty.csv 第 {} 行...", line_num);
        match result {
            Ok(record) => {
                // 打印成功解析的记录内容
                log::trace!("成功解析第 {} 行: {:?}", line_num, record);
                difficulties.push(record);
            }
            Err(e) => {
                // 打印详细错误和行号
                log::error!("解析 difficulty.csv 第 {} 行失败: {}", line_num, e);
                // 选择性操作：如果希望遇到错误就停止，可以取消下面的注释
                // return Err(e.into());
            }
        }
    }

    log::debug!("歌曲难度加载完成，共 {} 条", difficulties.len());
    Ok(difficulties)
}

// 加载歌曲别名
fn load_song_nicknames() -> AppResult<NicknameMap> {
    let path = Path::new(&CONFIG.info_data_path).join(&CONFIG.nicklist_file);
    log::debug!("正在加载歌曲别名，路径: {}", path.display());
    let content = fs::read_to_string(path)?;
    let nicknames: NicknameMap = serde_yaml::from_str(&content)?;
    log::debug!("歌曲别名加载完成，共 {} 条", nicknames.len());
    Ok(nicknames)
}

// 根据歌曲ID查找歌曲名称
pub fn get_song_name_by_id(id: &str) -> Option<String> {
    let result = SONG_ID_TO_NAME.get(id).cloned();
    if result.is_none() {
        log::debug!("未找到歌曲 ID '{}'对应的名称", id);
    }
    result
}

// 根据歌曲名称查找歌曲ID
pub fn get_song_id_by_name(name: &str) -> Option<String> {
    SONG_NAME_TO_ID.get(name).cloned()
}

// 根据别名查找歌曲名称 (忽略大小写)
pub fn get_song_by_nickname(nickname: &str) -> Option<String> {
    let query_lower = nickname.to_lowercase(); // 预先计算查询的小写形式
    for (song, nicknames) in SONG_NICKNAMES.iter() {
        // 比较小写形式以忽略大小写
        if nicknames.iter().any(|n| n.to_lowercase() == query_lower) {
            return Some(song.clone());
        }
    }
    None
}

// 根据歌曲ID获取难度定数
pub fn get_difficulty_by_id(id: &str, difficulty_level: &str) -> Option<f64> {
    let result = DIFFICULTY_MAP.get(id).and_then(|d| match difficulty_level {
        "EZ" => d.ez,
        "HD" => d.hd,
        "IN" => d.inl,
        "AT" => d.at,
        "Legacy" => None,
        _ => {
            log::warn!("未知的难度级别: {} (歌曲ID: {})", difficulty_level, id);
            None
        },
    });
    
    if result.is_none() && difficulty_level != "Legacy" {
        log::debug!("未找到歌曲 '{}' 难度 '{}' 的定数映射", id, difficulty_level);
    }
    
    result
} 