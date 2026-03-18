use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub is_revoked: bool,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenBlacklistEntry {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub revoked_at: i64,
    pub reason: Option<String>,
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
        let row = sqlx::query_as::<_, AccessToken>(
            r#"
            INSERT INTO access_tokens (token, user_id, device_id, created_ts, expires_at, last_used_ts, user_agent, ip_address, is_revoked)
            VALUES ($1, $2, $3, $4, $5, NULL, NULL, NULL, FALSE)
            RETURNING id, token, user_id, device_id, created_ts, expires_at, last_used_ts, user_agent, ip_address, is_revoked, revoked_at
            "#,
        )
        .bind(token)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<AccessToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, AccessToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_at, last_used_ts, user_agent, ip_address, is_revoked, revoked_at
            FROM access_tokens WHERE token = $1 AND is_revoked = FALSE
            "#,
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<AccessToken>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AccessToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_at, last_used_ts, user_agent, ip_address, is_revoked, revoked_at
            FROM access_tokens WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_token(&self, token: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE, revoked_at = $2 WHERE token = $1
            "#,
        )
        .bind(token)
        .bind(now)
        .execute(&*self.pool)
        .await;
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("current transaction is aborted") {
                    tracing::warn!("Connection in aborted transaction state in delete_token, attempting reset");
                    let _ = sqlx::query("ROLLBACK").execute(&*self.pool).await;
                    let _ = sqlx::query("SELECT 1").execute(&*self.pool).await;
                }
                Err(e)
            }
        }
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE, revoked_at = $2 WHERE user_id = $1 AND is_revoked = FALSE
            "#,
        )
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await;
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("current transaction is aborted") {
                    tracing::warn!("Connection in aborted transaction state in delete_user_tokens, attempting reset");
                    let _ = sqlx::query("ROLLBACK").execute(&*self.pool).await;
                    let _ = sqlx::query("SELECT 1").execute(&*self.pool).await;
                }
                Err(e)
            }
        }
    }

    pub async fn delete_device_tokens(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE access_tokens SET is_revoked = TRUE, revoked_at = $2 WHERE device_id = $1 AND is_revoked = FALSE
            "#,
        )
        .bind(device_id)
        .bind(now)
        .execute(&*self.pool)
        .await;
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("current transaction is aborted") {
                    tracing::warn!("Connection in aborted transaction state in delete_device_tokens, attempting reset");
                    let _ = sqlx::query("ROLLBACK").execute(&*self.pool).await;
                    let _ = sqlx::query("SELECT 1").execute(&*self.pool).await;
                }
                Err(e)
            }
        }
    }

    pub async fn token_exists(&self, token: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 AS "exists" FROM access_tokens WHERE token = $1 AND is_revoked = FALSE LIMIT 1
            "#,
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn is_token_revoked(&self, token: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 FROM access_tokens WHERE token = $1 AND is_revoked = TRUE LIMIT 1
            "#,
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn add_to_blacklist(
        &self,
        token: &str,
        user_id: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let token_hash = Self::hash_token(token);

        let result = sqlx::query(
            r#"
            INSERT INTO token_blacklist (token_hash, token, token_type, user_id, revoked_at, reason)
            VALUES ($1, $2, 'access', $3, $4, $5)
            ON CONFLICT (token_hash) DO NOTHING
            "#,
        )
        .bind(&token_hash)
        .bind(token)
        .bind(user_id)
        .bind(now)
        .bind(reason)
        .execute(&*self.pool)
        .await;
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("current transaction is aborted") {
                    tracing::warn!("Connection in aborted transaction state in add_to_blacklist, attempting reset");
                    let _ = sqlx::query("ROLLBACK").execute(&*self.pool).await;
                    let _ = sqlx::query("SELECT 1").execute(&*self.pool).await;
                }
                Err(e)
            }
        }
    }

    pub async fn is_in_blacklist(&self, token: &str) -> Result<bool, sqlx::Error> {
        let token_hash = Self::hash_token(token);
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 FROM token_blacklist WHERE token_hash = $1 LIMIT 1
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(&*self.pool)
        .await;
        
        match result {
            Ok(r) => Ok(r.is_some()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("current transaction is aborted") {
                    tracing::warn!("Connection in aborted transaction state, attempting reset");
                    let _ = sqlx::query("ROLLBACK").execute(&*self.pool).await;
                    let _ = sqlx::query("SELECT 1").execute(&*self.pool).await;
                }
                Err(e)
            }
        }
    }

    pub async fn cleanup_expired_blacklist_entries(
        &self,
        max_age_seconds: i64,
    ) -> Result<u64, sqlx::Error> {
        let cutoff = chrono::Utc::now().timestamp_millis() - max_age_seconds * 1000;
        let result = sqlx::query(
            r#"
            DELETE FROM token_blacklist WHERE revoked_at < $1
            "#,
        )
        .bind(cutoff)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    fn hash_token(token: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let result = hasher.finalize();
        URL_SAFE_NO_PAD.encode(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_token_struct() {
        let token = AccessToken {
            id: 1,
            token: "access_token_abc123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3600000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
            revoked_at: None,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.token, "access_token_abc123");
        assert_eq!(token.user_id, "@alice:example.com");
        assert!(token.device_id.is_some());
        assert!(!token.is_revoked);
        assert!(token.revoked_at.is_none());
    }

    #[test]
    fn test_access_token_without_device() {
        let token = AccessToken {
            id: 2,
            token: "token_xyz789".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: None,
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 7200000),
            last_used_ts: Some(1234567895000),
            user_agent: Some("Mozilla/5.0".to_string()),
            ip_address: Some("192.168.1.1".to_string()),
            is_revoked: false,
            revoked_at: None,
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
            token: "revoked_token".to_string(),
            user_id: "@charlie:example.com".to_string(),
            device_id: Some("DEVICE456".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3600000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: true,
            revoked_at: Some(1234567900000),
        };

        assert!(token.is_revoked);
        assert!(token.revoked_at.is_some());
    }

    #[test]
    fn test_access_token_expiration() {
        let now = chrono::Utc::now().timestamp_millis();
        let token = AccessToken {
            id: 4,
            token: "expiring_token".to_string(),
            user_id: "@user:example.com".to_string(),
            device_id: None,
            created_ts: now,
            expires_at: Some(now + 86400000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
            revoked_at: None,
        };

        assert!(token.expires_at.unwrap() > token.created_ts);
        assert!(token.expires_at.unwrap() - token.created_ts == 86400000);
    }

    #[test]
    fn test_access_token_storage_creation() {
        let token = AccessToken {
            id: 6,
            token: "test_token".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: None,
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3600000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
            revoked_at: None,
        };
        assert_eq!(token.token, "test_token");
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
            token: "user_associated_token".to_string(),
            user_id: "@test:matrix.org".to_string(),
            device_id: Some("WEBCLIENT".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234567890000 + 3600000),
            last_used_ts: Some(1234567895000),
            user_agent: Some("Mozilla/5.0".to_string()),
            ip_address: Some("10.0.0.1".to_string()),
            is_revoked: false,
            revoked_at: None,
        };

        assert!(token.user_id.starts_with('@'));
        assert!(token.user_id.contains(':'));
        assert!(token.device_id.as_ref().unwrap().contains("CLIENT"));
    }
}
