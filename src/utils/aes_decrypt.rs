use aes::cipher::{generic_array::GenericArray, BlockDecryptMut, KeyIvInit};
use aes::Aes128;
use block_padding::Pkcs7;
use cbc::Decryptor as CbcDecryptor;
use flate2::read::ZlibDecoder;
use std::io::{Cursor, Read};

use crate::utils::config;
use crate::utils::error::{AppError, AppResult};

/// 解密保存数据
///
/// 使用AES-128 CBC模式解密保存数据，使用库函数处理解密和去填充
///
/// # Arguments
/// * `data` - 加密的游戏存档数据
///
/// # Returns
/// * `AppResult<String>` - 解密后的JSON字符串
#[allow(dead_code)]
pub fn decrypt_save_data(data: &[u8]) -> AppResult<String> {
    let key_bytes = config::get_config()?.get_aes_key_bytes()?;

    // AES-128需要16字节密钥
    let key = GenericArray::from_slice(&key_bytes);
    let iv = GenericArray::from_slice(&[0u8; 16]); // 使用零IV，与原实现保持一致
    let cipher = CbcDecryptor::<Aes128>::new(key, iv);

    // 使用库函数解密并自动处理PKCS#7去填充
    let decrypted_data = cipher
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|e| AppError::SaveDecryptError(format!("AES解密或去填充失败: {e}")))?;

    // 解压缩数据
    let mut decoder = ZlibDecoder::new(Cursor::new(decrypted_data));
    let mut decompressed_data = Vec::new();
    decoder
        .read_to_end(&mut decompressed_data)
        .map_err(|e| AppError::SaveDecryptError(format!("解压缩失败: {e}")))?;

    // 转换为UTF-8字符串
    let json_str = String::from_utf8(decompressed_data)
        .map_err(|e| AppError::SaveDecryptError(format!("UTF-8解码失败: {e}")))?;

    Ok(json_str)
}
