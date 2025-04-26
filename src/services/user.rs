use crate::models::{PhigrosUser, UnbindVerificationCode};
use crate::utils::error::{AppError, AppResult};
use sqlx::SqlitePool;
use chrono::{Duration, Utc};
use rand::{distributions::Alphanumeric, Rng};

// 用户服务，管理QQ和SessionToken的绑定关系
#[derive(Clone)]
pub struct UserService {
    // 使用 SQLite 数据库存储
    pool: SqlitePool,
}

impl UserService {
    // 创建新的用户服务
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // 检查 QQ 号是否已绑定
    pub async fn is_qq_bound(&self, qq: &str) -> AppResult<bool> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_bindings WHERE qq = ?")
            .bind(qq)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("检查QQ绑定时出错: {}", e)))?;
        Ok(count.0 > 0)
    }

    // 根据QQ号查找用户
    pub async fn get_user_by_qq(&self, qq: &str) -> AppResult<PhigrosUser> {
        sqlx::query_as::<_, PhigrosUser>("SELECT * FROM user_bindings WHERE qq = ?")
            .bind(qq)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("获取用户时数据库错误: {}", e)))?
            .ok_or(AppError::UserBindingNotFound(format!("未找到 QQ {} 的绑定", qq)))
    }

    // 根据会话令牌查找用户
    pub async fn get_user_by_token(&self, token: &str) -> AppResult<PhigrosUser> {
        // Note: This might return *one* user even if multiple QQs are bound to the same token.
        // Depending on usage, you might need a function that returns Vec<PhigrosUser>.
        sqlx::query_as::<_, PhigrosUser>("SELECT * FROM user_bindings WHERE session_token = ? LIMIT 1")
            .bind(token)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("获取用户时数据库错误: {}", e)))?
            .ok_or(AppError::UserBindingNotFound(format!("未找到 Token 的绑定")))
    }

    // 保存用户绑定 (INSERT OR REPLACE 会自动处理更新)
    pub async fn save_user(&self, user: PhigrosUser) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO user_bindings (qq, session_token, nickname, last_update)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&user.qq)
        .bind(&user.session_token)
        .bind(&user.nickname)
        .bind(&user.last_update)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("保存用户绑定时出错: {}", e)))?;
        
        Ok(())
    }

    // 删除用户绑定
    pub async fn delete_user(&self, qq: &str) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM user_bindings WHERE qq = ?")
            .bind(qq)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("删除用户绑定时出错: {}", e)))?;
        
        // 检查是否有记录被删除
        if result.rows_affected() == 0 {
            return Err(AppError::UserBindingNotFound(format!("删除失败：未找到 QQ {} 的绑定", qq)));
        }
        
        Ok(())
    }

    // --- Verification Code Methods ---

    pub async fn generate_and_store_verification_code(
        &self,
        qq: &str,
    ) -> AppResult<UnbindVerificationCode> {
        let code: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8) // Generate an 8-character alphanumeric code
            .map(char::from)
            .collect();
        
        let expires_at = Utc::now() + Duration::minutes(5); // Code expires in 5 minutes

        // Assume unbind_verification_codes table exists
        // Use INSERT OR REPLACE to handle existing requests for the same QQ
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO unbind_verification_codes (qq, code, expires_at)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(qq)
        .bind(&code)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("存储验证码时出错: {}", e)))?;

        Ok(UnbindVerificationCode {
            qq: qq.to_string(),
            code,
            expires_at,
        })
    }

    pub async fn validate_and_consume_verification_code(
        &self,
        qq: &str,
        provided_code: &str,
    ) -> AppResult<()> {
        // Fetch the stored code details
        let stored_code_details = sqlx::query_as::<_, UnbindVerificationCode>(
            "SELECT qq, code, expires_at FROM unbind_verification_codes WHERE qq = ?",
        )
        .bind(qq)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询验证码时出错: {}", e)))?;

        match stored_code_details {
            Some(details) => {
                // Check expiry
                if Utc::now() > details.expires_at {
                    // Code expired, delete it
                    let _ = self.delete_verification_code(qq).await; // Attempt deletion, ignore error
                    log::warn!("验证码已过期 for QQ: {}", qq);
                    return Err(AppError::VerificationCodeExpired);
                }
                
                // Check code match
                if details.code != provided_code {
                    log::warn!("验证码不匹配 for QQ: {}. Expected: {}, Provided: {}", qq, details.code, provided_code);
                    return Err(AppError::VerificationCodeInvalid);
                }

                // Code is valid and matches, consume (delete) it
                self.delete_verification_code(qq).await?; 
                log::info!("验证码验证成功并已消费 for QQ: {}", qq);
                Ok(())
            }
            None => {
                log::warn!("未找到QQ {} 的验证码请求", qq);
                Err(AppError::VerificationCodeNotFound)
            }
        }
    }

    // Helper to delete code
    async fn delete_verification_code(&self, qq: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM unbind_verification_codes WHERE qq = ?")
            .bind(qq)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("删除验证码时出错: {}", e)))?;
        Ok(())
    }
} 