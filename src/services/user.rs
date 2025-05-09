use crate::models::{InternalUser, PlatformBinding, TokenListResponse, PlatformBindingInfo, UnbindVerificationCode};
use crate::utils::error::{AppError, AppResult};
use sqlx::SqlitePool;
use chrono::{Duration, Utc};
use rand::{distributions::Alphanumeric, Rng};

// 用户服务，管理内部ID和平台绑定关系
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

    // 检查平台账号是否已绑定
    pub async fn is_platform_id_bound(&self, platform: &str, platform_id: &str) -> AppResult<bool> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM platform_bindings WHERE platform = ? AND platform_id = ?"
        )
        .bind(&platform)
        .bind(platform_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("检查平台ID绑定时出错: {}", e)))?;
        Ok(count.0 > 0)
    }

    // 根据平台和平台ID查找绑定信息
    pub async fn get_binding_by_platform_id(&self, platform: &str, platform_id: &str) -> AppResult<PlatformBinding> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        sqlx::query_as::<_, PlatformBinding>(
            "SELECT * FROM platform_bindings WHERE platform = ? AND platform_id = ?"
        )
        .bind(&platform)
        .bind(platform_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取绑定信息时数据库错误: {}", e)))?
        .ok_or(AppError::UserBindingNotFound(format!("未找到平台 {} 的 ID {} 的绑定", platform, platform_id)))
    }

    // 根据会话令牌查找绑定信息
    pub async fn get_binding_by_token(&self, token: &str) -> AppResult<PlatformBinding> {
        sqlx::query_as::<_, PlatformBinding>(
            "SELECT * FROM platform_bindings WHERE session_token = ? LIMIT 1"
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取绑定信息时数据库错误: {}", e)))?
        .ok_or(AppError::UserBindingNotFound(format!("未找到 Token 的绑定")))
    }

    // 根据内部ID获取所有绑定信息
    pub async fn get_bindings_by_internal_id(&self, internal_id: &str) -> AppResult<Vec<PlatformBinding>> {
        sqlx::query_as::<_, PlatformBinding>(
            "SELECT * FROM platform_bindings WHERE internal_id = ?"
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取内部ID绑定信息时数据库错误: {}", e)))
    }

    // 获取内部用户信息
    pub async fn get_internal_user(&self, internal_id: &str) -> AppResult<InternalUser> {
        sqlx::query_as::<_, InternalUser>(
            "SELECT * FROM internal_users WHERE internal_id = ?"
        )
        .bind(internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取内部用户信息时数据库错误: {}", e)))?
        .ok_or(AppError::UserNotFound(format!("未找到内部ID为 {} 的用户", internal_id)))
    }

    // 创建内部用户
    pub async fn create_internal_user(&self, nickname: Option<String>) -> AppResult<InternalUser> {
        let user = InternalUser::new(nickname);
        
        sqlx::query(
            "INSERT INTO internal_users (internal_id, nickname, update_time) VALUES (?, ?, ?)"
        )
        .bind(&user.internal_id)
        .bind(&user.nickname)
        .bind(&user.update_time)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("创建内部用户时出错: {}", e)))?;
        
        Ok(user)
    }

    // 保存平台绑定
    pub async fn save_platform_binding(&self, binding: &PlatformBinding) -> AppResult<()> {
        // 确保平台名称小写
        let platform = binding.platform.to_lowercase();
        
        sqlx::query(
            r#"
            INSERT INTO platform_bindings (internal_id, platform, platform_id, session_token, bind_time)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&binding.internal_id)
        .bind(&platform)
        .bind(&binding.platform_id)
        .bind(&binding.session_token)
        .bind(&binding.bind_time)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("保存平台绑定时出错: {}", e)))?;
        
        Ok(())
    }

    // 更新平台绑定的token
    pub async fn update_platform_binding_token(&self, platform: &str, platform_id: &str, new_token: &str) -> AppResult<()> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        sqlx::query(
            "UPDATE platform_bindings SET session_token = ?, bind_time = ? WHERE platform = ? AND platform_id = ?"
        )
        .bind(new_token)
        .bind(Utc::now().to_rfc3339())
        .bind(&platform)
        .bind(platform_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("更新平台绑定token时出错: {}", e)))?;
        
        Ok(())
    }

    // 获取指定内部ID的所有绑定信息（用于展示）
    pub async fn get_token_list(&self, internal_id: &str) -> AppResult<TokenListResponse> {
        let bindings = self.get_bindings_by_internal_id(internal_id).await?;
        
        let binding_infos: Vec<PlatformBindingInfo> = bindings.into_iter()
            .map(|b| PlatformBindingInfo {
                platform: b.platform,
                platform_id: b.platform_id,
                session_token: b.session_token,
                bind_time: b.bind_time,
            })
            .collect();
        
        Ok(TokenListResponse {
            internal_id: internal_id.to_string(),
            bindings: binding_infos,
        })
    }

    // 删除平台绑定
    pub async fn delete_platform_binding(&self, platform: &str, platform_id: &str) -> AppResult<String> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        // 先获取内部ID
        let binding = self.get_binding_by_platform_id(&platform, platform_id).await?;
        let internal_id = binding.internal_id.clone();
        
        // 删除绑定
        let result = sqlx::query(
            "DELETE FROM platform_bindings WHERE platform = ? AND platform_id = ?"
        )
        .bind(&platform)
        .bind(platform_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("删除平台绑定时出错: {}", e)))?;
        
        // 检查是否有记录被删除
        if result.rows_affected() == 0 {
            return Err(AppError::UserBindingNotFound(
                format!("删除失败：未找到平台 {} 的 ID {} 的绑定", platform, platform_id)
            ));
        }
        
        // 检查内部ID是否还有其他绑定
        let remaining_bindings: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM platform_bindings WHERE internal_id = ?"
        )
        .bind(&internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("检查剩余绑定时出错: {}", e)))?;
        
        // 如果没有其他绑定，删除内部用户
        if remaining_bindings.0 == 0 {
            sqlx::query("DELETE FROM internal_users WHERE internal_id = ?")
                .bind(&internal_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AppError::DatabaseError(format!("删除内部用户时出错: {}", e)))?;
        }
        
        Ok(internal_id)
    }

    // --- Verification Code Methods ---

    pub async fn generate_and_store_verification_code(
        &self,
        platform: &str,
        platform_id: &str,
    ) -> AppResult<UnbindVerificationCode> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        let code: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8) // Generate an 8-character alphanumeric code
            .map(char::from)
            .collect();
        
        let expires_at = Utc::now() + Duration::minutes(5); // Code expires in 5 minutes

        // Assume unbind_verification_codes table exists
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO unbind_verification_codes (platform, platform_id, code, expires_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&platform)
        .bind(platform_id)
        .bind(&code)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("存储验证码时出错: {}", e)))?;

        Ok(UnbindVerificationCode {
            platform: platform.to_string(),
            platform_id: platform_id.to_string(),
            code,
            expires_at,
        })
    }

    pub async fn validate_and_consume_verification_code(
        &self,
        platform: &str,
        platform_id: &str,
        provided_code: &str,
    ) -> AppResult<()> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        // Fetch the stored code details
        let stored_code_details = sqlx::query_as::<_, UnbindVerificationCode>(
            "SELECT platform, platform_id, code, expires_at FROM unbind_verification_codes WHERE platform = ? AND platform_id = ?"
        )
        .bind(&platform)
        .bind(platform_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询验证码时出错: {}", e)))?;

        match stored_code_details {
            Some(details) => {
                // Check expiry
                if Utc::now() > details.expires_at {
                    // Code expired, delete it
                    let _ = self.delete_verification_code(&platform, platform_id).await; // Attempt deletion, ignore error
                    log::warn!("验证码已过期 for 平台: {}, ID: {}", platform, platform_id);
                    return Err(AppError::VerificationCodeExpired);
                }
                
                // Check code match
                if details.code != provided_code {
                    log::warn!("验证码不匹配 for 平台: {}, ID: {}. Expected: {}, Provided: {}", 
                        platform, platform_id, details.code, provided_code);
                    return Err(AppError::VerificationCodeInvalid);
                }

                // Code is valid and matches, consume (delete) it
                self.delete_verification_code(&platform, platform_id).await?; 
                log::info!("验证码验证成功并已消费 for 平台: {}, ID: {}", platform, platform_id);
                Ok(())
            }
            None => {
                log::warn!("未找到平台 {} 的 ID {} 的验证码请求", platform, platform_id);
                Err(AppError::VerificationCodeNotFound)
            }
        }
    }

    // Helper to delete code
    async fn delete_verification_code(&self, platform: &str, platform_id: &str) -> AppResult<()> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        sqlx::query("DELETE FROM unbind_verification_codes WHERE platform = ? AND platform_id = ?")
            .bind(&platform)
            .bind(platform_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("删除验证码时出错: {}", e)))?;
        Ok(())
    }

    // 辅助函数：当提供sessionToken时，返回或创建关联的内部ID
    pub async fn get_or_create_internal_id_by_token(&self, token: &str, platform: &str, platform_id: &str) -> AppResult<String> {
        // 确保平台名称小写
        let platform = platform.to_lowercase();
        
        // 先检查这个token是否已经绑定过
        match self.get_binding_by_token(token).await {
            Ok(existing_binding) => {
                // Token已绑定，返回关联的内部ID
                Ok(existing_binding.internal_id)
            },
            Err(AppError::UserBindingNotFound(_)) => {
                // Token未绑定，检查平台ID是否已绑定
                match self.get_binding_by_platform_id(&platform, platform_id).await {
                    Ok(existing_platform_binding) => {
                        // 平台ID已绑定，更新token
                        self.update_platform_binding_token(&platform, platform_id, token).await?;
                        Ok(existing_platform_binding.internal_id)
                    },
                    Err(AppError::UserBindingNotFound(_)) => {
                        // 平台ID未绑定，创建新用户并绑定
                        let new_user = self.create_internal_user(None).await?;
                        let binding = PlatformBinding::new(
                            new_user.internal_id.clone(),
                            platform.to_string(),
                            platform_id.to_string(),
                            token.to_string()
                        );
                        self.save_platform_binding(&binding).await?;
                        Ok(new_user.internal_id)
                    },
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(e)
        }
    }
} 