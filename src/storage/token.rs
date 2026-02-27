use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_ts: i64,
    pub is_valid: bool,
    pub revoked_ts: Option<i64>,
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
        expires_ts: Option<i64>,
    ) -> Result<AccessToken, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let row = sqlx::query_as::<_, AccessToken>(
            r#"
            INSERT INTO access_tokens (token, user_id, device_id, created_ts, expires_ts, is_valid)
            VALUES ($1, $2, $3, $4, $5, TRUE)
            RETURNING id, token, user_id, device_id, created_ts, expires_ts, is_valid, revoked_ts
            "#,
        )
        .bind(token)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(expires_ts)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<AccessToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, AccessToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_ts, is_valid, revoked_ts
            FROM access_tokens WHERE token = $1
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
            SELECT id, token, user_id, device_id, created_ts, expires_ts, is_valid, revoked_ts
            FROM access_tokens WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_token(&self, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM access_tokens WHERE token = $1
            "#,
        )
        .bind(token)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_tokens(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM access_tokens WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_device_tokens(&self, device_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM access_tokens WHERE device_id = $1
            "#,
        )
        .bind(device_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn token_exists(&self, token: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 AS "exists" FROM access_tokens WHERE token = $1 LIMIT 1
            "#,
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
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
            created_ts: 1234567890,
            expires_ts: 1234567890 + 3600,
            is_valid: true,
            revoked_ts: None,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.token, "access_token_abc123");
        assert_eq!(token.user_id, "@alice:example.com");
        assert!(token.device_id.is_some());
        assert!(token.is_valid);
        assert!(token.revoked_ts.is_none());
    }

    #[test]
    fn test_access_token_without_device() {
        let token = AccessToken {
            id: 2,
            token: "token_xyz789".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: None,
            created_ts: 1234567890,
            expires_ts: 1234567890 + 7200,
            is_valid: true,
            revoked_ts: None,
        };

        assert!(token.device_id.is_none());
        assert_eq!(token.expires_ts - token.created_ts, 7200);
    }

    #[test]
    fn test_access_token_revoked() {
        let token = AccessToken {
            id: 3,
            token: "revoked_token".to_string(),
            user_id: "@charlie:example.com".to_string(),
            device_id: Some("DEVICE456".to_string()),
            created_ts: 1234567890,
            expires_ts: 1234567890 + 3600,
            is_valid: false,
            revoked_ts: Some(1234567900),
        };

        assert!(!token.is_valid);
        assert!(token.revoked_ts.is_some());
    }

    #[test]
    fn test_access_token_expiration() {
        let now = chrono::Utc::now().timestamp();
        let token = AccessToken {
            id: 4,
            token: "expiring_token".to_string(),
            user_id: "@user:example.com".to_string(),
            device_id: None,
            created_ts: now,
            expires_ts: now + 86400,
            is_valid: true,
            revoked_ts: None,
        };

        assert!(token.expires_ts > token.created_ts);
        assert_eq!(token.expires_ts - token.created_ts, 86400);
    }

    #[test]
    fn test_access_token_storage_creation() {
        let token = AccessToken {
            id: 6,
            token: "test_token".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: None,
            created_ts: 1234567890,
            expires_ts: 1234567890 + 3600,
            is_valid: true,
            revoked_ts: None,
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
    fn test_token_user_id_association() {
        let token = AccessToken {
            id: 5,
            token: "user_associated_token".to_string(),
            user_id: "@test:matrix.org".to_string(),
            device_id: Some("WEBCLIENT".to_string()),
            created_ts: 1234567890,
            expires_ts: 1234567890 + 3600,
            is_valid: true,
            revoked_ts: None,
        };

        assert!(token.user_id.starts_with('@'));
        assert!(token.user_id.contains(':'));
        assert!(token.device_id.as_ref().unwrap().contains("CLIENT"));
    }
}
