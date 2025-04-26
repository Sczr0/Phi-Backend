use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use aes::Aes128;
use serde_json::Value;
use std::io::{Cursor, Read};
use flate2::read::ZlibDecoder;
use base64::Engine;
use base64::engine::general_purpose;
use cbc::Decryptor;
use block_padding::{Pkcs7, UnpadError};
use aes::cipher::{KeyIvInit, BlockDecryptMut};

use crate::utils::config;
use crate::utils::error::{AppError, AppResult};

/// 解密保存数据
/// 
/// 使用AES-128 CBC模式解密保存数据
/// 
/// # Arguments
/// * `data` - 加密的游戏存档数据
/// 
/// # Returns
/// * `AppResult<String>` - 解密后的JSON字符串
pub fn decrypt_save_data(data: &[u8]) -> AppResult<String> {
    let key_bytes = config::get_config()?.get_aes_key_bytes()?;
    
    // AES-128需要16字节密钥
    let key = GenericArray::from_slice(&key_bytes);
    let cipher = Aes128::new(key);
    
    // 检查数据长度是否为16的倍数
    if data.len() % 16 != 0 {
        return Err(AppError::SaveDecryptError(
            "加密数据长度不是16的倍数".to_string()
        ));
    }
    
    let mut decrypted_data = data.to_vec();
    
    // 按16字节块解密
    for chunk in decrypted_data.chunks_mut(16) {
        let block = GenericArray::from_mut_slice(chunk);
        cipher.decrypt_block(block);
    }
    
    // 移除PKCS#7填充
    let padding = decrypted_data.last().unwrap_or(&0);
    let padding = *padding as usize;
    
    if padding > 16 || padding == 0 {
        return Err(AppError::SaveDecryptError(
            "无效的PKCS#7填充".to_string()
        ));
    }
    
    let data_len = decrypted_data.len() - padding;
    decrypted_data.truncate(data_len);
    
    // 解压缩数据
    let mut decoder = ZlibDecoder::new(Cursor::new(decrypted_data));
    let mut decompressed_data = Vec::new();
    decoder.read_to_end(&mut decompressed_data).map_err(|e| {
        AppError::SaveDecryptError(format!("解压缩失败: {}", e))
    })?;
    
    // 转换为UTF-8字符串
    let json_str = String::from_utf8(decompressed_data).map_err(|e| {
        AppError::SaveDecryptError(format!("UTF-8解码失败: {}", e))
    })?;
    
    Ok(json_str)
}

/// 解析JSON数据
/// 
/// # Arguments
/// * `json_str` - JSON字符串
/// 
/// # Returns
/// * `AppResult<Value>` - 解析后的JSON值
pub fn parse_json(json_str: &str) -> AppResult<Value> {
    serde_json::from_str(json_str).map_err(|e| {
        AppError::SaveDecryptError(format!("JSON解析失败: {}", e))
    })
}

/// 解密并解析保存数据
/// 
/// # Arguments
/// * `data` - 加密的游戏存档数据
/// 
/// # Returns
/// * `AppResult<Value>` - 解析后的JSON值
pub fn decrypt_and_parse(data: &[u8]) -> AppResult<Value> {
    let json_str = decrypt_save_data(data)?;
    parse_json(&json_str)
}

/// 解密存档数据
/// 
/// 输入加密的游戏存档数据，返回解密后的JSON字符串
pub fn decrypt_save_data_base64(encrypted_data: &str) -> AppResult<String> {
    // 解码base64
    let data = general_purpose::STANDARD.decode(encrypted_data)
        .map_err(|e| AppError::DecodeError(e))?;
    
    // 检查数据长度
    if data.len() < 16 {
        return Err(AppError::SaveDecryptError("数据长度不足".to_string()));
    }
    
    // 固定的AES密钥和IV (这里使用示例密钥，实际应该从配置中读取)
    let key = b"PhigrosDecrypKey";  // 16字节AES-128密钥
    let iv = &data[0..16];  // 前16字节作为IV
    let ciphertext = &data[16..];

    // 创建解密器
    let cipher = Decryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|e| AppError::SaveDecryptError(format!("创建解密器失败 (Key/IV length error): {}", e)))?;

    // 解密数据 (使用 Pkcs7 padding)
    let decrypted_data = cipher.decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
         .map_err(|e| match e {
             UnpadError => AppError::SaveDecryptError("AES解密失败: 无效的 PKCS#7 填充".to_string()),
             // Handle other potential error kinds if they exist for decrypt_padded_vec_mut
             // _ => AppError::SaveDecryptError("AES解密时发生未知错误".to_string()),
         })?;
    
    // 使用zlib解压缩
    let mut decoder = ZlibDecoder::new(decrypted_data.as_slice());
    let mut decompressed_data = Vec::new();
    decoder.read_to_end(&mut decompressed_data)
        .map_err(|e| AppError::SaveDecryptError(format!("Zlib解压失败: {}", e)))?;
    
    // 转换为UTF-8字符串
    let json_str = String::from_utf8(decompressed_data)
        .map_err(|e| AppError::SaveDecryptError(format!("UTF-8转换失败: {}", e)))?;
    
    Ok(json_str)
}

/// 解密并解析存档数据
/// 
/// 组合解密和解析步骤
pub fn decrypt_and_parse_base64(encrypted_data: &str) -> AppResult<Value> {
    let json_str = decrypt_save_data_base64(encrypted_data)?;
    parse_json(&json_str)
} 