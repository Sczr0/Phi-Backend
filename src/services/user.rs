use crate::models::user::{
    InternalUser, PlatformBinding, PlatformBindingInfo, TokenListResponse, UnbindVerificationCode,
};
use crate::utils::error::{AppError, AppResult};
use chrono::{Duration, Utc};
use rand::Rng;
use sqlx::SqlitePool;

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
        let platform = platform.to_lowercase();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM platform_bindings WHERE platform = ? AND platform_id = ?",
        )
        .bind(&platform)
        .bind(platform_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("检查平台ID绑定时出错: {e}")))?;
        Ok(count.0 > 0)
    }

    // 根据平台和平台ID查找绑定信息
    pub async fn get_binding_by_platform_id(
        &self,
        platform: &str,
        platform_id: &str,
    ) -> AppResult<PlatformBinding> {
        let platform = platform.to_lowercase();

        sqlx::query_as::<_, PlatformBinding>(
            "SELECT * FROM platform_bindings WHERE platform = ? AND platform_id = ?",
        )
        .bind(&platform)
        .bind(platform_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取绑定信息时数据库错误: {e}")))?
        .ok_or(AppError::UserBindingNotFound(format!(
            "未找到平台 {platform} 的 ID {platform_id} 的绑定"
        )))
    }

    // 根据会话令牌查找绑定信息
    pub async fn get_binding_by_token(&self, token: &str) -> AppResult<PlatformBinding> {
        sqlx::query_as::<_, PlatformBinding>(
            "SELECT * FROM platform_bindings WHERE session_token = ? LIMIT 1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取绑定信息时数据库错误: {e}")))?
        .ok_or(AppError::UserBindingNotFound(
            "未找到 Token 的绑定".to_string(),
        ))
    }

    // 根据内部ID获取所有绑定信息
    pub async fn get_bindings_by_internal_id(
        &self,
        internal_id: &str,
    ) -> AppResult<Vec<PlatformBinding>> {
        sqlx::query_as::<_, PlatformBinding>(
            "SELECT * FROM platform_bindings WHERE internal_id = ?",
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("获取内部ID绑定信息时数据库错误: {e}")))
    }

    // 获取内部用户信息
    #[allow(dead_code)]
    pub async fn get_internal_user(&self, internal_id: &str) -> AppResult<InternalUser> {
        sqlx::query_as::<_, InternalUser>("SELECT * FROM internal_users WHERE internal_id = ?")
            .bind(internal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("获取内部用户信息时数据库错误: {e}")))?
            .ok_or(AppError::UserNotFound(format!(
                "未找到内部ID为 {internal_id} 的用户"
            )))
    }

    // 创建内部用户
    pub async fn create_internal_user(&self, nickname: Option<String>) -> AppResult<InternalUser> {
        let user = InternalUser::new(nickname);

        sqlx::query(
            "INSERT INTO internal_users (internal_id, nickname, update_time) VALUES (?, ?, ?)",
        )
        .bind(&user.internal_id)
        .bind(&user.nickname)
        .bind(&user.update_time)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("创建内部用户时出错: {e}")))?;

        Ok(user)
    }

    // 保存平台绑定
    pub async fn save_platform_binding(&self, binding: &PlatformBinding) -> AppResult<()> {
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
        .map_err(|e| AppError::DatabaseError(format!("保存平台绑定时出错: {e}")))?;

        Ok(())
    }

    // 更新平台绑定的token
    pub async fn update_platform_binding_token(
        &self,
        platform: &str,
        platform_id: &str,
        new_token: &str,
    ) -> AppResult<()> {
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
        .map_err(|e| AppError::DatabaseError(format!("更新平台绑定token时出错: {e}")))?;

        Ok(())
    }

    // 获取指定内部ID的所有绑定信息（用于展示）
    pub async fn get_token_list(&self, internal_id: &str) -> AppResult<TokenListResponse> {
        let bindings = self.get_bindings_by_internal_id(internal_id).await?;

        let binding_infos: Vec<PlatformBindingInfo> = bindings
            .into_iter()
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
    pub async fn delete_platform_binding(
        &self,
        platform: &str,
        platform_id: &str,
    ) -> AppResult<String> {
        let platform = platform.to_lowercase();

        let binding = self
            .get_binding_by_platform_id(&platform, platform_id)
            .await?;
        let internal_id = binding.internal_id.clone();

        let result =
            sqlx::query("DELETE FROM platform_bindings WHERE platform = ? AND platform_id = ?")
                .bind(&platform)
                .bind(platform_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AppError::DatabaseError(format!("删除平台绑定时出错: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(AppError::UserBindingNotFound(format!(
                "删除失败：未找到平台 {platform} 的 ID {platform_id} 的绑定"
            )));
        }

        let remaining_bindings: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM platform_bindings WHERE internal_id = ?")
                .bind(&internal_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AppError::DatabaseError(format!("检查剩余绑定时出错: {e}")))?;

        if remaining_bindings.0 == 0 {
            sqlx::query("DELETE FROM internal_users WHERE internal_id = ?")
                .bind(&internal_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AppError::DatabaseError(format!("删除内部用户时出错: {e}")))?;
        }

        Ok(internal_id)
    }

    // --- Verification Code Methods ---

    pub async fn generate_and_store_verification_code(
        &self,
        platform: &str,
        platform_id: &str,
    ) -> AppResult<UnbindVerificationCode> {
        let platform = platform.to_lowercase();

        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789";
        let mut rng = rand::thread_rng();
        let code: String = (0..8)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        let expires_at = Utc::now() + Duration::minutes(5);

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
        .map_err(|e| AppError::DatabaseError(format!("存储验证码时出错: {e}")))?;

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
        let platform = platform.to_lowercase();

        let stored_code_details = sqlx::query_as::<_, UnbindVerificationCode>(
            "SELECT platform, platform_id, code, expires_at FROM unbind_verification_codes WHERE platform = ? AND platform_id = ?"
        )
        .bind(&platform)
        .bind(platform_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(format!("查询验证码时出错: {e}")))?;

        match stored_code_details {
            Some(details) => {
                if Utc::now() > details.expires_at {
                    let _ = self.delete_verification_code(&platform, platform_id).await;
                    log::warn!("验证码已过期 for 平台: {platform}, ID: {platform_id}");
                    return Err(AppError::VerificationCodeExpired);
                }

                if details.code != provided_code {
                    log::warn!(
                        "验证码不匹配 for 平台: {}, ID: {}. Expected: {}, Provided: {}",
                        platform,
                        platform_id,
                        details.code,
                        provided_code
                    );
                    return Err(AppError::VerificationCodeInvalid);
                }

                self.delete_verification_code(&platform, platform_id)
                    .await?;
                log::info!("验证码验证成功并已消费 for 平台: {platform}, ID: {platform_id}");
                Ok(())
            }
            None => {
                log::warn!("未找到平台 {platform} 的 ID {platform_id} 的验证码请求");
                Err(AppError::VerificationCodeNotFound)
            }
        }
    }

    async fn delete_verification_code(&self, platform: &str, platform_id: &str) -> AppResult<()> {
        let platform = platform.to_lowercase();

        sqlx::query("DELETE FROM unbind_verification_codes WHERE platform = ? AND platform_id = ?")
            .bind(&platform)
            .bind(platform_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::DatabaseError(format!("删除验证码时出错: {e}")))?;
        Ok(())
    }

    pub async fn get_or_create_internal_id_by_token(
        &self,
        token: &str,
        platform: &str,
        platform_id: &str,
    ) -> AppResult<String> {
        let platform = platform.to_lowercase();

        match self.get_binding_by_token(token).await {
            Ok(existing_binding) => Ok(existing_binding.internal_id),
            Err(AppError::UserBindingNotFound(_)) => {
                match self
                    .get_binding_by_platform_id(&platform, platform_id)
                    .await
                {
                    Ok(existing_platform_binding) => {
                        self.update_platform_binding_token(&platform, platform_id, token)
                            .await?;
                        Ok(existing_platform_binding.internal_id)
                    }
                    Err(AppError::UserBindingNotFound(_)) => {
                        let new_user = self.create_internal_user(None).await?;
                        let binding = PlatformBinding::new(
                            new_user.internal_id.clone(),
                            platform.to_string(),
                            platform_id.to_string(),
                            token.to_string(),
                        );
                        self.save_platform_binding(&binding).await?;
                        Ok(new_user.internal_id)
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }
}
