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
