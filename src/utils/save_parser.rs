use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use base64::{engine::general_purpose, Engine as _};
use byteorder::{LittleEndian, ReadBytesExt};
use serde_json::Value;
use zip::ZipArchive;

use crate::models::save::{GameSave, SaveSummary, SongRecord};
use crate::models::rks::{RksRecord, RksResult};
use crate::models::b30::{B30Record, B30Result};
use crate::utils::crypto::{decrypt, validate_session_token};
use crate::utils::data_loader::{get_difficulty_by_id, get_song_name_by_id};
use crate::utils::error::{AppError, AppResult};

// BinaryReader and other functions remain the same...
// (The rest of the file content is omitted for brevity as it doesn't need changes)
// ...
// The full content of the file will be written, but I'm only showing the changed part.

// BinaryReader 用于辅助解析二进制数据
struct BinaryReader<'a> {
    cursor: Cursor<&'a [u8]>,
    current_byte: u8,
    bit_pos: u8,
}

impl<'a> BinaryReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
            current_byte: 0,
            bit_pos: 8,
        }
    }

    fn position(&self) -> u64 {
        self.cursor.position()
    }

    fn remaining(&self) -> u64 {
        self.cursor.get_ref().len() as u64 - self.cursor.position()
            + if self.bit_pos < 8 { 1 } else { 0 }
    }
    
    fn reset_bit_reading(&mut self) {
        self.bit_pos = 8;
    }

    fn read_byte_aligned(&mut self) -> AppResult<u8> {
        self.reset_bit_reading();
        self.cursor.read_u8().map_err(AppError::IoError)
    }

    fn read_bit(&mut self) -> AppResult<bool> {
        if self.bit_pos >= 8 {
            self.current_byte = self.cursor.read_u8().map_err(AppError::IoError)?;
            self.bit_pos = 0;
            log::trace!("读取新字节用于位读取: {:#04x}", self.current_byte);
        }
        
        let bit_value = (self.current_byte >> self.bit_pos) & 1;
        self.bit_pos += 1;
        Ok(bit_value != 0)
    }

    fn read_bool(&mut self) -> AppResult<bool> {
        self.read_bit()
    }
    
    fn read_bits(&mut self, count: usize) -> AppResult<Vec<bool>> {
        let mut bits = Vec::with_capacity(count);
        for _ in 0..count {
            bits.push(self.read_bit()?);
        }
        Ok(bits)
    }

    fn read_short_int_aligned(&mut self) -> AppResult<u16> {
        self.reset_bit_reading();
        self.cursor.read_u16::<LittleEndian>().map_err(AppError::IoError)
    }

    fn read_int_aligned(&mut self) -> AppResult<u32> {
        self.reset_bit_reading();
        self.cursor.read_u32::<LittleEndian>().map_err(AppError::IoError)
    }

    fn read_float_aligned(&mut self) -> AppResult<f32> {
        self.reset_bit_reading();
        self.cursor.read_f32::<LittleEndian>().map_err(AppError::IoError)
    }

    fn read_var_int_aligned(&mut self) -> AppResult<usize> {
        self.reset_bit_reading();
        let mut result = 0;
        let mut offset = 0;
        let mut value;
        
        loop {
            value = self.cursor.read_u8().map_err(AppError::IoError)?;
            result |= ((value & 0x7F) as usize) << (7 * offset);
            offset += 1;
            
            if value & 0x80 == 0 || offset > 5 {
                break;
            }
        }
        Ok(result)
    }

    fn read_string_aligned(&mut self) -> AppResult<String> {
        self.reset_bit_reading();
        let length = self.read_var_int_aligned()?;
        if self.remaining() < length as u64 {
             return Err(AppError::Other(format!("解析String时数据不足，需要{}字节，剩余{}", length, self.remaining())));
        }
        let mut buffer = vec![0u8; length];
        self.cursor.read_exact(&mut buffer)?; 
        String::from_utf8(buffer)
            .map_err(|e| AppError::Other(format!("字符串UTF-8解码失败: {}", e)))
    }
    
    fn read_money_aligned(&mut self) -> AppResult<Vec<usize>> {
        self.reset_bit_reading();
        let mut money = Vec::with_capacity(5);
        for _ in 0..5 {
            money.push(self.read_var_int_aligned()?);
        }
        Ok(money)
    }
    
    fn read_game_key_aligned(&mut self) -> AppResult<HashMap<String, Value>> {
        self.reset_bit_reading();
        
        let mut all_keys = HashMap::new();
        let key_sum = self.read_var_int_aligned()?;
        
        for _ in 0..key_sum {
            let name = self.read_string_aligned()?;
            let length = self.read_byte_aligned()?;
            
            let mut one_key = serde_json::Map::new();
            
            let type_flags = self.read_bits(5)?;
            one_key.insert("type".to_string(), Value::Array(type_flags.into_iter().map(Value::Bool).collect()));
            
            let mut flag = Vec::new();
            
            for _ in 0..(length as usize - 1) {
                flag.push(self.read_byte_aligned()? as usize);
            }
            
            one_key.insert("flag".to_string(), Value::Array(flag.into_iter().map(|v| Value::Number(v.into())).collect()));
            
            all_keys.insert(name, Value::Object(one_key));
        }
        
        Ok(all_keys)
    }
    
    fn read_game_record_aligned(&mut self) -> AppResult<HashMap<String, HashMap<String, SongRecord>>> {
        log::debug!("进入 read_game_record_aligned");
        self.reset_bit_reading();
        
        let diff_list = ["EZ", "HD", "IN", "AT", "Legacy"];
        let mut all_records = HashMap::new();
        
        let song_count = self.read_var_int_aligned()?;
        log::debug!("GameRecord: 读取到 song_count = {}", song_count);
        
        for i in 0..song_count {
            log::trace!("GameRecord: 开始处理第 {} 首歌曲", i + 1);
            let song_id_raw = self.read_string_aligned()?;
            log::trace!("GameRecord: 读取到 song_id_raw = '{}'", song_id_raw);
            let song_id = if song_id_raw.ends_with(".0") {
                song_id_raw[..song_id_raw.len()-2].to_string()
            } else if song_id_raw.contains("Lv") {
                song_id_raw[..song_id_raw.rfind("Lv").unwrap_or(song_id_raw.len())].to_string()
            } else {
                song_id_raw.clone()
            };
            log::trace!("GameRecord: 解析得到 song_id = '{}'", song_id);
            
            let record_length = self.read_var_int_aligned()?;
            log::trace!("GameRecord: 歌曲 '{}' 的 record_length = {}", song_id, record_length);
            let record_end_pos = self.position() + record_length as u64;
            
            let unlock = self.read_byte_aligned()?;
            let fc_flags = self.read_byte_aligned()?;
            log::trace!("GameRecord: 歌曲 '{}' 的 unlock = {:#04x}, fc_flags = {:#04x}", song_id, unlock, fc_flags);
            
            let mut difficulties = HashMap::new();
            
            for level_index in 0..diff_list.len() {
                let diff_name = diff_list[level_index];
                if ((unlock >> level_index) & 1) != 0 {
                    log::trace!("GameRecord: 歌曲 '{}', 难度 '{}' (index {}) 存在记录", song_id, diff_name, level_index);
                    let score = self.read_int_aligned()?;
                    let acc = self.read_float_aligned()?;
                    log::trace!("GameRecord: 歌曲 '{}', 难度 '{}', 读取到 score = {}, acc = {}", song_id, diff_name, score, acc);
                    
                    let is_fc_or_ap = ((fc_flags >> level_index) & 1) != 0;
                    let is_ap = score == 1000000;
                    let is_fc = is_fc_or_ap && !is_ap;
                    log::trace!("GameRecord: 歌曲 '{}', 难度 '{}', is_fc_or_ap = {}, is_ap = {}, is_fc = {}", song_id, diff_name, is_fc_or_ap, is_ap, is_fc);
                    
                    let record = SongRecord {
                        score: Some(score as f64),
                        acc: Some(acc as f64),
                        fc: Some(is_fc),
                        difficulty: None,
                        rks: None,
                    };
                    
                    difficulties.insert(diff_name.to_string(), record);
                } else {
                     log::trace!("GameRecord: 歌曲 '{}', 难度 '{}' (index {}) 不存在记录", song_id, diff_name, level_index);
                }
            }
            
            if self.position() != record_end_pos {
                log::warn!("GameRecord: 解析歌曲 {} 后指针位置 ({}) 与预期 ({}) 不符，强制修正",
                    song_id, self.position(), record_end_pos);
                self.cursor.set_position(record_end_pos);
            }
            
            if !difficulties.is_empty() {
                log::debug!("GameRecord: 歌曲 '{}' 添加了 {} 个难度记录", song_id, difficulties.len());
                all_records.insert(song_id, difficulties);
            } else {
                log::debug!("GameRecord: 歌曲 '{}' 没有解析到任何难度记录，跳过", song_id);
            }
        }
        
        log::info!("read_game_record_aligned: 成功解析出 {} 首歌曲的成绩记录", all_records.len());
        Ok(all_records)
    }
}

