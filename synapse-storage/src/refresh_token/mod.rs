use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token_id: Option<String>,
    pub scope: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub use_count: i32,
    pub is_revoked: bool,
    pub revoked_reason: Option<String>,
    pub client_info: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenUsage {
    pub id: i64,
    pub refresh_token_id: i64,
    pub user_id: String,
    pub old_access_token_id: Option<String>,
    pub new_access_token_id: Option<String>,
    pub used_ts: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenFamily {
    pub id: i64,
    pub family_id: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub last_refresh_ts: Option<i64>,
    pub refresh_count: i32,
    pub is_compromised: bool,
    pub compromised_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenRotation {
    pub id: i64,
    pub family_id: String,
    pub old_token_hash: Option<String>,
    pub new_token_hash: String,
    pub rotated_ts: i64,
    pub rotation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenBlacklistEntry {
    pub id: i64,
    pub token_hash: String,
    pub token_type: Option<String>,
    pub user_id: Option<String>,
    pub is_revoked: bool,
    pub expires_at: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRefreshTokenRequest {
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token_id: Option<String>,
    pub scope: Option<String>,
    pub expires_at: i64,
    pub client_info: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateRefreshTokenRequest {
    pub old_token_hash: String,
    pub new_token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub family_id: Option<String>,
    pub expires_at: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordUsageRequest {
    pub refresh_token_id: i64,
    pub user_id: String,
    pub old_access_token_id: Option<String>,
    pub new_access_token_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    #[serde(rename = "success")]
    pub is_success: bool,
    pub error_message: Option<String>,
}

impl RecordUsageRequest {
    pub fn new(
        refresh_token_id: i64,
        user_id: impl Into<String>,
        new_access_token_id: impl Into<String>,
        is_success: bool,
    ) -> Self {
        Self {
            refresh_token_id,
            user_id: user_id.into(),
            new_access_token_id: new_access_token_id.into(),
            is_success,
            ..Default::default()
        }
    }

    pub fn old_access_token_id(mut self, old_access_token_id: impl Into<String>) -> Self {
        self.old_access_token_id = Some(old_access_token_id.into());
        self
    }

    pub fn ip_address(mut self, ip_address: impl Into<String>) -> Self {
        self.ip_address = Some(ip_address.into());
        self
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn error_message(mut self, error_message: impl Into<String>) -> Self {
        self.error_message = Some(error_message.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenStats {
    pub user_id: String,
    pub total_tokens: i64,
    pub active_tokens: i64,
    pub revoked_tokens: i64,
    pub expired_tokens: i64,
    pub total_uses: i64,
}

#[async_trait]
pub trait RefreshTokenStoreApi: Send + Sync {
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error>;
    async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error>;
    async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error>;

    async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error>;
    async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error>;
    async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error>;
    async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error>;
    async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error>;
    async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error>;
    async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error>;
    async fn record_usage(&self, request: &RecordUsageRequest) -> Result<(), sqlx::Error>;
    async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error>;
    async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error>;
    async fn record_rotation(
        &self,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error>;
    async fn get_rotations(&self, family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error>;
    async fn add_to_blacklist(
        &self,
        token_hash: &str,
        token_type: &str,
        user_id: &str,
        expires_at: i64,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error>;
    async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error>;
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error>;
    async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error>;
    async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error>;
    async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error>;

    async fn revoke_device_tokens(&self, user_id: &str, device_id: &str, reason: &str) -> Result<i64, sqlx::Error>;

    async fn revoke_all_user_tokens_except_device(
        &self,
        user_id: &str,
        device_id: &str,
        reason: &str,
    ) -> Result<i64, sqlx::Error>;
}

#[derive(Clone)]
pub struct RefreshTokenStorage {
    pool: Arc<PgPool>,
}

impl RefreshTokenStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as!(
            RefreshToken,
            r#"
            INSERT INTO refresh_tokens (
                token_hash, user_id, device_id, access_token_id, scope, created_ts,
                expires_at, client_info, ip_address, user_agent
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING
                id as "id!",
                token_hash as "token_hash!",
                user_id as "user_id!",
                device_id as "device_id?",
                access_token_id as "access_token_id?",
                scope as "scope?",
                created_ts as "created_ts!",
                expires_at as "expires_at?",
                last_used_ts as "last_used_ts?",
                COALESCE(use_count, 0) as "use_count!",
                COALESCE(is_revoked, false) as "is_revoked!",
                revoked_reason as "revoked_reason?",
                client_info as "client_info?",
                ip_address as "ip_address?",
                user_agent as "user_agent?"
            "#,
            &request.token_hash,
            &request.user_id,
            request.device_id.as_deref(),
            request.access_token_id.as_deref(),
            request.scope.as_deref(),
            now,
            request.expires_at,
            request.client_info.as_ref(),
            request.ip_address.as_deref(),
            request.user_agent.as_deref()
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query_as!(
            RefreshToken,
            r#"
            SELECT
                id as "id!",
                token_hash as "token_hash!",
                user_id as "user_id!",
                device_id as "device_id?",
                access_token_id as "access_token_id?",
                scope as "scope?",
                created_ts as "created_ts!",
                expires_at as "expires_at?",
                last_used_ts as "last_used_ts?",
                COALESCE(use_count, 0) as "use_count!",
                COALESCE(is_revoked, false) as "is_revoked!",
                revoked_reason as "revoked_reason?",
                client_info as "client_info?",
                ip_address as "ip_address?",
                user_agent as "user_agent?"
            FROM refresh_tokens WHERE token_hash = $1
            "#,
            token_hash
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query_as!(
            RefreshToken,
            r#"
            SELECT
                id as "id!",
                token_hash as "token_hash!",
                user_id as "user_id!",
                device_id as "device_id?",
                access_token_id as "access_token_id?",
                scope as "scope?",
                created_ts as "created_ts!",
                expires_at as "expires_at?",
                last_used_ts as "last_used_ts?",
                COALESCE(use_count, 0) as "use_count!",
                COALESCE(is_revoked, false) as "is_revoked!",
                revoked_reason as "revoked_reason?",
                client_info as "client_info?",
                ip_address as "ip_address?",
                user_agent as "user_agent?"
            FROM refresh_tokens WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RefreshToken,
            r#"
            SELECT
                id as "id!",
                token_hash as "token_hash!",
                user_id as "user_id!",
                device_id as "device_id?",
                access_token_id as "access_token_id?",
                scope as "scope?",
                created_ts as "created_ts!",
                expires_at as "expires_at?",
                last_used_ts as "last_used_ts?",
                COALESCE(use_count, 0) as "use_count!",
                COALESCE(is_revoked, false) as "is_revoked!",
                revoked_reason as "revoked_reason?",
                client_info as "client_info?",
                ip_address as "ip_address?",
                user_agent as "user_agent?"
            FROM refresh_tokens WHERE user_id = $1 ORDER BY created_ts DESC
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let rows = sqlx::query_as!(
            RefreshToken,
            r#"
            SELECT
                id as "id!",
                token_hash as "token_hash!",
                user_id as "user_id!",
                device_id as "device_id?",
                access_token_id as "access_token_id?",
                scope as "scope?",
                created_ts as "created_ts!",
                expires_at as "expires_at?",
                last_used_ts as "last_used_ts?",
                COALESCE(use_count, 0) as "use_count!",
                COALESCE(is_revoked, false) as "is_revoked!",
                revoked_reason as "revoked_reason?",
                client_info as "client_info?",
                ip_address as "ip_address?",
                user_agent as "user_agent?"
            FROM refresh_tokens
            WHERE user_id = $1
            AND is_revoked = FALSE
            AND expires_at > $2
            ORDER BY created_ts DESC
            "#,
            user_id,
            now
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_reason = $2
            WHERE token_hash = $1
            "#,
            token_hash,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_reason = $2
            WHERE token_hash = $1 AND is_revoked = FALSE
            "#,
            token_hash,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_reason = $2
            WHERE id = $1
            "#,
            id,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_reason = $2
            WHERE user_id = $1 AND is_revoked = FALSE
            "#,
            user_id,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn revoke_all_user_tokens_except_device(
        &self,
        user_id: &str,
        device_id: &str,
        reason: &str,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_reason = $3
            WHERE user_id = $1 AND device_id != $2 AND is_revoked = FALSE
            "#,
            user_id,
            device_id,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    /// 吊销某用户在指定设备上的全部 refresh token。
    ///
    /// 单设备登出时调用：仅清掉该设备的令牌族，不影响用户在其他设备的会话。
    pub async fn revoke_device_tokens(&self, user_id: &str, device_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_reason = $3
            WHERE user_id = $1 AND device_id = $2 AND is_revoked = FALSE
            "#,
            user_id,
            device_id,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn update_token_usage(&self, token_hash: &str, access_token_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE refresh_tokens SET
                access_token_id = $2,
                last_used_ts = $3,
                use_count = use_count + 1
            WHERE token_hash = $1
            "#,
            token_hash,
            access_token_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn record_usage(&self, request: &RecordUsageRequest) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            INSERT INTO refresh_token_usage (
                refresh_token_id, user_id, old_access_token_id, new_access_token_id,
                used_ts, ip_address, user_agent, is_success, error_message
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            request.refresh_token_id,
            &request.user_id,
            request.old_access_token_id.as_deref(),
            &request.new_access_token_id,
            now,
            request.ip_address.as_deref(),
            request.user_agent.as_deref(),
            request.is_success,
            request.error_message.as_deref()
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as!(
            RefreshTokenFamily,
            r#"
            INSERT INTO refresh_token_families (family_id, user_id, device_id, created_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING
                id as "id!",
                family_id as "family_id!",
                user_id as "user_id!",
                device_id as "device_id?",
                created_ts as "created_ts!",
                last_refresh_ts as "last_refresh_ts?",
                COALESCE(refresh_count, 0) as "refresh_count!",
                COALESCE(is_compromised, false) as "is_compromised!",
                compromised_at as "compromised_ts?"
            "#,
            family_id,
            user_id,
            device_id,
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_family(&self, family_id: &str) -> Result<Option<RefreshTokenFamily>, sqlx::Error> {
        let row = sqlx::query_as!(
            RefreshTokenFamily,
            r#"
            SELECT
                id as "id!",
                family_id as "family_id!",
                user_id as "user_id!",
                device_id as "device_id?",
                created_ts as "created_ts!",
                last_refresh_ts as "last_refresh_ts?",
                COALESCE(refresh_count, 0) as "refresh_count!",
                COALESCE(is_compromised, false) as "is_compromised!",
                compromised_at as "compromised_ts?"
            FROM refresh_token_families WHERE family_id = $1
            "#,
            family_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE refresh_token_families SET
                is_compromised = TRUE,
                compromised_at = $2
            WHERE family_id = $1
            "#,
            family_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn record_rotation(
        &self,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            INSERT INTO refresh_token_rotations (family_id, old_token_hash, new_token_hash, rotated_ts, rotation_reason)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            family_id,
            old_token_hash,
            new_token_hash,
            now,
            reason
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            UPDATE refresh_token_families SET
                last_refresh_ts = $2,
                refresh_count = refresh_count + 1
            WHERE family_id = $1
            "#,
            family_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_rotations(&self, family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RefreshTokenRotation,
            r#"
            SELECT
                id as "id!",
                family_id as "family_id!",
                old_token_hash as "old_token_hash?",
                new_token_hash as "new_token_hash!",
                rotated_ts as "rotated_ts!",
                rotation_reason as "rotation_reason?"
            FROM refresh_token_rotations WHERE family_id = $1 ORDER BY rotated_ts DESC
            "#,
            family_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn add_to_blacklist(
        &self,
        token_hash: &str,
        token_type: &str,
        user_id: &str,
        expires_at: i64,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO token_blacklist (token_hash, token, token_type, user_id, is_revoked, expires_at, reason)
            VALUES ($1, NULL, $2, $3, TRUE, $4, $5)
            ON CONFLICT (token_hash) DO NOTHING
            "#,
            token_hash,
            token_type,
            user_id,
            expires_at,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) as "count!" FROM token_blacklist WHERE token_hash = $1 AND expires_at > $2
            "#,
            token_hash,
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(result > 0)
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query!(
            r#"
            DELETE FROM refresh_tokens WHERE expires_at < $1 AND is_revoked = FALSE
            "#,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query!(
            r#"
            DELETE FROM token_blacklist WHERE expires_at < $1
            "#,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error> {
        let row = sqlx::query_as!(
            RefreshTokenStats,
            r#"
            SELECT
                user_id as "user_id!",
                COUNT(*) as "total_tokens!",
                COUNT(*) FILTER (WHERE is_revoked = FALSE AND expires_at > EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) as "active_tokens!",
                COUNT(*) FILTER (WHERE is_revoked = TRUE) as "revoked_tokens!",
                COUNT(*) FILTER (WHERE expires_at <= EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) as "expired_tokens!",
                COALESCE(SUM(use_count), 0) as "total_uses!"
            FROM refresh_tokens
            WHERE user_id = $1
            GROUP BY user_id
            "#,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RefreshTokenUsage,
            r#"
            SELECT
                id as "id!",
                refresh_token_id as "refresh_token_id!",
                user_id as "user_id!",
                old_access_token_id as "old_access_token_id?",
                new_access_token_id as "new_access_token_id?",
                used_ts as "used_ts!",
                ip_address as "ip_address?",
                user_agent as "user_agent?",
                COALESCE(is_success, false) as "is_success!",
                error_message as "error_message?"
            FROM refresh_token_usage WHERE user_id = $1 ORDER BY used_ts DESC LIMIT $2
            "#,
            user_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM refresh_tokens WHERE token_hash = $1
            "#,
            token_hash
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM refresh_tokens WHERE user_id = $1
            "#,
            user_id
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }
}

#[async_trait]
impl RefreshTokenStoreApi for RefreshTokenStorage {
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        self.get_user_tokens(user_id).await
    }

    async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error> {
        self.get_token_by_id(id).await
    }

    async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error> {
        self.delete_token(token_hash).await
    }

    async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error> {
        self.create_token(request).await
    }

    async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        self.get_token(token_hash).await
    }

    async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        self.get_active_tokens(user_id).await
    }

    async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error> {
        self.revoke_token(token_hash, reason).await
    }

    async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error> {
        self.revoke_token_cas(token_hash, reason).await
    }

    async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error> {
        self.revoke_token_by_id(id, reason).await
    }

    async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        self.revoke_all_user_tokens(user_id, reason).await
    }

    async fn record_usage(&self, request: &RecordUsageRequest) -> Result<(), sqlx::Error> {
        self.record_usage(request).await
    }

    async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error> {
        self.create_family(family_id, user_id, device_id).await
    }

    async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error> {
        self.mark_family_compromised(family_id).await
    }

    async fn record_rotation(
        &self,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        self.record_rotation(family_id, old_token_hash, new_token_hash, reason).await
    }

    async fn get_rotations(&self, family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error> {
        self.get_rotations(family_id).await
    }

    async fn add_to_blacklist(
        &self,
        token_hash: &str,
        token_type: &str,
        user_id: &str,
        expires_at: i64,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.add_to_blacklist(token_hash, token_type, user_id, expires_at, reason).await
    }

    async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        self.is_blacklisted(token_hash).await
    }

    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        self.cleanup_expired_tokens().await
    }

    async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error> {
        self.cleanup_blacklist().await
    }

    async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error> {
        self.get_user_stats(user_id).await
    }

    async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error> {
        self.get_usage_history(user_id, limit).await
    }

    async fn revoke_device_tokens(&self, user_id: &str, device_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        self.revoke_device_tokens(user_id, device_id, reason).await
    }

    async fn revoke_all_user_tokens_except_device(
        &self,
        user_id: &str,
        device_id: &str,
        reason: &str,
    ) -> Result<i64, sqlx::Error> {
        self.revoke_all_user_tokens_except_device(user_id, device_id, reason).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_usage_request_new() {
        let req = RecordUsageRequest::new(1, "@alice:example.com", "acc_token_123", true);

        assert_eq!(req.refresh_token_id, 1);
        assert_eq!(req.user_id, "@alice:example.com");
        assert_eq!(req.new_access_token_id, "acc_token_123");
        assert!(req.is_success);
        assert!(req.old_access_token_id.is_none());
        assert!(req.ip_address.is_none());
        assert!(req.user_agent.is_none());
        assert!(req.error_message.is_none());
    }

    #[test]
    fn test_record_usage_request_builder_chain() {
        let req = RecordUsageRequest::new(2, "@bob:example.com", "acc_token_456", false)
            .old_access_token_id("old_token")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .error_message("Token expired");

        assert_eq!(req.refresh_token_id, 2);
        assert!(!req.is_success);
        assert_eq!(req.old_access_token_id.as_deref(), Some("old_token"));
        assert_eq!(req.ip_address.as_deref(), Some("192.168.1.1"));
        assert_eq!(req.user_agent.as_deref(), Some("Mozilla/5.0"));
        assert_eq!(req.error_message.as_deref(), Some("Token expired"));
    }

    #[test]
    fn test_refresh_token_struct() {
        let token = RefreshToken {
            id: 1,
            token_hash: "hash123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            access_token_id: Some("acc123".to_string()),
            scope: None,
            created_ts: 1700000000000,
            expires_at: None,
            last_used_ts: Some(1700000001000),
            use_count: 5,
            is_revoked: false,
            revoked_reason: None,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        assert_eq!(token.id, 1);
        assert_eq!(token.user_id, "@alice:example.com");
        assert!(!token.is_revoked);
        assert_eq!(token.device_id.as_deref(), Some("DEVICE123"));
        assert_eq!(token.use_count, 5);
    }

    #[test]
    fn test_refresh_token_serde() {
        let token = RefreshToken {
            id: 2,
            token_hash: "hash456".to_string(),
            user_id: "@charlie:example.com".to_string(),
            device_id: None,
            access_token_id: None,
            scope: Some("read".to_string()),
            created_ts: 1700000000000,
            expires_at: Some(1800000000000),
            last_used_ts: None,
            use_count: 0,
            is_revoked: true,
            revoked_reason: Some("logout".to_string()),
            client_info: None,
            ip_address: None,
            user_agent: None,
        };

        let json = serde_json::to_string(&token).unwrap();
        let deserialized: RefreshToken = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, token.id);
        assert_eq!(deserialized.is_revoked, token.is_revoked);
        assert_eq!(deserialized.use_count, token.use_count);
    }

    #[test]
    fn test_token_blacklist_entry() {
        let entry = TokenBlacklistEntry {
            id: 1,
            token_hash: "hash_blacklisted".to_string(),
            token_type: Some("refresh".to_string()),
            user_id: Some("@dave:example.com".to_string()),
            is_revoked: true,
            expires_at: None,
            reason: Some("security".to_string()),
        };
        assert_eq!(entry.token_hash, "hash_blacklisted");
        assert_eq!(entry.reason.as_deref(), Some("security"));
    }

    #[test]
    fn test_refresh_token_stats() {
        let stats = RefreshTokenStats {
            user_id: "@alice:example.com".to_string(),
            total_tokens: 100,
            active_tokens: 80,
            revoked_tokens: 15,
            expired_tokens: 5,
            total_uses: 200,
        };
        assert_eq!(stats.total_tokens, 100);
        assert_eq!(stats.active_tokens, 80);
    }

    #[test]
    fn test_create_refresh_token_request() {
        let req = CreateRefreshTokenRequest {
            token_hash: "hash_new".to_string(),
            user_id: "@eve:example.com".to_string(),
            device_id: Some("DEVICE999".to_string()),
            access_token_id: None,
            scope: None,
            expires_at: 1800000000000,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        assert_eq!(req.token_hash, "hash_new");
        assert_eq!(req.user_id, "@eve:example.com");
        assert_eq!(req.expires_at, 1800000000000);
    }

    #[test]
    fn test_rotate_refresh_token_request() {
        let req = RotateRefreshTokenRequest {
            old_token_hash: "old_hash".to_string(),
            new_token_hash: "new_hash".to_string(),
            user_id: "@frank:example.com".to_string(),
            device_id: None,
            family_id: Some("family_abc".to_string()),
            expires_at: 1800000000000,
            ip_address: None,
            user_agent: None,
        };
        assert_eq!(req.old_token_hash, "old_hash");
        assert_eq!(req.new_token_hash, "new_hash");
        assert_eq!(req.family_id.as_deref(), Some("family_abc"));
    }

    #[test]
    fn test_refresh_token_usage() {
        let usage = RefreshTokenUsage {
            id: 1,
            refresh_token_id: 5,
            user_id: "@grace:example.com".to_string(),
            old_access_token_id: Some("old_acc".to_string()),
            new_access_token_id: Some("new_acc".to_string()),
            used_ts: 1700000000000,
            ip_address: Some("10.0.0.1".to_string()),
            user_agent: Some("App/1.0".to_string()),
            is_success: true,
            error_message: None,
        };
        assert_eq!(usage.refresh_token_id, 5);
        assert!(usage.is_success);
    }

    #[test]
    fn test_refresh_token_family() {
        let family = RefreshTokenFamily {
            id: 1,
            family_id: "family_abc".to_string(),
            user_id: "@henry:example.com".to_string(),
            device_id: Some("DEV123".to_string()),
            created_ts: 1700000000000,
            last_refresh_ts: Some(1700000001000),
            refresh_count: 3,
            is_compromised: false,
            compromised_ts: None,
        };
        assert_eq!(family.family_id, "family_abc");
        assert_eq!(family.refresh_count, 3);
        assert!(!family.is_compromised);
    }

    #[test]
    fn test_refresh_token_rotation() {
        let rotation = RefreshTokenRotation {
            id: 1,
            family_id: "family_abc".to_string(),
            old_token_hash: Some("old_hash".to_string()),
            new_token_hash: "new_hash".to_string(),
            rotated_ts: 1700000000000,
            rotation_reason: Some("refresh".to_string()),
        };
        assert_eq!(rotation.family_id, "family_abc");
        assert_eq!(rotation.new_token_hash, "new_hash");
    }

    // ===== Database-dependent tests =====

    use std::sync::atomic::{AtomicU64, Ordering};

    static RT_TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn rt_unique_suffix() -> u64 {
        RT_TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    async fn setup_refresh_token_db(pool: &Arc<PgPool>) {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS refresh_tokens (
                id BIGSERIAL PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL,
                device_id TEXT,
                access_token_id TEXT,
                scope TEXT,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT,
                last_used_ts BIGINT,
                use_count INTEGER DEFAULT 0,
                is_revoked BOOLEAN DEFAULT FALSE,
                revoked_reason TEXT,
                client_info JSONB,
                ip_address TEXT,
                user_agent TEXT
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create refresh_tokens table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS refresh_token_usage (
                id BIGSERIAL PRIMARY KEY,
                refresh_token_id BIGINT NOT NULL,
                user_id TEXT NOT NULL,
                old_access_token_id TEXT,
                new_access_token_id TEXT,
                used_ts BIGINT NOT NULL,
                ip_address TEXT,
                user_agent TEXT,
                is_success BOOLEAN DEFAULT TRUE,
                error_message TEXT
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create refresh_token_usage table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS refresh_token_families (
                id BIGSERIAL PRIMARY KEY,
                family_id TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL,
                device_id TEXT,
                created_ts BIGINT NOT NULL,
                last_refresh_ts BIGINT,
                refresh_count INTEGER DEFAULT 0,
                is_compromised BOOLEAN DEFAULT FALSE,
                compromised_at BIGINT
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create refresh_token_families table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS refresh_token_rotations (
                id BIGSERIAL PRIMARY KEY,
                family_id TEXT NOT NULL,
                old_token_hash TEXT,
                new_token_hash TEXT NOT NULL,
                rotated_ts BIGINT NOT NULL,
                rotation_reason TEXT
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create refresh_token_rotations table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS token_blacklist (
                id BIGSERIAL PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                token TEXT,
                token_type TEXT DEFAULT 'access',
                user_id TEXT,
                is_revoked BOOLEAN DEFAULT TRUE,
                reason TEXT,
                expires_at BIGINT
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create token_blacklist table");
    }

    fn make_create_request(suffix: u64, user_id: &str, expires_at: i64) -> CreateRefreshTokenRequest {
        CreateRefreshTokenRequest {
            token_hash: format!("hash_{suffix}"),
            user_id: user_id.to_string(),
            device_id: Some(format!("device_{suffix}")),
            access_token_id: Some(format!("atid_{suffix}")),
            scope: Some("openid".to_string()),
            expires_at,
            client_info: None,
            ip_address: None,
            user_agent: None,
        }
    }

    async fn get_rt_test_pool() -> Option<Arc<PgPool>> {
        match crate::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                tracing::warn!("Skipping refresh_token DB test because test database is unavailable: {error}");
                None
            }
        }
    }

    #[tokio::test]
    async fn test_db_create_token_success() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");

        assert!(token.id > 0);
        assert_eq!(token.token_hash, format!("hash_{suffix}"));
        assert_eq!(token.user_id, user_id);
        assert_eq!(token.device_id, Some(format!("device_{suffix}")));
        assert_eq!(token.scope, Some("openid".to_string()));
        assert!(!token.is_revoked);
        assert_eq!(token.use_count, 0);
        assert!(token.created_ts > 0);
        assert_eq!(token.expires_at, Some(future_ts));
    }

    #[tokio::test]
    async fn test_db_get_token_found_and_missing() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let created = storage.create_token(request).await.expect("Failed to create token");

        let found = storage.get_token(&format!("hash_{suffix}")).await.expect("Failed to get token");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.token_hash, created.token_hash);

        let missing = storage.get_token("nonexistent_hash").await.expect("Failed to query missing token");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_db_get_token_by_id_found_and_missing() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let created = storage.create_token(request).await.expect("Failed to create token");

        let found = storage.get_token_by_id(created.id).await.expect("Failed to get token by id");
        assert!(found.is_some());
        assert_eq!(found.unwrap().token_hash, format!("hash_{suffix}"));

        let missing = storage.get_token_by_id(999_999).await.expect("Failed to query missing token");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_db_get_user_tokens_multiple() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3u64 {
            let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
            req.token_hash = format!("hash_{suffix}_{i}");
            storage.create_token(req).await.expect("Failed to create token");
        }

        let tokens = storage.get_user_tokens(&user_id).await.expect("Failed to get user tokens");
        assert_eq!(tokens.len(), 3);
        // ORDER BY created_ts DESC — all created within the same millisecond typically,
        // so just verify we got all three.
        for t in &tokens {
            assert_eq!(t.user_id, user_id);
        }
    }

    #[tokio::test]
    async fn test_db_get_user_tokens_empty() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let tokens = storage.get_user_tokens("@nobody:localhost").await.expect("Failed to get user tokens");
        assert!(tokens.is_empty());
    }

    #[tokio::test]
    async fn test_db_get_active_tokens_filters() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();
        let future_ts = now + 3_600_000;
        let past_ts = now - 3_600_000;

        // Active token (not revoked, future expiry)
        let mut req_active = make_create_request(suffix, &user_id, future_ts);
        req_active.token_hash = format!("active_{suffix}");
        storage.create_token(req_active).await.expect("Failed to create active token");

        // Revoked token
        let mut req_revoked = make_create_request(suffix + 1, &user_id, future_ts);
        req_revoked.token_hash = format!("revoked_{suffix}");
        let revoked_token = storage.create_token(req_revoked).await.expect("Failed to create revoked token");
        storage.revoke_token(&revoked_token.token_hash, "logout").await.expect("Failed to revoke");

        // Expired token
        let mut req_expired = make_create_request(suffix + 2, &user_id, past_ts);
        req_expired.token_hash = format!("expired_{suffix}");
        storage.create_token(req_expired).await.expect("Failed to create expired token");

        let active = storage.get_active_tokens(&user_id).await.expect("Failed to get active tokens");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].token_hash, format!("active_{suffix}"));
    }

    #[tokio::test]
    async fn test_db_revoke_token() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");
        assert!(!token.is_revoked);

        storage.revoke_token(&token.token_hash, "user_logout").await.expect("Failed to revoke token");

        let after = storage.get_token(&token.token_hash).await.expect("Failed to get token").unwrap();
        assert!(after.is_revoked);
        assert_eq!(after.revoked_reason.as_deref(), Some("user_logout"));
    }

    #[tokio::test]
    async fn test_db_revoke_token_cas() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");

        // First CAS revoke on active token should succeed
        let first = storage.revoke_token_cas(&token.token_hash, "revoke_1").await.expect("Failed CAS revoke");
        assert!(first);

        // Second CAS revoke on already-revoked token should fail
        let second = storage.revoke_token_cas(&token.token_hash, "revoke_2").await.expect("Failed CAS revoke");
        assert!(!second);

        // CAS revoke on non-existent token should fail
        let missing = storage.revoke_token_cas("nonexistent", "revoke_3").await.expect("Failed CAS revoke");
        assert!(!missing);
    }

    #[tokio::test]
    async fn test_db_revoke_token_by_id() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");
        storage.revoke_token_by_id(token.id, "breach").await.expect("Failed to revoke by id");

        let after = storage.get_token_by_id(token.id).await.expect("Failed to get token").unwrap();
        assert!(after.is_revoked);
        assert_eq!(after.revoked_reason.as_deref(), Some("breach"));
    }

    #[tokio::test]
    async fn test_db_revoke_all_user_tokens() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3u64 {
            let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
            req.token_hash = format!("hash_{suffix}_{i}");
            storage.create_token(req).await.expect("Failed to create token");
        }

        let count = storage.revoke_all_user_tokens(&user_id, "global_logout").await.expect("Failed to revoke all");
        assert_eq!(count, 3);

        // Second call should revoke 0 (already revoked)
        let count2 = storage.revoke_all_user_tokens(&user_id, "global_logout").await.expect("Failed to revoke all");
        assert_eq!(count2, 0);
    }

    #[tokio::test]
    async fn test_db_revoke_all_user_tokens_except_device() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        // Create tokens on two different devices
        for device in ["keep_device", "revoke_device"] {
            for i in 0..2u64 {
                let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
                req.token_hash = format!("hash_{suffix}_{device}_{i}");
                req.device_id = Some(device.to_string());
                storage.create_token(req).await.expect("Failed to create token");
            }
        }

        let count = storage
            .revoke_all_user_tokens_except_device(&user_id, "keep_device", "remote_wipe")
            .await
            .expect("Failed to revoke except device");
        assert_eq!(count, 2);

        // Verify kept device tokens are still active
        let active = storage.get_active_tokens(&user_id).await.expect("Failed to get active tokens");
        assert_eq!(active.len(), 2);
        for t in &active {
            assert_eq!(t.device_id.as_deref(), Some("keep_device"));
        }
    }

    #[tokio::test]
    async fn test_db_revoke_device_tokens() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for device in ["device_a", "device_b"] {
            let mut req = make_create_request(suffix, &user_id, future_ts);
            req.token_hash = format!("hash_{suffix}_{device}");
            req.device_id = Some(device.to_string());
            storage.create_token(req).await.expect("Failed to create token");
        }

        let count = storage
            .revoke_device_tokens(&user_id, "device_a", "single_logout")
            .await
            .expect("Failed to revoke device tokens");
        assert_eq!(count, 1);

        let active = storage.get_active_tokens(&user_id).await.expect("Failed to get active tokens");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].device_id.as_deref(), Some("device_b"));
    }

    #[tokio::test]
    async fn test_db_update_token_usage() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");
        assert_eq!(token.use_count, 0);
        assert!(token.last_used_ts.is_none());

        storage.update_token_usage(&token.token_hash, "new_access_1").await.expect("Failed to update usage");
        storage.update_token_usage(&token.token_hash, "new_access_2").await.expect("Failed to update usage");

        let after = storage.get_token(&token.token_hash).await.expect("Failed to get token").unwrap();
        assert_eq!(after.use_count, 2);
        assert!(after.last_used_ts.is_some());
        assert_eq!(after.access_token_id.as_deref(), Some("new_access_2"));
    }

    #[tokio::test]
    async fn test_db_record_usage_and_history() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");

        let success_req = RecordUsageRequest::new(token.id, &user_id, "new_at_1", true)
            .old_access_token_id("old_at_1")
            .ip_address("10.0.0.1")
            .user_agent("Agent/1.0");
        storage.record_usage(&success_req).await.expect("Failed to record success usage");

        let fail_req = RecordUsageRequest::new(token.id, &user_id, "new_at_2", false).error_message("invalid_token");
        storage.record_usage(&fail_req).await.expect("Failed to record failure usage");

        let history = storage.get_usage_history(&user_id, 10).await.expect("Failed to get usage history");
        assert_eq!(history.len(), 2);
        // ORDER BY used_ts DESC — most recent first
        assert!(!history[0].is_success);
        assert_eq!(history[0].error_message.as_deref(), Some("invalid_token"));
        assert!(history[1].is_success);
    }

    #[tokio::test]
    async fn test_db_create_and_get_family() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let family_id = format!("family_{suffix}");
        let user_id = format!("@user_{suffix}:localhost");

        let family = storage.create_family(&family_id, &user_id, Some("dev_1")).await.expect("Failed to create family");
        assert!(family.id > 0);
        assert_eq!(family.family_id, family_id);
        assert_eq!(family.user_id, user_id);
        assert_eq!(family.device_id.as_deref(), Some("dev_1"));
        assert_eq!(family.refresh_count, 0);
        assert!(!family.is_compromised);

        let found = storage.get_family(&family_id).await.expect("Failed to get family");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, family.id);

        let missing = storage.get_family("nonexistent_family").await.expect("Failed to get family");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_db_mark_family_compromised() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let family_id = format!("family_{suffix}");
        let user_id = format!("@user_{suffix}:localhost");

        storage.create_family(&family_id, &user_id, None).await.expect("Failed to create family");
        storage.mark_family_compromised(&family_id).await.expect("Failed to mark compromised");

        let after = storage.get_family(&family_id).await.expect("Failed to get family").unwrap();
        assert!(after.is_compromised);
        assert!(after.compromised_ts.is_some());
    }

    #[tokio::test]
    async fn test_db_record_rotation_and_get_rotations() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let family_id = format!("family_{suffix}");
        let user_id = format!("@user_{suffix}:localhost");

        storage.create_family(&family_id, &user_id, None).await.expect("Failed to create family");

        storage
            .record_rotation(&family_id, Some("old_hash_1"), "new_hash_1", "refresh")
            .await
            .expect("Failed to record rotation 1");
        storage
            .record_rotation(&family_id, Some("new_hash_1"), "new_hash_2", "refresh")
            .await
            .expect("Failed to record rotation 2");

        let rotations = storage.get_rotations(&family_id).await.expect("Failed to get rotations");
        assert_eq!(rotations.len(), 2);
        // ORDER BY rotated_ts DESC — most recent first
        assert_eq!(rotations[0].new_token_hash, "new_hash_2");
        assert_eq!(rotations[1].new_token_hash, "new_hash_1");

        // Verify family was updated
        let family = storage.get_family(&family_id).await.expect("Failed to get family").unwrap();
        assert_eq!(family.refresh_count, 2);
        assert!(family.last_refresh_ts.is_some());
    }

    #[tokio::test]
    async fn test_db_add_to_blacklist_and_check() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        storage
            .add_to_blacklist("bl_hash_1", "refresh", "@user:localhost", future_ts, Some("compromised"))
            .await
            .expect("Failed to add to blacklist");

        let is_blacklisted = storage.is_blacklisted("bl_hash_1").await.expect("Failed to check blacklist");
        assert!(is_blacklisted);

        let not_listed = storage.is_blacklisted("not_blacklisted").await.expect("Failed to check blacklist");
        assert!(!not_listed);
    }

    #[tokio::test]
    async fn test_db_is_blacklisted_expired_entry() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let past_ts = chrono::Utc::now().timestamp_millis() - 3_600_000;

        storage
            .add_to_blacklist("expired_bl", "refresh", "@user:localhost", past_ts, Some("old"))
            .await
            .expect("Failed to add expired entry");

        let is_blacklisted = storage.is_blacklisted("expired_bl").await.expect("Failed to check blacklist");
        assert!(!is_blacklisted);
    }

    #[tokio::test]
    async fn test_db_add_to_blacklist_idempotent() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        storage
            .add_to_blacklist("dup_hash", "refresh", "@user:localhost", future_ts, Some("first"))
            .await
            .expect("Failed first insert");
        // ON CONFLICT DO NOTHING — second insert should not error
        storage
            .add_to_blacklist("dup_hash", "refresh", "@user:localhost", future_ts, Some("second"))
            .await
            .expect("Failed second insert");

        let is_blacklisted = storage.is_blacklisted("dup_hash").await.expect("Failed to check blacklist");
        assert!(is_blacklisted);
    }

    #[tokio::test]
    async fn test_db_cleanup_expired_tokens() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();
        let past_ts = now - 3_600_000;
        let future_ts = now + 3_600_000;

        // Expired + not revoked → should be deleted
        let mut req_expired = make_create_request(suffix, &user_id, past_ts);
        req_expired.token_hash = format!("expired_{suffix}");
        storage.create_token(req_expired).await.expect("Failed to create expired token");

        // Expired + revoked → should NOT be deleted (query has AND is_revoked = FALSE)
        let mut req_revoked = make_create_request(suffix + 1, &user_id, past_ts);
        req_revoked.token_hash = format!("revoked_expired_{suffix}");
        let revoked_token = storage.create_token(req_revoked).await.expect("Failed to create revoked token");
        storage.revoke_token(&revoked_token.token_hash, "revoke").await.expect("Failed to revoke");

        // Active → should NOT be deleted
        let mut req_active = make_create_request(suffix + 2, &user_id, future_ts);
        req_active.token_hash = format!("active_{suffix}");
        storage.create_token(req_active).await.expect("Failed to create active token");

        let deleted = storage.cleanup_expired_tokens().await.expect("Failed to cleanup");
        assert_eq!(deleted, 1);

        let remaining = storage.get_user_tokens(&user_id).await.expect("Failed to get tokens");
        assert_eq!(remaining.len(), 2);
    }

    #[tokio::test]
    async fn test_db_cleanup_blacklist() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        let past_ts = now - 3_600_000;
        let future_ts = now + 3_600_000;

        storage
            .add_to_blacklist("expired_entry", "refresh", "@u:localhost", past_ts, Some("old"))
            .await
            .expect("Failed to add expired");
        storage
            .add_to_blacklist("active_entry", "refresh", "@u:localhost", future_ts, Some("active"))
            .await
            .expect("Failed to add active");

        let deleted = storage.cleanup_blacklist().await.expect("Failed to cleanup blacklist");
        assert_eq!(deleted, 1);

        assert!(!storage.is_blacklisted("expired_entry").await.expect("Failed to check"));
        assert!(storage.is_blacklisted("active_entry").await.expect("Failed to check"));
    }

    #[tokio::test]
    async fn test_db_get_user_stats() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();
        let future_ts = now + 3_600_000;
        let past_ts = now - 3_600_000;

        // Active token with use_count 5
        let mut req_active = make_create_request(suffix, &user_id, future_ts);
        req_active.token_hash = format!("active_{suffix}");
        let active = storage.create_token(req_active).await.expect("Failed to create active");
        storage.update_token_usage(&active.token_hash, "at_1").await.expect("Failed to update");
        for _ in 0..4 {
            storage.update_token_usage(&active.token_hash, "at_1").await.expect("Failed to update");
        }

        // Revoked token
        let mut req_revoked = make_create_request(suffix + 1, &user_id, future_ts);
        req_revoked.token_hash = format!("revoked_{suffix}");
        let revoked = storage.create_token(req_revoked).await.expect("Failed to create revoked");
        storage.revoke_token(&revoked.token_hash, "logout").await.expect("Failed to revoke");
        storage.update_token_usage(&revoked.token_hash, "at_2").await.expect("Failed to update");
        for _ in 0..2 {
            storage.update_token_usage(&revoked.token_hash, "at_2").await.expect("Failed to update");
        }

        // Expired token
        let mut req_expired = make_create_request(suffix + 2, &user_id, past_ts);
        req_expired.token_hash = format!("expired_{suffix}");
        let expired = storage.create_token(req_expired).await.expect("Failed to create expired");
        storage.update_token_usage(&expired.token_hash, "at_3").await.expect("Failed to update");
        storage.update_token_usage(&expired.token_hash, "at_3").await.expect("Failed to update");

        let stats = storage.get_user_stats(&user_id).await.expect("Failed to get stats").expect("Stats should exist");
        assert_eq!(stats.user_id, user_id);
        assert_eq!(stats.total_tokens, 3);
        assert_eq!(stats.active_tokens, 1);
        assert_eq!(stats.revoked_tokens, 1);
        assert_eq!(stats.expired_tokens, 1);
        // update_token_usage is unconditional: 5 (active) + 3 (revoked) + 2 (expired) = 10
        assert_eq!(stats.total_uses, 10);
    }

    #[tokio::test]
    async fn test_db_get_user_stats_no_tokens() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let stats = storage.get_user_stats("@nobody:localhost").await.expect("Failed to get stats");
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_db_delete_token() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");
        storage.delete_token(&token.token_hash).await.expect("Failed to delete token");

        let after = storage.get_token(&token.token_hash).await.expect("Failed to get token");
        assert!(after.is_none());

        // Deleting non-existent token should not error
        storage.delete_token("nonexistent").await.expect("Deleting nonexistent should not error");
    }

    #[tokio::test]
    async fn test_db_delete_user_tokens() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;

        for i in 0..3u64 {
            let mut req = make_create_request(suffix * 10 + i, &user_id, future_ts);
            req.token_hash = format!("hash_{suffix}_{i}");
            storage.create_token(req).await.expect("Failed to create token");
        }

        let count = storage.delete_user_tokens(&user_id).await.expect("Failed to delete user tokens");
        assert_eq!(count, 3);

        let tokens = storage.get_user_tokens(&user_id).await.expect("Failed to get tokens");
        assert!(tokens.is_empty());
    }

    #[tokio::test]
    async fn test_db_store_api_trait_impl() {
        let pool = match get_rt_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_refresh_token_db(&pool).await;

        let storage = RefreshTokenStorage::new(&pool);
        let suffix = rt_unique_suffix();
        let user_id = format!("@user_{suffix}:localhost");
        let future_ts = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let request = make_create_request(suffix, &user_id, future_ts);

        let token = storage.create_token(request).await.expect("Failed to create token");

        // Test trait method get_user_tokens
        let tokens: Vec<RefreshToken> =
            RefreshTokenStoreApi::get_user_tokens(&storage, &user_id).await.expect("trait get_user_tokens failed");
        assert_eq!(tokens.len(), 1);

        // Test trait method get_token_by_id
        let found =
            RefreshTokenStoreApi::get_token_by_id(&storage, token.id).await.expect("trait get_token_by_id failed");
        assert!(found.is_some());

        // Test trait method delete_token
        RefreshTokenStoreApi::delete_token(&storage, &token.token_hash).await.expect("trait delete_token failed");
        let after = storage.get_token(&token.token_hash).await.expect("Failed to get token");
        assert!(after.is_none());
    }
}
