use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::env;
use serde::Deserialize;

use crate::models::song::{SongDifficulty, SongInfo, NicknameMap};
use crate::models::predictions::PredictedConstants;
use crate::utils::error::AppResult;

// --- 辅助函数：从环境变量获取路径，如果未设置则使用默认值 ---
fn get_data_path(env_var: &str, default_value: &str) -> PathBuf {
    env::var(env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default_value))
}

lazy_static! {
    static ref INFO_DATA_PATH_BUF: PathBuf = get_data_path("INFO_DATA_PATH", "info");
    
    static ref INFO_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("INFO_FILE").unwrap_or_else(|_| "info.csv".to_string())
    );
    static ref DIFFICULTY_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("DIFFICULTY_FILE").unwrap_or_else(|_| "difficulty.csv".to_string())
    );
    static ref NICKLIST_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("NICKLIST_FILE").unwrap_or_else(|_| "nicklist.yaml".to_string())
    );
    static ref PREDICTIONS_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("PREDICTIONS_FILE").unwrap_or_else(|_| "chart_predictions_wide.csv".to_string())
    );

    pub static ref SONG_INFO: Arc<Vec<SongInfo>> = Arc::new({
        match load_song_info(&INFO_FILE_PATH) {
            Ok(info) => {
                log::info!("已加载 {} 条歌曲信息", info.len());
                info
            }
            Err(e) => {
                log::error!("加载歌曲信息失败: {}", e);
                Vec::new()
            }
        }
    });
    pub static ref SONG_DIFFICULTY: Arc<Vec<SongDifficulty>> = Arc::new({
        match load_song_difficulty(&DIFFICULTY_FILE_PATH) {
            Ok(difficulty) => {
                log::info!("已加载 {} 条歌曲难度信息", difficulty.len());
                difficulty
            }
            Err(e) => {
                log::error!("加载歌曲难度信息失败: {}", e);
                Vec::new()
            }
        }
    });
    pub static ref SONG_NICKNAMES: Arc<NicknameMap> = Arc::new({
        match load_song_nicknames(&NICKLIST_FILE_PATH) {
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
    pub static ref SONG_ID_TO_NAME: Arc<HashMap<String, String>> = Arc::new({
        let mut map = HashMap::new();
        for info in SONG_INFO.iter() {
            map.insert(info.id.clone(), info.song.clone());
        }
        log::info!("已创建 ID->歌曲名 映射，共 {} 条", map.len());
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
    pub static ref PREDICTED_CONSTANTS: Arc<HashMap<String, PredictedConstants>> = Arc::new({
        match load_predicted_constants(&PREDICTIONS_FILE_PATH) {
            Ok(predictions) => {
                log::info!("已加载 {} 条预测常数数据", predictions.len());
                predictions
            }
            Err(e) => {
                log::error!("加载预测常数数据失败: {}", e);
                HashMap::new()
            }
        }
    });
}

#[derive(Deserialize)]
struct PredictedConstantRecord {
    song_id: String,
    ez: Option<f32>,
    hd: Option<f32>,
    inl: Option<f32>,
    at: Option<f32>,
}

fn load_song_info(path: &Path) -> AppResult<Vec<SongInfo>> {
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

fn load_song_difficulty(path: &Path) -> AppResult<Vec<SongDifficulty>> {
    log::debug!("正在加载歌曲难度，路径: {}", path.display());
    let mut rdr = csv::Reader::from_path(path)?;
    let mut difficulties = Vec::new();

    for (index, result) in rdr.deserialize().enumerate() {
        let line_num = index + 2;
        log::trace!("处理 difficulty.csv 第 {} 行...", line_num);
        match result {
            Ok(record) => {
                log::trace!("成功解析第 {} 行: {:?}", line_num, record);
                difficulties.push(record);
            }
            Err(e) => {
                log::error!("解析 difficulty.csv 第 {} 行失败: {}", line_num, e);
            }
        }
    }

    log::debug!("歌曲难度加载完成，共 {} 条", difficulties.len());
    Ok(difficulties)
}

fn load_song_nicknames(path: &Path) -> AppResult<NicknameMap> {
    log::debug!("正在加载歌曲别名，路径: {}", path.display());
    let content = fs::read_to_string(path)?;
    let nicknames: NicknameMap = serde_yaml::from_str(&content)?;
    log::debug!("歌曲别名加载完成，共 {} 条", nicknames.len());
    Ok(nicknames)
}

fn load_predicted_constants(path: &Path) -> AppResult<HashMap<String, PredictedConstants>> {
    log::debug!("正在加载预测常数数据，路径: {}", path.display());
    
    if !path.exists() {
        log::warn!("预测常数文件不存在: {}", path.display());
        return Ok(HashMap::new());
    }
    
    let mut rdr = csv::Reader::from_path(path)?;
    let mut predictions = HashMap::new();
    
    for (index, result) in rdr.deserialize().enumerate() {
        let line_num = index + 2;
        match result {
            Ok(record) => {
                let prediction_record: PredictedConstantRecord = record;
                let constants = PredictedConstants {
                    ez: prediction_record.ez,
                    hd: prediction_record.hd,
                    inl: prediction_record.inl,
                    at: prediction_record.at,
                };
                predictions.insert(prediction_record.song_id, constants);
            }
            Err(e) => {
                log::error!("解析预测常数数据第 {} 行失败: {}", line_num, e);
            }
        }
    }
    
    log::debug!("预测常数数据加载完成，共 {} 条", predictions.len());
    Ok(predictions)
}

pub fn get_song_name_by_id(id: &str) -> Option<String> {
    let result = SONG_ID_TO_NAME.get(id).cloned();
    if result.is_none() {
        log::debug!("未找到歌曲 ID '{}'对应的名称", id);
    }
    result
}

#[allow(dead_code)]
pub fn get_song_id_by_name(name: &str) -> Option<String> {
    SONG_NAME_TO_ID.get(name).cloned()
}

#[allow(dead_code)]
pub fn get_song_by_nickname(nickname: &str) -> Option<String> {
    let query_lower = nickname.to_lowercase();
    for (song, nicknames) in SONG_NICKNAMES.iter() {
        if nicknames.iter().any(|n| n.to_lowercase() == query_lower) {
            return Some(song.clone());
        }
    }
    None
}

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

pub fn get_predicted_constant(id: &str, difficulty_level: &str) -> Option<f32> {
    PREDICTED_CONSTANTS.get(id).and_then(|p| match difficulty_level {
        "EZ" => p.ez,
        "HD" => p.hd,
        "IN" => p.inl,
        "AT" => p.at,
        _ => {
            log::warn!("获取预测常数时遇到未知的难度级别: {} (歌曲ID: {})", difficulty_level, id);
            None
        }
    })
}