pub fn check_session_token(token: &str) -> AppResult<()> {
    if !validate_session_token(token) {
        return Err(AppError::InvalidSessionToken);
    }
    Ok(())
}

pub fn unzip_save(save_data: &[u8]) -> AppResult<HashMap<String, Vec<u8>>> {
    let mut save_dict = HashMap::new();
    let cursor = Cursor::new(save_data);
    let mut zip = ZipArchive::new(cursor)?;

    if save_data.len() <= 30 {
        return Err(AppError::InvalidSaveSize(save_data.len()));
    }

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let filename = file.name().to_string();
        
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        save_dict.insert(filename, contents);
    }

    Ok(save_dict)
}

pub fn decrypt_save(save_dict: HashMap<String, Vec<u8>>) -> AppResult<GameSave> {
    log::debug!("开始解密存档...");
    let mut result = GameSave {
        game_key: None,
        game_progress: None,
        game_record: None,
        settings: None,
        user: None,
    };

    let mut file_heads = HashMap::new();
    for (key, value) in &save_dict {
        if !value.is_empty() {
            file_heads.insert(key.clone(), value[0]);
        }
    }

    for (filename, data) in save_dict {
        if data.is_empty() {
            log::warn!("文件 {} 为空", filename);
            continue;
        }

        log::debug!("处理文件: {}, 原始大小: {} 字节", filename, data.len());

        let file_head = data[0];
        let encrypted_data = &data[1..];
        log::debug!("文件 {} 的头部: {}, 加密数据大小: {} 字节", filename, file_head, encrypted_data.len());
        
        let decrypted_data = match decrypt(encrypted_data) {
            Ok(data) => data,
            Err(e) => {
                log::error!("解密文件 {} 失败: {}", filename, e);
                return Err(AppError::AesError(format!("解密文件 {} 失败: {}", filename, e)));
            }
        };
        log::debug!("文件 {} 解密后大小: {} 字节", filename, decrypted_data.len());
        
        let mut reader = BinaryReader::new(&decrypted_data);
        
        match filename.as_str() {
            "gameKey" => {
                let mut map = HashMap::new();
                if file_head == 3 {
                    if let Ok(parsed_data) = parse_game_key03(&mut reader) {
                        map = parsed_data;
                    } else {
                        log::warn!("解析 gameKey03 失败");
                    }
                } else if file_head == 2 {
                    if let Ok(parsed_data) = parse_game_key02(&mut reader) {
                        map = parsed_data;
                    } else {
                        log::warn!("解析 gameKey02 失败");
                    }
                } else {
                    log::warn!("未知的 gameKey 文件头: {}", file_head);
                }
                result.game_key = Some(map);
            },
            "gameProgress" => {
                let mut map = HashMap::new();
                if file_head == 4 {
                    if let Ok(parsed_data) = parse_game_progress04(&mut reader) {
                        map = parsed_data;
                    } else {
                        log::warn!("解析 gameProgress04 失败");
                    }
                } else if file_head == 3 {
                     if let Ok(parsed_data) = parse_game_progress03(&mut reader) {
                        map = parsed_data;
                    } else {
                        log::warn!("解析 gameProgress03 失败");
                    }
                } else {
                    log::warn!("未知的 gameProgress 文件头: {}", file_head);
                }
                result.game_progress = Some(map);
            },
            "gameRecord" => {
                log::info!("准备解析 GameRecord...");
                if file_head == 1 {
                    if let Ok(game_record) = reader.read_game_record_aligned() {
                        result.game_record = Some(game_record);
                    } else {
                        log::warn!("解析 gameRecord 失败");
                        result.game_record = Some(HashMap::new());
                    }
                } else {
                    log::warn!("未知的 gameRecord 文件头: {}", file_head);
                    result.game_record = Some(HashMap::new());
                }
            },
            "settings" => {
                let mut map = HashMap::new();
                if file_head == 1 {
                    if let Ok(parsed_data) = parse_settings01(&mut reader) {
                        map = parsed_data;
                    } else {
                        log::warn!("解析 settings01 失败");
                    }
                } else {
                    log::warn!("未知的 settings 文件头: {}", file_head);
                }
                result.settings = Some(map);
            },
            "user" => {
                let mut map = HashMap::new();
                if file_head == 1 {
                     if let Ok(parsed_data) = parse_user01(&mut reader) {
                        map = parsed_data;
                    } else {
                        log::warn!("解析 user01 失败");
                    }
                } else {
                    log::warn!("未知的 user 文件头: {}", file_head);
                }
                result.user = Some(map);
            },
            _ => {
                log::warn!("未知的文件类型: {}", filename);
            }
        }
        
        if reader.remaining() > 0 {
            log::warn!("文件 {} 解析后仍有 {} 字节未读取", filename, reader.remaining());
        }
    }
    log::debug!("存档解密和初步解析完成");
    Ok(result)
}

