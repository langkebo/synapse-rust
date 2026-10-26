use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `RefreshToken` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshToken {
    /// The `id` field.
    pub id: i64,
    /// The `token_hash` field.
    pub token_hash: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `access_token_id` field.
    pub access_token_id: Option<String>,
    /// The `scope` field.
    pub scope: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `last_used_ts` field.
    pub last_used_ts: Option<i64>,
    /// The `use_count` field.
    pub use_count: i32,
    /// The `is_revoked` field.
    pub is_revoked: bool,
    /// The `revoked_reason` field.
    pub revoked_reason: Option<String>,
    /// The `client_info` field.
    pub client_info: Option<serde_json::Value>,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
}

/// The `RefreshTokenUsage` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenUsage {
    /// The `id` field.
    pub id: i64,
    /// The `refresh_token_id` field.
    pub refresh_token_id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `old_access_token_id` field.
    pub old_access_token_id: Option<String>,
    /// The `new_access_token_id` field.
    pub new_access_token_id: Option<String>,
    /// The `used_ts` field.
    pub used_ts: i64,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error_message` field.
    pub error_message: Option<String>,
}

/// The `RefreshTokenFamily` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenFamily {
    /// The `id` field.
    pub id: i64,
    /// The `family_id` field.
    pub family_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `last_refresh_ts` field.
    pub last_refresh_ts: Option<i64>,
    /// The `refresh_count` field.
    pub refresh_count: i32,
    /// The `is_compromised` field.
    pub is_compromised: bool,
    /// The `compromised_ts` field.
    pub compromised_ts: Option<i64>,
}

/// The `RefreshTokenRotation` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenRotation {
    /// The `id` field.
    pub id: i64,
    /// The `family_id` field.
    pub family_id: String,
    /// The `old_token_hash` field.
    pub old_token_hash: Option<String>,
    /// The `new_token_hash` field.
    pub new_token_hash: String,
    /// The `rotated_ts` field.
    pub rotated_ts: i64,
    /// The `rotation_reason` field.
    pub rotation_reason: Option<String>,
}

/// The `TokenBlacklistEntry` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenBlacklistEntry {
    /// The `id` field.
    pub id: i64,
    /// The `token_hash` field.
    pub token_hash: String,
    /// The `token_type` field.
    pub token_type: Option<String>,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `is_revoked` field.
    pub is_revoked: bool,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `reason` field.
    pub reason: Option<String>,
}

/// The `CreateRefreshTokenRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRefreshTokenRequest {
    /// The `token_hash` field.
    pub token_hash: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `access_token_id` field.
    pub access_token_id: Option<String>,
    /// The `scope` field.
    pub scope: Option<String>,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `client_info` field.
    pub client_info: Option<serde_json::Value>,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
}

/// The `RotateRefreshTokenRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateRefreshTokenRequest {
    /// The `old_token_hash` field.
    pub old_token_hash: String,
    /// The `new_token_hash` field.
    pub new_token_hash: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `family_id` field.
    pub family_id: Option<String>,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
}

