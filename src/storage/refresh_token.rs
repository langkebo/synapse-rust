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
    #[serde(rename = "success")]
    pub is_success: bool,
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
    pub success: bool,
    pub error_message: Option<String>,
}

impl RecordUsageRequest {
    pub fn new(
        refresh_token_id: i64,
        user_id: impl Into<String>,
        new_access_token_id: impl Into<String>,
        success: bool,
    ) -> Self {
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

        let row = sqlx::query_as!(
            RefreshToken,
            r#"
            INSERT INTO refresh_tokens (
                token_hash, user_id, device_id, access_token_id, scope, created_ts,
                expires_at, client_info, ip_address, user_agent
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, token_hash, user_id, device_id, access_token_id, scope,
                created_ts AS "created_ts!", expires_at, last_used_ts, use_count AS "use_count!",
                is_revoked AS "is_revoked!", revoked_reason, client_info, ip_address, user_agent
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
            request.user_agent.as_deref(),
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query_as!(
            RefreshToken,
            r#"SELECT id, token_hash, user_id, device_id, access_token_id, scope,
                created_ts AS "created_ts!", expires_at, last_used_ts, use_count AS "use_count!",
                is_revoked AS "is_revoked!", revoked_reason, client_info, ip_address, user_agent
            FROM refresh_tokens WHERE token_hash = $1"#,
            token_hash
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error> {
        let row = sqlx::query_as!(
            RefreshToken,
            r#"SELECT id, token_hash, user_id, device_id, access_token_id, scope,
                created_ts AS "created_ts!", expires_at, last_used_ts, use_count AS "use_count!",
                is_revoked AS "is_revoked!", revoked_reason, client_info, ip_address, user_agent
            FROM refresh_tokens WHERE id = $1"#,
            id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RefreshToken,
            r#"SELECT id, token_hash, user_id, device_id, access_token_id, scope,
                created_ts AS "created_ts!", expires_at, last_used_ts, use_count AS "use_count!",
                is_revoked AS "is_revoked!", revoked_reason, client_info, ip_address, user_agent
            FROM refresh_tokens WHERE user_id = $1 ORDER BY created_ts DESC"#,
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
            r#"SELECT id, token_hash, user_id, device_id, access_token_id, scope,
                created_ts AS "created_ts!", expires_at, last_used_ts, use_count AS "use_count!",
                is_revoked AS "is_revoked!", revoked_reason, client_info, ip_address, user_agent
            FROM refresh_tokens
            WHERE user_id = $1
            AND is_revoked = FALSE
            AND expires_at > $2
            ORDER BY created_ts DESC"#,
            user_id,
            now
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE refresh_tokens SET is_revoked = TRUE, revoked_reason = $2 WHERE token_hash = $1"#,
            token_hash,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"UPDATE refresh_tokens SET is_revoked = TRUE, revoked_reason = $2 WHERE token_hash = $1 AND is_revoked = FALSE"#,
            token_hash,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE refresh_tokens SET is_revoked = TRUE, revoked_reason = $2 WHERE id = $1"#,
            id,
            reason
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"UPDATE refresh_tokens SET is_revoked = TRUE, revoked_reason = $2 WHERE user_id = $1 AND is_revoked = FALSE"#,
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
            r#"UPDATE refresh_tokens SET is_revoked = TRUE, revoked_reason = $3 WHERE user_id = $1 AND device_id != $2 AND is_revoked = FALSE"#,
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
            r#"UPDATE refresh_tokens SET is_revoked = TRUE, revoked_reason = $3 WHERE user_id = $1 AND device_id = $2 AND is_revoked = FALSE"#,
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
            r#"UPDATE refresh_tokens SET access_token_id = $2, last_used_ts = $3, use_count = use_count + 1 WHERE token_hash = $1"#,
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
            r#"INSERT INTO refresh_token_usage (refresh_token_id, user_id, old_access_token_id, new_access_token_id, used_ts, ip_address, user_agent, is_success, error_message)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            request.refresh_token_id,
            &request.user_id,
            request.old_access_token_id.as_deref(),
            &request.new_access_token_id,
            now,
            request.ip_address.as_deref(),
            request.user_agent.as_deref(),
            request.success,
            request.error_message.as_deref(),
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
            r#"INSERT INTO refresh_token_families (family_id, user_id, device_id, created_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING id, family_id, user_id, device_id,
                created_ts AS "created_ts!", last_refresh_ts, refresh_count AS "refresh_count!",
                is_compromised AS "is_compromised!", compromised_at AS "compromised_ts?""#,
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
            r#"SELECT id, family_id, user_id, device_id,
                created_ts AS "created_ts!", last_refresh_ts, refresh_count AS "refresh_count!",
                is_compromised AS "is_compromised!", compromised_at AS "compromised_ts?"
            FROM refresh_token_families WHERE family_id = $1"#,
            family_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query!(
            r#"UPDATE refresh_token_families SET is_compromised = TRUE, compromised_at = $2 WHERE family_id = $1"#,
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
            r#"INSERT INTO refresh_token_rotations (family_id, old_token_hash, new_token_hash, rotated_ts, rotation_reason)
            VALUES ($1, $2, $3, $4, $5)"#,
            family_id,
            old_token_hash,
            new_token_hash,
            now,
            reason
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"UPDATE refresh_token_families SET last_refresh_ts = $2, refresh_count = refresh_count + 1 WHERE family_id = $1"#,
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
            r#"SELECT id, family_id, old_token_hash, new_token_hash,
                rotated_ts AS "rotated_ts!", rotation_reason
            FROM refresh_token_rotations WHERE family_id = $1 ORDER BY rotated_ts DESC"#,
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
            r#"INSERT INTO token_blacklist (token_hash, token_type, user_id, is_revoked, expires_at, reason)
            VALUES ($1, $2, $3, TRUE, $4, $5)
            ON CONFLICT (token_hash) DO NOTHING"#,
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

        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*)::BIGINT AS "count!" FROM token_blacklist WHERE token_hash = $1 AND expires_at > $2"#,
            token_hash,
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query!(
            r#"DELETE FROM refresh_tokens WHERE expires_at < $1 AND is_revoked = FALSE"#,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query!(
            r#"DELETE FROM token_blacklist WHERE expires_at < $1"#,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error> {
        let row = sqlx::query_as!(
            RefreshTokenStats,
            r#"SELECT
                user_id,
                COUNT(*) as "total_tokens!",
                COUNT(*) FILTER (WHERE is_revoked = FALSE AND expires_at > EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) as "active_tokens!",
                COUNT(*) FILTER (WHERE is_revoked = TRUE) as "revoked_tokens!",
                COUNT(*) FILTER (WHERE expires_at <= EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) as "expired_tokens!",
                COALESCE(SUM(use_count), 0) as "total_uses!"
            FROM refresh_tokens
            WHERE user_id = $1
            GROUP BY user_id"#,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RefreshTokenUsage,
            r#"SELECT id, refresh_token_id, user_id, old_access_token_id, new_access_token_id,
                used_ts AS "used_ts!", ip_address, user_agent,
                is_success AS "is_success!: bool", error_message
            FROM refresh_token_usage WHERE user_id = $1 ORDER BY used_ts DESC LIMIT $2"#,
            user_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM refresh_tokens WHERE token_hash = $1"#,
            token_hash
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"DELETE FROM refresh_tokens WHERE user_id = $1"#,
            user_id
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== RecordUsageRequest builder tests =====

    #[test]
    fn test_record_usage_request_new() {
        let req = RecordUsageRequest::new(1, "@alice:example.com", "new_token_123", true);
        assert_eq!(req.refresh_token_id, 1);
        assert_eq!(req.user_id, "@alice:example.com");
        assert_eq!(req.new_access_token_id, "new_token_123");
        assert!(req.success);
        assert!(req.old_access_token_id.is_none());
        assert!(req.ip_address.is_none());
        assert!(req.user_agent.is_none());
        assert!(req.error_message.is_none());
    }

    #[test]
    fn test_record_usage_request_builder_failure() {
        let req = RecordUsageRequest::new(2, "@bob:example.com", "new_token_456", false)
            .old_access_token_id("old_token_abc")
            .ip_address("192.168.1.1")
            .user_agent("TestAgent/1.0")
            .error_message("Token expired");
        assert_eq!(req.refresh_token_id, 2);
        assert_eq!(req.user_id, "@bob:example.com");
        assert_eq!(req.new_access_token_id, "new_token_456");
        assert!(!req.success);
        assert_eq!(req.old_access_token_id, Some("old_token_abc".to_string()));
        assert_eq!(req.ip_address, Some("192.168.1.1".to_string()));
        assert_eq!(req.user_agent, Some("TestAgent/1.0".to_string()));
        assert_eq!(req.error_message, Some("Token expired".to_string()));
    }

    #[test]
    fn test_record_usage_request_builder_chaining() {
        let req = RecordUsageRequest::new(3, "@carol:example.com", "tok_xyz", true)
            .old_access_token_id("old_xyz")
            .ip_address("10.0.0.1")
            .user_agent("Chrome/120")
            .error_message("should be ignored on success");
        assert_eq!(req.refresh_token_id, 3);
        assert!(req.success);
        assert_eq!(req.error_message, Some("should be ignored on success".to_string()));
    }

    #[test]
    fn test_record_usage_request_default() {
        let req = RecordUsageRequest::default();
        assert_eq!(req.refresh_token_id, 0);
        assert!(req.user_id.is_empty());
        assert!(req.new_access_token_id.is_empty());
        assert!(!req.success);
        assert!(req.old_access_token_id.is_none());
        assert!(req.ip_address.is_none());
        assert!(req.user_agent.is_none());
        assert!(req.error_message.is_none());
    }

    // ===== RefreshToken struct tests =====

    #[test]
    fn test_refresh_token_fields() {
        let token = RefreshToken {
            id: 1,
            token_hash: "hash_abc123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            access_token_id: Some("acc_tok_1".to_string()),
            scope: Some("read write".to_string()),
            created_ts: 1700000000000,
            expires_at: Some(1700086400000),
            last_used_ts: Some(1700050000000),
            use_count: 5,
            is_revoked: false,
            revoked_reason: None,
            client_info: Some(serde_json::json!({"name": "TestClient"})),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("TestAgent/1.0".to_string()),
        };
        assert_eq!(token.id, 1);
        assert_eq!(token.token_hash, "hash_abc123");
        assert_eq!(token.user_id, "@alice:example.com");
        assert!(token.device_id.is_some());
        assert_eq!(token.use_count, 5);
        assert!(!token.is_revoked);
        assert!(token.revoked_reason.is_none());
        assert!(token.client_info.is_some());
    }

    #[test]
    fn test_refresh_token_revoked() {
        let token = RefreshToken {
            id: 2,
            token_hash: "revoked_hash".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: None,
            access_token_id: None,
            scope: None,
            created_ts: 1700000000000,
            expires_at: None,
            last_used_ts: None,
            use_count: 0,
            is_revoked: true,
            revoked_reason: Some("User logged out".to_string()),
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        assert!(token.is_revoked);
        assert_eq!(token.revoked_reason, Some("User logged out".to_string()));
        assert!(token.device_id.is_none());
        assert!(token.expires_at.is_none());
    }

    #[test]
    fn test_refresh_token_expired() {
        let now = chrono::Utc::now().timestamp_millis();
        let token = RefreshToken {
            id: 3,
            token_hash: "expired_hash".to_string(),
            user_id: "@carol:example.com".to_string(),
            device_id: Some("DEVICE456".to_string()),
            access_token_id: None,
            scope: None,
            created_ts: now - 172800000,
            expires_at: Some(now - 86400000),
            last_used_ts: None,
            use_count: 0,
            is_revoked: false,
            revoked_reason: None,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };
        assert!(token.expires_at.unwrap() < now);
        assert!(!token.is_revoked);
    }

    // ===== RefreshTokenFamily struct tests =====

    #[test]
    fn test_refresh_token_family() {
        let family = RefreshTokenFamily {
            id: 1,
            family_id: "family-uuid-123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1700000000000,
            last_refresh_ts: Some(1700050000000),
            refresh_count: 3,
            is_compromised: false,
            compromised_ts: None,
        };
        assert_eq!(family.family_id, "family-uuid-123");
        assert_eq!(family.refresh_count, 3);
        assert!(!family.is_compromised);
        assert!(family.compromised_ts.is_none());
    }

    #[test]
    fn test_refresh_token_family_compromised() {
        let family = RefreshTokenFamily {
            id: 2,
            family_id: "compromised-family".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: None,
            created_ts: 1700000000000,
            last_refresh_ts: None,
            refresh_count: 0,
            is_compromised: true,
            compromised_ts: Some(1700050000000),
        };
        assert!(family.is_compromised);
        assert!(family.compromised_ts.is_some());
    }

    // ===== RefreshTokenRotation struct tests =====

    #[test]
    fn test_refresh_token_rotation() {
        let rotation = RefreshTokenRotation {
            id: 1,
            family_id: "family-123".to_string(),
            old_token_hash: Some("old_hash".to_string()),
            new_token_hash: "new_hash".to_string(),
            rotated_ts: 1700050000000,
            rotation_reason: Some("Token refresh".to_string()),
        };
        assert_eq!(rotation.family_id, "family-123");
        assert_eq!(rotation.new_token_hash, "new_hash");
        assert!(rotation.old_token_hash.is_some());
        assert_eq!(rotation.rotation_reason, Some("Token refresh".to_string()));
    }

    #[test]
    fn test_refresh_token_rotation_first_rotation() {
        let rotation = RefreshTokenRotation {
            id: 2,
            family_id: "family-456".to_string(),
            old_token_hash: None,
            new_token_hash: "first_token".to_string(),
            rotated_ts: 1700000000000,
            rotation_reason: None,
        };
        assert!(rotation.old_token_hash.is_none());
        assert!(rotation.rotation_reason.is_none());
    }

    // ===== TokenBlacklistEntry struct tests =====

    #[test]
    fn test_token_blacklist_entry() {
        let entry = TokenBlacklistEntry {
            id: 1,
            token_hash: "blacklisted_hash".to_string(),
            token_type: Some("access_token".to_string()),
            user_id: Some("@alice:example.com".to_string()),
            is_revoked: true,
            expires_at: Some(1800000000000),
            reason: Some("Security incident".to_string()),
        };
        assert!(entry.is_revoked);
        assert_eq!(entry.token_type, Some("access_token".to_string()));
        assert_eq!(entry.reason, Some("Security incident".to_string()));
    }

    // ===== RefreshTokenStats struct tests =====

    #[test]
    fn test_refresh_token_stats() {
        let stats = RefreshTokenStats {
            user_id: "@alice:example.com".to_string(),
            total_tokens: 10,
            active_tokens: 5,
            revoked_tokens: 3,
            expired_tokens: 2,
            total_uses: 50,
        };
        assert_eq!(stats.total_tokens, 10);
        assert_eq!(stats.active_tokens + stats.revoked_tokens + stats.expired_tokens, 10);
        assert_eq!(stats.total_uses, 50);
    }

    // ===== CreateRefreshTokenRequest struct tests =====

    #[test]
    fn test_create_refresh_token_request() {
        let req = CreateRefreshTokenRequest {
            token_hash: "hash_value".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            access_token_id: Some("acc_123".to_string()),
            scope: Some("read write".to_string()),
            expires_at: 1700086400000,
            client_info: Some(serde_json::json!({"name": "App"})),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("TestAgent/1.0".to_string()),
        };
        assert_eq!(req.token_hash, "hash_value");
        assert_eq!(req.user_id, "@alice:example.com");
        assert_eq!(req.expires_at, 1700086400000);
        assert!(req.device_id.is_some());
    }

    // ===== RotateRefreshTokenRequest struct tests =====

    #[test]
    fn test_rotate_refresh_token_request() {
        let req = RotateRefreshTokenRequest {
            old_token_hash: "old_hash".to_string(),
            new_token_hash: "new_hash".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            family_id: Some("family-uuid".to_string()),
            expires_at: 1700086400000,
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("TestAgent/1.0".to_string()),
        };
        assert_eq!(req.old_token_hash, "old_hash");
        assert_eq!(req.new_token_hash, "new_hash");
        assert!(req.family_id.is_some());
    }

    #[test]
    fn test_rotate_refresh_token_request_no_family() {
        let req = RotateRefreshTokenRequest {
            old_token_hash: "old".to_string(),
            new_token_hash: "new".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: None,
            family_id: None,
            expires_at: 1700086400000,
            ip_address: None,
            user_agent: None,
        };
        assert!(req.family_id.is_none());
        assert!(req.device_id.is_none());
    }

    // ===== RefreshTokenUsage struct tests =====

    #[test]
    fn test_refresh_token_usage() {
        let usage = RefreshTokenUsage {
            id: 1,
            refresh_token_id: 100,
            user_id: "@alice:example.com".to_string(),
            old_access_token_id: Some("old_acc".to_string()),
            new_access_token_id: Some("new_acc".to_string()),
            used_ts: 1700050000000,
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("TestAgent/1.0".to_string()),
            is_success: true,
            error_message: None,
        };
        assert!(usage.is_success);
        assert!(usage.error_message.is_none());
        assert_eq!(usage.refresh_token_id, 100);
    }

    #[test]
    fn test_refresh_token_usage_failed() {
        let usage = RefreshTokenUsage {
            id: 2,
            refresh_token_id: 101,
            user_id: "@bob:example.com".to_string(),
            old_access_token_id: None,
            new_access_token_id: None,
            used_ts: 1700050000000,
            ip_address: None,
            user_agent: None,
            is_success: false,
            error_message: Some("Token revoked".to_string()),
        };
        assert!(!usage.is_success);
        assert_eq!(usage.error_message, Some("Token revoked".to_string()));
    }
}
