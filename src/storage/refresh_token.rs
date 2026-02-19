use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token_id: Option<String>,
    pub scope: Option<String>,
    pub expires_at: Option<i64>,
    pub created_ts: i64,
    pub last_used_ts: Option<i64>,
    pub use_count: i32,
    pub is_revoked: bool,
    pub revoked_ts: Option<i64>,
    pub revoked_reason: Option<String>,
    pub client_info: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RefreshTokenUsage {
    pub id: i64,
    pub refresh_token_id: i64,
    pub user_id: String,
    pub old_access_token_id: Option<String>,
    pub new_access_token_id: Option<String>,
    pub used_ts: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RefreshTokenRotation {
    pub id: i64,
    pub family_id: String,
    pub old_token_hash: Option<String>,
    pub new_token_hash: String,
    pub rotated_ts: i64,
    pub rotation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TokenBlacklistEntry {
    pub id: i64,
    pub token_hash: String,
    pub token_type: String,
    pub user_id: String,
    pub revoked_ts: i64,
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
    pub success: bool,
    pub error_message: Option<String>,
}

impl RecordUsageRequest {
    pub fn new(refresh_token_id: i64, user_id: impl Into<String>, new_access_token_id: impl Into<String>, success: bool) -> Self {
        Self {
            refresh_token_id,
            user_id: user_id.into(),
            new_access_token_id: new_access_token_id.into(),
            success,
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

        let row = sqlx::query_as::<_, RefreshToken>(
            r#"
            INSERT INTO refresh_tokens (
                token_hash, user_id, device_id, access_token_id, scope, expires_at,
                created_ts, client_info, ip_address, user_agent
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(&request.token_hash)
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(&request.access_token_id)
        .bind(&request.scope)
        .bind(request.expires_at)
        .bind(now)
        .bind(&request.client_info)
        .bind(&request.ip_address)
        .bind(&request.user_agent)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, RefreshToken>(
            "SELECT * FROM refresh_tokens WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, RefreshToken>(
            "SELECT * FROM refresh_tokens WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RefreshToken>(
            "SELECT * FROM refresh_tokens WHERE user_id = $1 ORDER BY created_ts DESC",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let rows = sqlx::query_as::<_, RefreshToken>(
            r#"
            SELECT * FROM refresh_tokens 
            WHERE user_id = $1 
            AND is_revoked = FALSE 
            AND expires_at > $2
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .bind(now)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_ts = $2,
                revoked_reason = $3
            WHERE token_hash = $1
            "#,
        )
        .bind(token_hash)
        .bind(now)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_ts = $2,
                revoked_reason = $3
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(now)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE refresh_tokens SET
                is_revoked = TRUE,
                revoked_ts = EXTRACT(EPOCH FROM NOW()) * 1000,
                revoked_reason = $2
            WHERE user_id = $1 AND is_revoked = FALSE
            "#,
        )
        .bind(user_id)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn update_token_usage(&self, token_hash: &str, access_token_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE refresh_tokens SET
                access_token_id = $2,
                last_used_ts = $3,
                use_count = use_count + 1
            WHERE token_hash = $1
            "#,
        )
        .bind(token_hash)
        .bind(access_token_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn record_usage(&self, request: &RecordUsageRequest) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO refresh_token_usage (
                refresh_token_id, user_id, old_access_token_id, new_access_token_id,
                used_ts, ip_address, user_agent, success, error_message
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(request.refresh_token_id)
        .bind(&request.user_id)
        .bind(&request.old_access_token_id)
        .bind(&request.new_access_token_id)
        .bind(now)
        .bind(&request.ip_address)
        .bind(&request.user_agent)
        .bind(request.success)
        .bind(&request.error_message)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_family(&self, family_id: &str, user_id: &str, device_id: Option<&str>) -> Result<RefreshTokenFamily, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RefreshTokenFamily>(
            r#"
            INSERT INTO refresh_token_families (family_id, user_id, device_id, created_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(family_id)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_family(&self, family_id: &str) -> Result<Option<RefreshTokenFamily>, sqlx::Error> {
        let row = sqlx::query_as::<_, RefreshTokenFamily>(
            "SELECT * FROM refresh_token_families WHERE family_id = $1",
        )
        .bind(family_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE refresh_token_families SET
                is_compromised = TRUE,
                compromised_ts = $2
            WHERE family_id = $1
            "#,
        )
        .bind(family_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn record_rotation(&self, family_id: &str, old_token_hash: Option<&str>, new_token_hash: &str, reason: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO refresh_token_rotations (family_id, old_token_hash, new_token_hash, rotated_ts, rotation_reason)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(family_id)
        .bind(old_token_hash)
        .bind(new_token_hash)
        .bind(now)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE refresh_token_families SET
                last_refresh_ts = $2,
                refresh_count = refresh_count + 1
            WHERE family_id = $1
            "#,
        )
        .bind(family_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_rotations(&self, family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RefreshTokenRotation>(
            "SELECT * FROM refresh_token_rotations WHERE family_id = $1 ORDER BY rotated_ts DESC",
        )
        .bind(family_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn add_to_blacklist(&self, token_hash: &str, token_type: &str, user_id: &str, expires_at: i64, reason: Option<&str>) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO token_blacklist (token_hash, token_type, user_id, revoked_ts, expires_at, reason)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (token_hash) DO NOTHING
            "#,
        )
        .bind(token_hash)
        .bind(token_type)
        .bind(user_id)
        .bind(now)
        .bind(expires_at)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM token_blacklist WHERE token_hash = $1 AND expires_at > $2",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query(
            "DELETE FROM refresh_tokens WHERE expires_at < $1 AND is_revoked = FALSE",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query(
            "DELETE FROM token_blacklist WHERE expires_at < $1",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error> {
        let row = sqlx::query_as::<_, RefreshTokenStats>(
            r#"
            SELECT 
                user_id,
                COUNT(*) as total_tokens,
                COUNT(*) FILTER (WHERE is_revoked = FALSE AND expires_at > EXTRACT(EPOCH FROM NOW()) * 1000) as active_tokens,
                COUNT(*) FILTER (WHERE is_revoked = TRUE) as revoked_tokens,
                COUNT(*) FILTER (WHERE expires_at <= EXTRACT(EPOCH FROM NOW()) * 1000) as expired_tokens,
                COALESCE(SUM(use_count), 0) as total_uses
            FROM refresh_tokens
            WHERE user_id = $1
            GROUP BY user_id
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RefreshTokenUsage>(
            "SELECT * FROM refresh_token_usage WHERE user_id = $1 ORDER BY used_ts DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        Ok(result.rows_affected() as i64)
    }
}