fn parse_user01(reader: &mut BinaryReader) -> AppResult<HashMap<String, Value>> {
    let mut map = HashMap::new();
    map.insert("showPlayerId".to_string(), Value::Number(reader.read_byte_aligned()?.into()));
    map.insert("selfIntro".to_string(), Value::String(reader.read_string_aligned()?));
    map.insert("avatar".to_string(), Value::String(reader.read_string_aligned()?));
    map.insert("background".to_string(), Value::String(reader.read_string_aligned()?));
    Ok(map)
}

fn parse_settings01(reader: &mut BinaryReader) -> AppResult<HashMap<String, Value>> {
    let mut map = HashMap::new();
    map.insert("chordSupport".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("fcAPIndicator".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("enableHitSound".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("lowResolutionMode".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("deviceName".to_string(), Value::String(reader.read_string_aligned()?));
    map.insert("bright".to_string(), Value::Number(serde_json::Number::from_f64(reader.read_float_aligned()?.into()).unwrap()));
    map.insert("musicVolume".to_string(), Value::Number(serde_json::Number::from_f64(reader.read_float_aligned()?.into()).unwrap()));
    map.insert("effectVolume".to_string(), Value::Number(serde_json::Number::from_f64(reader.read_float_aligned()?.into()).unwrap()));
    map.insert("hitSoundVolume".to_string(), Value::Number(serde_json::Number::from_f64(reader.read_float_aligned()?.into()).unwrap()));
    map.insert("soundOffset".to_string(), Value::Number(serde_json::Number::from_f64(reader.read_float_aligned()?.into()).unwrap()));
    map.insert("noteScale".to_string(), Value::Number(serde_json::Number::from_f64(reader.read_float_aligned()?.into()).unwrap()));
    Ok(map)
}

fn parse_game_key02(reader: &mut BinaryReader) -> AppResult<HashMap<String, Value>> {
    let mut map = HashMap::new();
    map.insert("keyList".to_string(), Value::Object(reader.read_game_key_aligned()?.into_iter().map(|(k,v)| (k,v)).collect()));
    map.insert("lanotaReadKeys".to_string(), Value::Array(reader.read_bits(6)?.into_iter().map(Value::Bool).collect()));
    map.insert("camelliaReadKey".to_string(), Value::Array(reader.read_bits(8)?.into_iter().map(Value::Bool).collect()));
    Ok(map)
}

fn parse_game_key03(reader: &mut BinaryReader) -> AppResult<HashMap<String, Value>> {
    let mut map = parse_game_key02(reader)?;
    map.insert("sideStory4BeginReadKey".to_string(), Value::Number(reader.read_byte_aligned()?.into()));
    map.insert("oldScoreClearedV390".to_string(), Value::Number(reader.read_byte_aligned()?.into()));
    Ok(map)
}

fn parse_game_progress03(reader: &mut BinaryReader) -> AppResult<HashMap<String, Value>> {
    let mut map = HashMap::new();
    map.insert("isFirstRun".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("legacyChapterFinished".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("alreadyShowCollectionTip".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("alreadyShowAutoUnlockINTip".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("completed".to_string(), Value::String(reader.read_string_aligned()?));
    map.insert("songUpdateInfo".to_string(), Value::Number(reader.read_var_int_aligned()?.into()));
    map.insert("challengeModeRank".to_string(), Value::Number(reader.read_short_int_aligned()?.into()));
    let money = reader.read_money_aligned()?;
    map.insert("money".to_string(), Value::Array(money.into_iter().map(|val| Value::Number(val.into())).collect()));
    map.insert("unlockFlagOfSpasmodic".to_string(), Value::Array(reader.read_bits(4)?.into_iter().map(Value::Bool).collect()));
    map.insert("unlockFlagOfIgallta".to_string(), Value::Array(reader.read_bits(4)?.into_iter().map(Value::Bool).collect()));
    map.insert("unlockFlagOfRrharil".to_string(), Value::Array(reader.read_bits(4)?.into_iter().map(Value::Bool).collect()));
    map.insert("flagOfSongRecordKey".to_string(), Value::Array(reader.read_bits(8)?.into_iter().map(Value::Bool).collect()));
    map.insert("randomVersionUnlocked".to_string(), Value::Array(reader.read_bits(6)?.into_iter().map(Value::Bool).collect()));
    map.insert("chapter8UnlockBegin".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("chapter8UnlockSecondPhase".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("chapter8Passed".to_string(), Value::Bool(reader.read_bool()?));
    map.insert("chapter8SongUnlocked".to_string(), Value::Array(reader.read_bits(6)?.into_iter().map(Value::Bool).collect()));
    Ok(map)
}

fn parse_game_progress04(reader: &mut BinaryReader) -> AppResult<HashMap<String, Value>> {
    let mut map = parse_game_progress03(reader)?;
    map.insert("flagOfSongRecordKeyTakumi".to_string(), Value::Array(reader.read_bits(3)?.into_iter().map(Value::Bool).collect()));
    Ok(map)
}

pub fn parse_save(save_data: &[u8]) -> AppResult<GameSave> {
    let save_dict = unzip_save(save_data)?;
    decrypt_save(save_dict)
}

pub fn parse_save_with_difficulty(save_data: &[u8]) -> AppResult<GameSave> {
    log::debug!("开始解析存档并添加难度和RKS信息...");
    let mut save = parse_save(save_data)?;
    log::debug!("基础存档解析完成，准备添加难度和RKS");
    
    if let Some(game_record) = &mut save.game_record {
        log::debug!("发现 GameRecord，共 {} 首歌曲", game_record.len());
        for (song_id, difficulties) in game_record.iter_mut() {
            log::trace!("处理歌曲: '{}'", song_id);
            for (diff_name, record) in difficulties.iter_mut() {
                log::trace!("  处理难度: '{}'", diff_name);

                if record.difficulty.is_none() {
                    log::trace!("    存档中无定数，尝试从 difficulty.csv 加载...");
                    if let Some(loaded_difficulty) = get_difficulty_by_id(song_id, diff_name) {
                        log::debug!("    成功从 difficulty.csv 加载定数 {} 用于 '{}' - '{}'", loaded_difficulty, song_id, diff_name);
                        record.difficulty = Some(loaded_difficulty);
                    } else {
                        log::warn!("    存档和 difficulty.csv 中均未找到歌曲 '{}' 难度 '{}' 的定数", song_id, diff_name);
                    }
                } else {
                    log::trace!("    存档中已包含定数: {:?}", record.difficulty);
                }

                if let Some(difficulty) = record.difficulty {
                    if difficulty > 0.0 {
                        if let Some(acc) = record.acc {
                            log::trace!("    有 ACC: {}, 定数: {}, 准备计算 RKS", acc, difficulty);
                            if acc >= 70.0 {
                                let rks = crate::utils::rks_utils::calculate_chart_rks(acc, difficulty);
                                log::trace!("    计算得到 RKS: {}", rks);
                                record.rks = Some(rks);
                            } else {
                                log::trace!("    ACC 不满足 RKS 计算条件 (acc={})", acc);
                                record.rks = Some(0.0);
                            }
                        } else {
                            log::trace!("    没有 ACC，无法计算 RKS");
                            record.rks = Some(0.0);
                        }
                    } else {
                         log::trace!("    定数为 0 或负数，不计算 RKS");
                         record.rks = Some(0.0);
                    }
                } else {
                     log::trace!("    无定数信息，无法计算 RKS");
                     record.rks = Some(0.0);
                }
            }
        }
        log::debug!("难度和 RKS 信息添加完成");
    } else {
        log::debug!("存档中没有 GameRecord 数据");
    }
    
    Ok(save)
}

#[allow(dead_code)]
pub fn calculate_rks(save: &GameSave) -> AppResult<RksResult> {
    let game_record = save.game_record.as_ref()
        .ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;
    
    let mut rks_records = Vec::new();
    
    for (song_id, difficulties) in game_record {
        let song_name = get_song_name_by_id(song_id).unwrap_or_else(|| song_id.clone());
        
        for (diff_name, record) in difficulties {
            if let Some(difficulty) = get_difficulty_by_id(song_id, diff_name) {
                if record.acc.unwrap_or(0.0) > 70.0 {
                    let rks_record = RksRecord::new(
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

#[allow(dead_code)]
pub fn get_summary_from_base64(summary_base64: &str) -> AppResult<SaveSummary> {
    let _summary_data = general_purpose::STANDARD.decode(summary_base64)?;
    
    Err(AppError::Other("摘要解析功能尚未完全实现".to_string()))
}

pub fn calculate_b30(save: &GameSave) -> AppResult<B30Result> {
    log::debug!("进入 calculate_b30 函数");
    let game_record = save.game_record.as_ref()
        .ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;
    log::debug!("B30: 获取到 GameRecord，包含 {} 首歌曲", game_record.len());

    log::debug!("B30: 开始并行收集有效成绩记录...");
    let mut all_played_records: Vec<B30Record> = game_record
        .par_iter()
        .flat_map(|(song_id, difficulties)| {
            difficulties
                .par_iter()
                .filter_map(|(diff_name, record)| {
                    if let (Some(acc), Some(difficulty)) = (record.acc, record.difficulty) {
                        if acc >= 70.0 && difficulty > 0.0 {
                            let rks = crate::utils::rks_utils::calculate_chart_rks(acc, difficulty);
                            let is_ap = record.score.map_or(false, |s| s == 1_000_000.0);
                            Some(B30Record {
                                song_id: song_id.clone(),
                                difficulty_str: diff_name.clone(),
                                score: record.score,
                                acc: Some(acc),
                                fc: record.fc,
                                difficulty: Some(difficulty),
                                rks: Some(rks),
                                is_ap,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    log::debug!("B30: 共收集到 {} 条有效成绩记录", all_played_records.len());
    if all_played_records.len() < 5 && !all_played_records.is_empty() {
        log::debug!("B30: 抽样几条记录: {:?}", all_played_records.iter().take(5).collect::<Vec<_>>());
    } else if all_played_records.is_empty() {
        log::warn!("B30: 未收集到任何有效成绩记录!");
    }

    log::debug!("B30: 开始计算 Top 27...");
    all_played_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
    let top_27: Vec<B30Record> = all_played_records.iter().take(27).cloned().collect();
    log::debug!("B30: Top 27 实际数量: {}", top_27.len());

    log::debug!("B30: 开始计算 Top 3 AP...");
    let mut ap_records: Vec<B30Record> = all_played_records.into_iter().filter(|r| r.is_ap).collect();
     log::debug!("B30: 找到 {} 条 AP 记录", ap_records.len());
    ap_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
    let top_3_ap: Vec<B30Record> = ap_records.into_iter().take(3).collect();
    log::debug!("B30: Top 3 AP 实际数量: {}", top_3_ap.len());

    log::debug!("B30: 开始计算最终 Overall RKS...");
    let total_rks_sum: f64 = top_27.iter().chain(top_3_ap.iter())
                                .filter_map(|r| r.rks)
                                .sum();
    log::debug!("B30: Top 27 和 Top 3 AP 的 RKS 总和: {}", total_rks_sum);
    
    let overall_rks = if !top_27.is_empty() || !top_3_ap.is_empty() {
        log::debug!("B30: 使用固定分母 30 计算 Overall RKS");
        total_rks_sum / 30.0 
    } else {
         log::debug!("B30: 没有有效记录，Overall RKS 为 0");
        0.0
    };
    log::info!("B30: 最终计算得到 Overall RKS: {}", overall_rks);

    Ok(B30Result {
        overall_rks,
        top_27,
        top_3_ap,
    })
}