/// The `RecordUsageRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordUsageRequest {
    /// The `refresh_token_id` field.
    pub refresh_token_id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `old_access_token_id` field.
    pub old_access_token_id: Option<String>,
    /// The `new_access_token_id` field.
    pub new_access_token_id: String,
    /// The `ip_address` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
    #[serde(rename = "success")]
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error_message` field.
    pub error_message: Option<String>,
}

impl RecordUsageRequest {
    /// See [`new`].
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

    /// See [`old_access_token_id`].
    pub fn old_access_token_id(mut self, old_access_token_id: impl Into<String>) -> Self {
        self.old_access_token_id = Some(old_access_token_id.into());
        self
    }

    /// See [`ip_address`].
    pub fn ip_address(mut self, ip_address: impl Into<String>) -> Self {
        self.ip_address = Some(ip_address.into());
        self
    }

    /// See [`user_agent`].
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// See [`error_message`].
    pub fn error_message(mut self, error_message: impl Into<String>) -> Self {
        self.error_message = Some(error_message.into());
        self
    }
}

/// The `RefreshTokenStats` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshTokenStats {
    /// The `user_id` field.
    pub user_id: String,
    /// The `total_tokens` field.
    pub total_tokens: i64,
    /// The `active_tokens` field.
    pub active_tokens: i64,
    /// The `revoked_tokens` field.
    pub revoked_tokens: i64,
    /// The `expired_tokens` field.
    pub expired_tokens: i64,
    /// The `total_uses` field.
    pub total_uses: i64,
}

/// The `RefreshTokenStoreApi` trait.
#[async_trait]
pub trait RefreshTokenStoreApi: Send + Sync {
    /// See [`get_user_tokens`].
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error>;
    /// See [`get_token_by_id`].
    async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error>;
    /// See [`delete_token`].
    async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error>;

    /// See [`create_token`].
    async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error>;
    /// See [`get_token`].
    async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error>;
    /// See [`get_active_tokens`].
    async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error>;
    /// See [`revoke_token`].
    async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error>;
    /// See [`revoke_token_cas`].
    async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error>;
    /// See [`revoke_token_by_id`].
    async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error>;
    /// See [`revoke_all_user_tokens`].
    async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error>;
    /// See [`record_usage`].
    async fn record_usage(&self, request: &RecordUsageRequest) -> Result<(), sqlx::Error>;
    /// See [`create_family`].
    async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error>;
    /// See [`mark_family_compromised`].
    async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error>;
    /// See [`record_rotation`].
    async fn record_rotation(
        &self,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error>;
    /// See [`rotate_token_atomic`].
    ///
    /// Atomic rotation: CAS-revoke the old token, insert the new token, and
    /// record the rotation in one transaction. Returns `Ok(None)` when the old
    /// token was already revoked (CAS miss).
    async fn rotate_token_atomic(
        &self,
        old_token_hash: &str,
        revoke_reason: &str,
        new_token: CreateRefreshTokenRequest,
        family_id: &str,
        rotation_reason: &str,
    ) -> Result<Option<RefreshToken>, sqlx::Error>;
    /// See [`get_rotations`].
    async fn get_rotations(&self, family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error>;

    /// 按 `(user_id, device_id)` 找该设备当前的 token 族 id。
    ///
    /// **为什么需要它**：族是按 `(family_id, user_id, device_id)` 三元建的（见 [`create_family`]），
    /// 所以"设备"才是族的自然键。refresh 的重放检测必须先用它拿到族 id，才能去查
    /// [`get_rotations`]；此前调用方拿 `user_id` 当 `family_id` 传（形态完全不同），
    /// 查询恒 0 行，导致重放检测恒被跳过 —— 见
    /// `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2.5。
    async fn get_family_id(&self, user_id: &str, device_id: Option<&str>) -> Result<Option<String>, sqlx::Error>;
    /// See [`add_to_blacklist`].
    async fn add_to_blacklist(
        &self,
        token_hash: &str,
        token_type: &str,
        user_id: &str,
        expires_at: i64,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error>;
    /// See [`is_blacklisted`].
    async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error>;
    /// See [`cleanup_expired_tokens`].
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error>;
    /// See [`cleanup_blacklist`].
    async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error>;
    /// See [`get_user_stats`].
    async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error>;
    /// See [`get_usage_history`].
    async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error>;

    /// See [`revoke_device_tokens`].
    async fn revoke_device_tokens(&self, user_id: &str, device_id: &str, reason: &str) -> Result<i64, sqlx::Error>;

    /// See [`revoke_all_user_tokens_except_device`].
    async fn revoke_all_user_tokens_except_device(
        &self,
        user_id: &str,
        device_id: &str,
        reason: &str,
    ) -> Result<i64, sqlx::Error>;
}

/// The `RefreshTokenStorage` struct.
#[derive(Clone)]
pub struct RefreshTokenStorage {
    pool: Arc<PgPool>,
}

