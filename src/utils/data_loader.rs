use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::env; // 导入 env 模块
use serde::Deserialize; // 导入 Deserialize 宏

// 不再需要导入 CONFIG
// use crate::config::CONFIG; 
use crate::models::{SongDifficulty, SongInfo, NicknameMap, PredictedConstants /*, PredictionResponse*/};
use crate::utils::error::AppResult;

// --- 辅助函数：从环境变量获取路径，如果未设置则使用默认值 ---
fn get_data_path(env_var: &str, default_value: &str) -> PathBuf {
    env::var(env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default_value))
}

fn get_data_file_path(base_path_var: &str, base_path_default: &str, file_env_var: &str, file_default: &str) -> PathBuf {
    let base_path = get_data_path(base_path_var, base_path_default);
    let file_name = env::var(file_env_var).unwrap_or_else(|_| file_default.to_string());
    base_path.join(file_name)
}
// --- 结束辅助函数 ---

lazy_static! {
    // 获取基础数据路径，默认为 "info"
    static ref INFO_DATA_PATH_BUF: PathBuf = get_data_path("INFO_DATA_PATH", "info");
    
    // 构建完整文件路径
    static ref INFO_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("INFO_FILE").unwrap_or_else(|_| "info.csv".to_string())
    );
    static ref DIFFICULTY_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("DIFFICULTY_FILE").unwrap_or_else(|_| "difficulty.csv".to_string())
    );
    static ref NICKLIST_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("NICKLIST_FILE").unwrap_or_else(|_| "nicklist.yaml".to_string())
    );
    // 预测常数文件路径
    static ref PREDICTIONS_FILE_PATH: PathBuf = INFO_DATA_PATH_BUF.join(
        env::var("PREDICTIONS_FILE").unwrap_or_else(|_| "chart_predictions_wide.csv".to_string())
    );

    pub static ref SONG_INFO: Arc<Vec<SongInfo>> = Arc::new({
        match load_song_info(&INFO_FILE_PATH) { // 传递路径
            Ok(info) => {
                log::info!("已加载 {} 条歌曲信息", info.len());
                // ... (调试日志保持不变) ...
                info
            }
            Err(e) => {
                log::error!("加载歌曲信息失败: {}", e);
                Vec::new()
            }
        }
    });
    pub static ref SONG_DIFFICULTY: Arc<Vec<SongDifficulty>> = Arc::new({
        match load_song_difficulty(&DIFFICULTY_FILE_PATH) { // 传递路径
            Ok(difficulty) => {
                log::info!("已加载 {} 条歌曲难度信息", difficulty.len());
                // ... (调试日志保持不变) ...
                difficulty
            }
            Err(e) => {
                log::error!("加载歌曲难度信息失败: {}", e);
                Vec::new()
            }
        }
    });
    pub static ref SONG_NICKNAMES: Arc<NicknameMap> = Arc::new({
        match load_song_nicknames(&NICKLIST_FILE_PATH) { // 传递路径
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
    // ... (其他 lazy_static 保持不变) ...
    pub static ref SONG_ID_TO_NAME: Arc<HashMap<String, String>> = Arc::new({
        let mut map = HashMap::new();
        for info in SONG_INFO.iter() {
            map.insert(info.id.clone(), info.song.clone());
        }
        log::info!("已创建 ID->歌曲名 映射，共 {} 条", map.len());
        // ... (调试日志保持不变) ...
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
    // 预测常数数据
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

// 临时的结构体，用于从CSV反序列化预测常数数据
#[derive(Deserialize)]
struct PredictedConstantRecord {
    song_id: String,
    ez: Option<f32>,
    hd: Option<f32>,
    inl: Option<f32>,
    at: Option<f32>,
}

// 加载歌曲信息 - 修改为接受 Path 参数
fn load_song_info(path: &Path) -> AppResult<Vec<SongInfo>> {
    // 不再需要从 CONFIG 获取路径
    // let path = Path::new(&CONFIG.info_data_path).join(&CONFIG.info_file);
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

// 加载歌曲难度 - 修改为接受 Path 参数
fn load_song_difficulty(path: &Path) -> AppResult<Vec<SongDifficulty>> {
    // 不再需要从 CONFIG 获取路径
    // let path = Path::new(&CONFIG.info_data_path).join(&CONFIG.difficulty_file);
    log::debug!("正在加载歌曲难度，路径: {}", path.display());
    let mut rdr = csv::Reader::from_path(path)?;
    let mut difficulties = Vec::new();

    // ... (错误处理逻辑保持不变) ...
    for (index, result) in rdr.deserialize().enumerate() {
        let line_num = index + 2; // +1 for header, +1 for 1-based index
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

// 加载歌曲别名 - 修改为接受 Path 参数
fn load_song_nicknames(path: &Path) -> AppResult<NicknameMap> {
    // 不再需要从 CONFIG 获取路径
    // let path = Path::new(&CONFIG.info_data_path).join(&CONFIG.nicklist_file);
    log::debug!("正在加载歌曲别名，路径: {}", path.display());
    let content = fs::read_to_string(path)?;
    let nicknames: NicknameMap = serde_yaml::from_str(&content)?;
    log::debug!("歌曲别名加载完成，共 {} 条", nicknames.len());
    Ok(nicknames)
}

// 加载预测常数数据
fn load_predicted_constants(path: &Path) -> AppResult<HashMap<String, PredictedConstants>> {
    log::debug!("正在加载预测常数数据，路径: {}", path.display());
    
    // 检查文件是否存在
    if !path.exists() {
        log::warn!("预测常数文件不存在: {}", path.display());
        return Ok(HashMap::new());
    }
    
    let mut rdr = csv::Reader::from_path(path)?;
    let mut predictions = HashMap::new();
    
    for (index, result) in rdr.deserialize().enumerate() {
        let line_num = index + 2; // +1 for header, +1 for 1-based index
        match result {
            Ok(record) => {
                // 反序列化到临时结构体
                let prediction_record: PredictedConstantRecord = record;
                // 创建 PredictedConstants 实例作为 Value
                let constants = PredictedConstants {
                    ez: prediction_record.ez,
                    hd: prediction_record.hd,
                    inl: prediction_record.inl,
                    at: prediction_record.at,
                };
                // 使用临时结构体中的 song_id 作为 Key
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

// 根据歌曲ID和难度级别获取预测常数
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