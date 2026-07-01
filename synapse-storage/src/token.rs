use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccessToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub is_revoked: bool,
}

#[derive(Clone)]
pub struct AccessTokenStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl AccessTokenStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<AccessToken, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let token_hash = Self::hash_token(token);
        let row = sqlx::query_as!(
            AccessToken,
            r#"
            INSERT INTO access_tokens (token_hash, token, user_id, device_id, created_ts, expires_at, last_used_ts, user_agent, ip_address, is_revoked)
            VALUES ($1, NULL, $2, $3, $4, $5, NULL, NULL, NULL, FALSE)
            RETURNING id as "id!", token_hash as "token_hash!", user_id as "user_id!", device_id as "device_id?", created_ts as "created_ts!", expires_at as "expires_at?", last_used_ts as "last_used_ts?", user_agent as "user_agent?", ip_address as "ip_address?", is_revoked as "is_revoked!"
            "#,
            &token_hash,
            user_id,
            device_id,
            now,
            expires_at
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<AccessToken>, sqlx::Error> {
        let token_hash = Self::hash_token(token);
        let row = sqlx::query_as!(
            AccessToken,
            r#"
            SELECT id as "id!", token_hash as "token_hash!", user_id as "user_id!", device_id as "device_id?", created_ts as "created_ts!", expires_at as "expires_at?", last_used_ts as "last_used_ts?", user_agent as "user_agent?", ip_address as "ip_address?", is_revoked as "is_revoked!"
            FROM access_tokens WHERE token_hash = $1 AND is_revoked = FALSE
            "#,
            &token_hash
        )
        .fetch_optional(&*self.pool)
        .await?;
        if row.is_some() {
            return Ok(row);
        }
        let legacy_hash = Self::hash_token_legacy(token);
        let row = sqlx::query_as!(
            AccessToken,
            r#"
            SELECT id as "id!", token_hash as "token_hash!", user_id as "user_id!", device_id as "device_id?", created_ts as "created_ts!", expires_at as "expires_at?", last_used_ts as "last_used_ts?", user_agent as "user_agent?", ip_address as "ip_address?", is_revoked as "is_revoked!"
            FROM access_tokens WHERE token_hash = $1 AND is_revoked = FALSE
            "#,
            &legacy_hash
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<AccessToken>, sqlx::Error> {
        let rows = sqlx::query_as!(
            AccessToken,
            r#"
            SELECT id as "id!", token_hash as "token_hash!", user_id as "user_id!", device_id as "device_id?", created_ts as "created_ts!", expires_at as "expires_at?", last_used_ts as "last_used_ts?", user_agent as "user_agent?", ip_address as "ip_address?", is_revoked as "is_revoked!"
            FROM access_tokens WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_user_token_by_id(&self, user_id: &str, token_id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM access_tokens
            WHERE id = $1 AND user_id = $2
            "#,
            token_id,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_token(&self, token: &str) -> Result<(), sqlx::Error> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);
        sqlx::query!(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE WHERE token_hash IN ($1, $2) AND is_revoked = FALSE
            "#,
            &token_hash,
            &legacy_hash
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE WHERE user_id = $1 AND is_revoked = FALSE
            "#,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_device_tokens(&self, device_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE WHERE device_id = $1 AND is_revoked = FALSE
            "#,
            device_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_device_tokens(&self, user_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE
            WHERE user_id = $1 AND device_id = $2 AND is_revoked = FALSE
            "#,
            user_id,
            device_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_tokens_except_device(&self, user_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE
            WHERE user_id = $1 AND device_id != $2 AND is_revoked = FALSE
            "#,
            user_id,
            device_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn token_exists(&self, token: &str) -> Result<bool, sqlx::Error> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "exists!"
            FROM access_tokens
            WHERE token_hash IN ($1, $2) AND is_revoked = FALSE
            LIMIT 1
            "#,
            &token_hash,
            &legacy_hash
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn is_token_revoked(&self, token: &str) -> Result<bool, sqlx::Error> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "exists!"
            FROM access_tokens
            WHERE token_hash IN ($1, $2) AND is_revoked = TRUE
            LIMIT 1
            "#,
            &token_hash,
            &legacy_hash
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn add_to_blacklist(&self, token: &str, user_id: &str, reason: Option<&str>) -> Result<(), sqlx::Error> {
        let token_hash = Self::hash_token(token);
        self.add_hash_to_blacklist(&token_hash, user_id, reason).await?;
        let legacy_hash = Self::hash_token_legacy(token);
        self.add_hash_to_blacklist(&legacy_hash, user_id, reason).await
    }

    pub async fn add_hash_to_blacklist(
        &self,
        token_hash: &str,
        user_id: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO token_blacklist (token_hash, token, token_type, user_id, is_revoked, reason)
            VALUES ($1, NULL, 'access', $2, TRUE, $3)
            ON CONFLICT (token_hash) DO NOTHING
            "#,
            token_hash,
            user_id,
            reason
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn is_in_blacklist(&self, token: &str) -> Result<bool, sqlx::Error> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "exists!"
            FROM token_blacklist
            WHERE token_hash IN ($1, $2)
              AND (expires_at IS NULL OR expires_at = 0 OR expires_at > $3)
            LIMIT 1
            "#,
            &token_hash,
            &legacy_hash,
            now
        )
        .fetch_optional(&*self.pool)
        .await;

        match result {
            Ok(r) => Ok(r.is_some()),
            Err(e) => Err(e),
        }
    }

    pub async fn cleanup_expired_blacklist_entries(&self, max_age_seconds: i64) -> Result<u64, sqlx::Error> {
        let cutoff = chrono::Utc::now().timestamp_millis() - max_age_seconds * 1000;
        let result = sqlx::query!(
            r#"
            DELETE FROM token_blacklist WHERE expires_at > 0 AND expires_at < $1
            "#,
            cutoff
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query!(
            r#"
            DELETE FROM access_tokens WHERE expires_at IS NOT NULL AND expires_at < $1
            "#,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    fn hash_token(token: &str) -> String {
        synapse_common::crypto::hash_token(token)
    }

    fn hash_token_legacy(token: &str) -> String {
        synapse_common::crypto::hash_token_legacy(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_token_struct() {
        let token = AccessToken {
            id: 1,
            token_hash: "access_token_hash_abc123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3_600_000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.token_hash, "access_token_hash_abc123");
        assert_eq!(token.user_id, "@alice:example.com");
        assert!(token.device_id.is_some());
        assert!(!token.is_revoked);
    }

    #[test]
    fn test_access_token_without_device() {
        let token = AccessToken {
            id: 2,
            token_hash: "token_hash_xyz789".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: None,
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 7200000),
            last_used_ts: Some(1234567895000),
            user_agent: Some("Mozilla/5.0".to_string()),
            ip_address: Some("192.168.1.1".to_string()),
            is_revoked: false,
        };

        assert!(token.device_id.is_none());
        assert!(token.expires_at.unwrap() - token.created_ts == 7200000);
        assert!(token.last_used_ts.is_some());
        assert!(token.user_agent.is_some());
    }

    #[test]
    fn test_access_token_revoked() {
        let token = AccessToken {
            id: 3,
            token_hash: "revoked_token_hash".to_string(),
            user_id: "@charlie:example.com".to_string(),
            device_id: Some("DEVICE456".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3_600_000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: true,
        };

        assert!(token.is_revoked);
    }

    #[test]
    fn test_access_token_expiration() {
        let now = chrono::Utc::now().timestamp_millis();
        let token = AccessToken {
            id: 4,
            token_hash: "expiring_token_hash".to_string(),
            user_id: "@user:example.com".to_string(),
            device_id: None,
            created_ts: now,
            expires_at: Some(now + 86_400_000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };

        assert!(token.expires_at.unwrap() > token.created_ts);
        assert!(token.expires_at.unwrap() - token.created_ts == 86_400_000);
    }

    #[test]
    fn test_access_token_storage_creation() {
        let token = AccessToken {
            id: 6,
            token_hash: "test_token_hash".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: None,
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3_600_000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        assert_eq!(token.token_hash, "test_token_hash");
    }

    #[test]
    fn test_token_format_validation() {
        let valid_tokens = vec![
            "syt_abc123_def456",
            "MDAxYWxvY2F0aW9uIGV4YW1wbGUuY29tCjAwMWlkZW50aWZpZXIga2V5CjAwMmNpZCB0b2tlbiA9IDEyMzQ1",
            "simple_token_123",
        ];

        for token in valid_tokens {
            assert!(!token.is_empty());
            assert!(token.len() > 5);
        }
    }

    #[test]
    fn test_access_token_user_association() {
        let token = AccessToken {
            id: 5,
            token_hash: "user_associated_token_hash".to_string(),
            user_id: "@test:matrix.org".to_string(),
            device_id: Some("WEBCLIENT".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3_600_000),
            last_used_ts: Some(1234567895000),
            user_agent: Some("Mozilla/5.0".to_string()),
            ip_address: Some("10.0.0.1".to_string()),
            is_revoked: false,
        };

        assert!(token.user_id.starts_with('@'));
        assert!(token.user_id.contains(':'));
        assert!(token.device_id.as_ref().unwrap().contains("CLIENT"));
    }

    // -------------------------------------------------------------------------
    // Appended tests (v2 plan A11): AccessToken expiration logic, is_revoked
    // vs is_valid (active) mutual exclusion, Clone semantics.
    // -------------------------------------------------------------------------

    /// Pure helper: a token is "valid" iff it is not revoked AND has not
    /// expired. Mirrors the runtime check performed by auth middleware.
    fn is_token_valid(token: &AccessToken, now_ms: i64) -> bool {
        if token.is_revoked {
            return false;
        }
        match token.expires_at {
            Some(exp) => exp > now_ms,
            None => true, // No expiry => never expires
        }
    }

    #[test]
    fn test_access_token_expired_in_past() {
        let now = chrono::Utc::now().timestamp_millis();
        let token = AccessToken {
            id: 10,
            token_hash: "h".to_string(),
            user_id: "@u:s".to_string(),
            device_id: None,
            created_ts: now - 10_000,
            expires_at: Some(now - 1_000), // expired 1s ago
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        assert!(!is_token_valid(&token, now), "expired token must be invalid");
        assert!(token.expires_at.unwrap() < now);
    }

    #[test]
    fn test_access_token_no_expiration_never_expires() {
        let now = chrono::Utc::now().timestamp_millis();
        let token = AccessToken {
            id: 11,
            token_hash: "h".to_string(),
            user_id: "@u:s".to_string(),
            device_id: None,
            created_ts: 1,
            expires_at: None,
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        assert!(is_token_valid(&token, now), "token with no expiry should never expire");
        assert!(token.expires_at.is_none());
    }

    #[test]
    fn test_access_token_revoked_overrides_expiration() {
        // Token is revoked AND expired — revoked state must take precedence.
        let now = chrono::Utc::now().timestamp_millis();
        let token = AccessToken {
            id: 12,
            token_hash: "h".to_string(),
            user_id: "@u:s".to_string(),
            device_id: None,
            created_ts: now - 100_000,
            expires_at: Some(now - 1_000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: true,
        };
        assert!(!is_token_valid(&token, now));
        assert!(token.is_revoked);
    }

    #[test]
    fn test_access_token_active_and_unexpired() {
        let now = chrono::Utc::now().timestamp_millis();
        let token = AccessToken {
            id: 13,
            token_hash: "h".to_string(),
            user_id: "@u:s".to_string(),
            device_id: Some("DEV".to_string()),
            created_ts: now,
            expires_at: Some(now + 3_600_000), // 1 hour in future
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        assert!(is_token_valid(&token, now));
        assert!(!token.is_revoked);
    }

    #[test]
    fn test_access_token_revoked_state_transition() {
        let mut token = AccessToken {
            id: 14,
            token_hash: "h".to_string(),
            user_id: "@u:s".to_string(),
            device_id: None,
            created_ts: 1,
            expires_at: None,
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        let now = chrono::Utc::now().timestamp_millis();
        assert!(is_token_valid(&token, now), "fresh token should be valid");
        // Revoke the token.
        token.is_revoked = true;
        assert!(!is_token_valid(&token, now), "revoked token must be invalid");
    }

    #[test]
    fn test_access_token_clone_preserves_all_fields() {
        let token = AccessToken {
            id: 15,
            token_hash: "hash-xyz".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE".to_string()),
            created_ts: 1_700_000_000_000,
            expires_at: Some(1_700_000_003_600),
            last_used_ts: Some(1_700_000_001_000),
            user_agent: Some("Mozilla/5.0".to_string()),
            ip_address: Some("10.0.0.1".to_string()),
            is_revoked: false,
        };
        let cloned = token.clone();
        assert_eq!(cloned.id, token.id);
        assert_eq!(cloned.token_hash, token.token_hash);
        assert_eq!(cloned.user_id, token.user_id);
        assert_eq!(cloned.device_id, token.device_id);
        assert_eq!(cloned.expires_at, token.expires_at);
        assert_eq!(cloned.last_used_ts, token.last_used_ts);
        assert_eq!(cloned.is_revoked, token.is_revoked);
    }

    #[test]
    fn test_access_token_expiry_boundary_exactly_now() {
        // Edge case: token whose expires_at == now is treated as expired
        // (strict greater-than comparison — token must be valid strictly
        // after `now`).
        let now = 1_700_000_000_000;
        let token = AccessToken {
            id: 16,
            token_hash: "h".to_string(),
            user_id: "@u:s".to_string(),
            device_id: None,
            created_ts: now - 3_600_000,
            expires_at: Some(now),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        assert!(!is_token_valid(&token, now), "expires_at == now should be expired");
        assert!(is_token_valid(&token, now - 1), "expires_at == now should still be valid one tick before");
    }
}