impl RefreshTokenStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_token`].
    pub async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error> {
        let now = current_timestamp_millis();
        let mut conn = self.pool.acquire().await?;
        Self::insert_refresh_token(&mut conn, &request, now).await
    }

    /// [`create_token`] 的事务版本（PERF-02）。
    ///
    /// 供注册路径在同一个事务里写入刷新令牌，失败时随外层事务回滚。
    pub async fn create_token_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        request: CreateRefreshTokenRequest,
    ) -> Result<RefreshToken, sqlx::Error> {
        let now = current_timestamp_millis();
        Self::insert_refresh_token(tx, &request, now).await
    }

    /// Insert a refresh-token row on a caller-provided connection/transaction.
    ///
    /// Factored out of [`create_token`] so that [`rotate_token_atomic`] can
    /// reuse the identical SQL inside its rotation transaction (CONC-02).
    async fn insert_refresh_token(
        conn: &mut sqlx::PgConnection,
        request: &CreateRefreshTokenRequest,
        now: i64,
    ) -> Result<RefreshToken, sqlx::Error> {
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
        .fetch_one(&mut *conn)
        .await?;

        Ok(row)
    }

    /// See [`get_token`].
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

    /// See [`get_token_by_id`].
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

    /// See [`get_user_tokens`].
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
            FROM refresh_tokens WHERE user_id = $1 ORDER BY created_ts DESC, id DESC
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_active_tokens`].
    pub async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let now = current_timestamp_millis();

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
            ORDER BY created_ts DESC, id DESC
            "#,
            user_id,
            now
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`revoke_token`].
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

    /// See [`revoke_token_cas`].
    pub async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        Self::revoke_token_cas_on(&mut conn, token_hash, reason).await
    }

    /// Compare-and-swap revoke of a refresh token on a caller-provided
    /// connection/transaction. Returns `true` if this call performed the
    /// revocation, `false` if the token was already revoked (or absent).
    ///
    /// Factored out of [`revoke_token_cas`] so that [`rotate_token_atomic`]
    /// can reuse the identical SQL inside its rotation transaction (CONC-02).
    async fn revoke_token_cas_on(
        conn: &mut sqlx::PgConnection,
        token_hash: &str,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
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
        .execute(&mut *conn)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`revoke_token_by_id`].
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

    /// See [`revoke_all_user_tokens`].
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

    /// See [`revoke_all_user_tokens_except_device`].
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

    /// See [`update_token_usage`].
    pub async fn update_token_usage(&self, token_hash: &str, access_token_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

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

    /// See [`record_usage`].
    pub async fn record_usage(&self, request: &RecordUsageRequest) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

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

    /// See [`create_family`].
    pub async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error> {
        let now = current_timestamp_millis();

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

    /// See [`get_family`].
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

    /// See [`mark_family_compromised`].
    pub async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

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

    /// See [`record_rotation`].
    ///
    /// CONC-03: the rotation-log insert and the family counter update run in a
    /// single transaction so they cannot diverge. A rotation row whose family
    /// counter never advanced would desynchronise the next refresh's replay
    /// check (`refresh_token_families` vs `refresh_token_rotations`).
    pub async fn record_rotation(
        &self,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;
        Self::record_rotation_on(&mut tx, family_id, old_token_hash, new_token_hash, reason, now).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Append a rotation-log row and bump the family counters on a
    /// caller-provided connection/transaction.
    ///
    /// Factored out of [`record_rotation`] so that [`rotate_token_atomic`] can
    /// reuse the identical SQL inside its rotation transaction (CONC-02/03).
    async fn record_rotation_on(
        conn: &mut sqlx::PgConnection,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
        now: i64,
    ) -> Result<(), sqlx::Error> {
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
        .execute(&mut *conn)
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
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    /// CONC-02: atomically rotate a refresh token.
    ///
    /// Performs the compare-and-swap revocation of the old token, the insertion
    /// of the new token, and the rotation-log/family-counter update inside a
    /// single transaction. Returns:
    ///   * `Ok(Some(new_token))` when this call won the CAS and rotated, or
    ///   * `Ok(None)` when the old token was already revoked (CAS miss), in
    ///     which case nothing is written and the caller applies its replay /
    ///     benign-race policy.
    pub async fn rotate_token_atomic(
        &self,
        old_token_hash: &str,
        revoke_reason: &str,
        new_token: CreateRefreshTokenRequest,
        family_id: &str,
        rotation_reason: &str,
    ) -> Result<Option<RefreshToken>, sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        if !Self::revoke_token_cas_on(&mut tx, old_token_hash, revoke_reason).await? {
            tx.rollback().await?;
            return Ok(None);
        }

        let row = Self::insert_refresh_token(&mut tx, &new_token, now).await?;
        Self::record_rotation_on(&mut tx, family_id, Some(old_token_hash), &new_token.token_hash, rotation_reason, now)
            .await?;

        tx.commit().await?;
        Ok(Some(row))
    }

    /// See [`get_rotations`].
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
            FROM refresh_token_rotations WHERE family_id = $1 ORDER BY rotated_ts DESC, id DESC
            "#,
            family_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_family_id`].
    ///
    /// `device_id = NULL` 时用 `IS NOT DISTINCT FROM` 匹配，避免 `= NULL` 恒 false
    /// （同一 user 的无设备 token 应当归到同一族）。
    pub async fn get_family_id(&self, user_id: &str, device_id: Option<&str>) -> Result<Option<String>, sqlx::Error> {
        let family_id = sqlx::query_scalar!(
            r#"
            SELECT family_id as "family_id!"
            FROM refresh_token_families
            WHERE user_id = $1
              AND device_id IS NOT DISTINCT FROM $2
            ORDER BY created_ts DESC, id DESC
            LIMIT 1
            "#,
            user_id,
            device_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(family_id)
    }

    /// See [`add_to_blacklist`].
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

    /// See [`is_blacklisted`].
    pub async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        let now = current_timestamp_millis();

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

    /// See [`cleanup_expired_tokens`].
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();

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

    /// See [`cleanup_blacklist`].
    pub async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();

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

    /// See [`get_user_stats`].
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

    /// See [`get_usage_history`].
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
            FROM refresh_token_usage WHERE user_id = $1 ORDER BY used_ts DESC, id DESC LIMIT $2
            "#,
            user_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`delete_token`].
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

    /// See [`delete_user_tokens`].
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

    async fn rotate_token_atomic(
        &self,
        old_token_hash: &str,
        revoke_reason: &str,
        new_token: CreateRefreshTokenRequest,
        family_id: &str,
        rotation_reason: &str,
    ) -> Result<Option<RefreshToken>, sqlx::Error> {
        self.rotate_token_atomic(old_token_hash, revoke_reason, new_token, family_id, rotation_reason).await
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

    async fn get_family_id(&self, user_id: &str, device_id: Option<&str>) -> Result<Option<String>, sqlx::Error> {
        // inherent 方法优先于 trait 方法 ⇒ 转发到上面的实现，不会递归。
        self.get_family_id(user_id, device_id).await
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
mod tests;
