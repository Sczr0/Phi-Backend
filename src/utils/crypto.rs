use aes::Aes256;
use base64::{engine::general_purpose, Engine as _};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use cbc::{Decryptor, Encryptor};
use md5::{Digest, Md5};
use once_cell::sync::Lazy; // 引入 once_cell 来实现单次初始化

use crate::config::{AES_IV_BASE64, AES_KEY_BASE64};
use crate::utils::error::{AppError, AppResult};

// --- 1. 使用 Lazy 实现密钥和IV的单次初始化 ---
// 它们在程序启动时只会被解码一次，然后被缓存。
// 如果配置中的key/iv有问题，程序会直接panic，这是合理的，因为这是严重配置错误。
static AES_KEY: Lazy<[u8; 32]> = Lazy::new(|| {
    let key_vec = general_purpose::STANDARD
        .decode(AES_KEY_BASE64)
        .expect("无法解码配置中的AES密钥 (AES_KEY_BASE64)");
    key_vec
        .try_into()
        .expect("配置中的AES密钥长度必须是32字节")
});

static AES_IV: Lazy<[u8; 16]> = Lazy::new(|| {
    let iv_vec = general_purpose::STANDARD
        .decode(AES_IV_BASE64)
        .expect("无法解码配置中的AES IV (AES_IV_BASE64)");
    iv_vec
        .try_into()
        .expect("配置中的AES IV长度必须是16字节")
});

#[allow(dead_code)]
pub fn encrypt(data: &[u8]) -> AppResult<Vec<u8>> {
    // 直接使用已经初始化好的静态 KEY 和 IV
    // new_from_slices 已经隐式地验证了长度，因为我们用了固定长度数组 [u8; 32]
    let cipher = Encryptor::<Aes256>::new_from_slices(&*AES_KEY, &*AES_IV)
        .map_err(|e| AppError::AesError(format!("AES加密器初始化失败: {e}")))?;

    // 直接让库处理填充和加密
    let result = cipher.encrypt_padded_vec_mut::<Pkcs7>(data);
    Ok(result)
}

// --- 3解密函数 ---
pub fn decrypt(data: &[u8]) -> AppResult<Vec<u8>> {
    // 同样，直接使用静态 KEY 和 IV
    let cipher = Decryptor::<Aes256>::new_from_slices(&*AES_KEY, &*AES_IV)
        .map_err(|e| AppError::AesError(format!("AES解密器初始化失败: {e}")))?;

    // 库会处理解密和去填充，如果填充错误或数据损坏，这里会返回Err
    cipher
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|e| {
            log::error!("AES解密失败: {e}");
            // 这里的错误比简单的 to_string() 信息更丰富
            AppError::AesError(format!("解密或去填充失败: {e}"))
        })
}

#[allow(dead_code)]
pub fn calculate_md5(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{result:x}")
}


// --- session token 验证 ---
pub fn validate_session_token(token: &str) -> bool {
    if token.len() != 25 {
        return false;
    }
    token.chars().all(|c| c.is_ascii_alphanumeric() && c.is_ascii_lowercase())
}