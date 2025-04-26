use aes::Aes256;
use base64::{engine::general_purpose, Engine as _};
use cbc::{cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit}, Decryptor, Encryptor};
use md5::{Digest, Md5};

use crate::config::{AES_IV_BASE64, AES_KEY_BASE64};
use crate::utils::error::{AppError, AppResult};

// 获取AES密钥和IV
fn get_aes_key_iv() -> AppResult<(Vec<u8>, Vec<u8>)> {
    let key = general_purpose::STANDARD.decode(AES_KEY_BASE64)?;
    let iv = general_purpose::STANDARD.decode(AES_IV_BASE64)?;
    
    log::debug!("AES密钥长度: {} 字节", key.len());  // 应该是32字节 (256位)
    log::debug!("AES IV长度: {} 字节", iv.len());    // 应该是16字节 (128位)
    
    Ok((key, iv))
}

// AES CBC加密
pub fn encrypt(data: &[u8]) -> AppResult<Vec<u8>> {
    let (key, iv) = get_aes_key_iv()?;
    
    if key.len() != 32 {
        return Err(AppError::AesError(format!("AES密钥长度不正确，期望32字节，实际{}字节", key.len())));
    }
    
    if iv.len() != 16 {
        return Err(AppError::AesError(format!("AES IV长度不正确，期望16字节，实际{}字节", iv.len())));
    }
    
    // 使用CBC模式加密
    type Aes256CbcEnc = Encryptor<Aes256>;
    let cipher = Aes256CbcEnc::new_from_slices(&key, &iv)
        .map_err(|e| AppError::AesError(e.to_string()))?;
    
    let padded_data = pkcs7_pad(data, 16);
    log::debug!("原始数据长度: {} 字节, 填充后长度: {} 字节", data.len(), padded_data.len());
    
    // 使用自定义实现加密
    let result = cipher.encrypt_padded_vec_mut::<Pkcs7>(data);
    log::debug!("加密后数据长度: {} 字节", result.len());
    
    Ok(result)
}

// AES CBC解密
pub fn decrypt(data: &[u8]) -> AppResult<Vec<u8>> {
    let (key, iv) = get_aes_key_iv()?;
    
    if key.len() != 32 {
        return Err(AppError::AesError(format!("AES密钥长度不正确，期望32字节，实际{}字节", key.len())));
    }
    
    if iv.len() != 16 {
        return Err(AppError::AesError(format!("AES IV长度不正确，期望16字节，实际{}字节", iv.len())));
    }
    
    if data.len() % 16 != 0 {
        return Err(AppError::AesError(format!("加密数据长度必须是16的倍数，实际长度: {}", data.len())));
    }
    
    log::debug!("加密数据长度: {} 字节", data.len());
    
    // 使用CBC模式解密
    type Aes256CbcDec = Decryptor<Aes256>;
    let cipher = Aes256CbcDec::new_from_slices(&key, &iv)
        .map_err(|e| AppError::AesError(e.to_string()))?;
    
    match cipher.decrypt_padded_vec_mut::<Pkcs7>(data) {
        Ok(result) => {
            log::debug!("解密成功，解密后数据长度: {} 字节", result.len());
            Ok(result)
        },
        Err(e) => {
            log::error!("解密失败: {}", e);
            Err(AppError::AesError(e.to_string()))
        }
    }
}

// PKCS#7 填充
fn pkcs7_pad(data: &[u8], block_size: usize) -> Vec<u8> {
    let padding_size = block_size - (data.len() % block_size);
    let mut padded_data = data.to_vec();
    padded_data.extend(vec![padding_size as u8; padding_size]);
    padded_data
}

// 计算MD5校验和
pub fn calculate_md5(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

// 验证sessionToken格式
pub fn validate_session_token(token: &str) -> bool {
    if token.is_empty() || token.len() != 25 {
        return false;
    }
    
    token.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
} 