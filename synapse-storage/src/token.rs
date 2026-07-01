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
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Insert a minimal user row so the foreign-key constraint on
    /// `access_tokens.user_id` is satisfied.  Uses ON CONFLICT DO NOTHING
    /// so the same user can safely be referenced by multiple tests.
    async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        let username = user_id
            .strip_prefix('@')
            .and_then(|u| u.split(':').next())
            .unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    /// Insert a minimal device row so the foreign-key constraint on
    /// `access_tokens.device_id` is satisfied.
    async fn ensure_test_device(pool: &Pool<Postgres>, user_id: &str, device_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (device_id) DO NOTHING"#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test device");
    }

    #[tokio::test]
    async fn test_create_token_returns_valid_record() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_token_{}", uuid::Uuid::new_v4());
        let user_id = "@test_user:example.com";

        ensure_test_user(&pool, user_id).await;

        let token = storage.create_token(token_str, user_id, None, None)
            .await
            .expect("create_token should succeed");

        assert!(token.id > 0);
        assert!(!token.token_hash.is_empty());
        assert_eq!(token.user_id, user_id);
        assert!(token.device_id.is_none());
        assert!(token.created_ts > 0);
        assert!(!token.is_revoked);
    }

    #[tokio::test]
    async fn test_create_token_with_device_id() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_device_token_{}", uuid::Uuid::new_v4());
        let device_id = &format!("device_{}", uuid::Uuid::new_v4());

        ensure_test_user(&pool, "@alice:test.com").await;
        ensure_test_device(&pool, "@alice:test.com", device_id).await;

        let token = storage.create_token(token_str, "@alice:test.com", Some(device_id), None)
            .await
            .expect("create_token with device should succeed");

        assert_eq!(token.device_id.as_deref(), Some(device_id.as_str()));
    }

    #[tokio::test]
    async fn test_create_token_with_expiry() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_expiring_token_{}", uuid::Uuid::new_v4());
        let expires_at = chrono::Utc::now().timestamp_millis() + 3600000;

        ensure_test_user(&pool, "@bob:test.com").await;

        let token = storage.create_token(token_str, "@bob:test.com", None, Some(expires_at))
            .await
            .expect("create_token with expiry should succeed");

        assert!(token.expires_at.is_some());
    }

    #[tokio::test]
    async fn test_get_token_finds_created_token() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_retrieve_{}", uuid::Uuid::new_v4());

        ensure_test_user(&pool, "@charlie:test.com").await;

        storage.create_token(token_str, "@charlie:test.com", None, None)
            .await
            .expect("create should succeed");

        let found = storage.get_token(token_str)
            .await
            .expect("get_token should succeed")
            .expect("token should be found");

        assert_eq!(found.user_id, "@charlie:test.com");
    }

    #[tokio::test]
    async fn test_get_token_returns_none_for_nonexistent() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);

        let result = storage.get_token("nonexistent_token_12345")
            .await
            .expect("query should succeed");

        assert!(result.is_none(), "nonexistent token should return None");
    }

    #[tokio::test]
    async fn test_get_user_tokens_returns_only_user_tokens() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_a = &format!("@user_a_{suffix}:test.com");
        let user_b = &format!("@user_b_{suffix}:test.com");

        ensure_test_user(&pool, user_a).await;
        ensure_test_user(&pool, user_b).await;

        storage.create_token(&format!("tok_a1_{suffix}"), user_a, None, None).await.unwrap();
        storage.create_token(&format!("tok_b1_{suffix}"), user_b, None, None).await.unwrap();

        let tokens_a = storage.get_user_tokens(user_a).await.unwrap();
        assert!(tokens_a.iter().all(|t| t.user_id == *user_a));
    }

    #[tokio::test]
    async fn test_delete_token_revokes_it() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_revoke_{}", uuid::Uuid::new_v4());

        ensure_test_user(&pool, "@revoke:test.com").await;
        storage.create_token(token_str, "@revoke:test.com", None, None).await.unwrap();
        storage.delete_token(token_str).await.expect("delete should succeed");

        let found = storage.get_token(token_str).await.unwrap();
        assert!(found.is_none(), "revoked token should not be found by get_token");
    }

    #[tokio::test]
    async fn test_is_token_revoked_detects_revoked() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_is_revoked_{}", uuid::Uuid::new_v4());

        ensure_test_user(&pool, "@is_revoked:test.com").await;
        storage.create_token(token_str, "@is_revoked:test.com", None, None).await.unwrap();
        assert!(!storage.is_token_revoked(token_str).await.unwrap());

        storage.delete_token(token_str).await.unwrap();
        assert!(storage.is_token_revoked(token_str).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_user_tokens_revokes_all_for_user() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user = &format!("@bulk_revoke_{suffix}:test.com");

        ensure_test_user(&pool, user).await;

        storage.create_token(&format!("t1_{suffix}"), user, None, None).await.unwrap();
        storage.create_token(&format!("t2_{suffix}"), user, None, None).await.unwrap();
        storage.delete_user_tokens(user).await.unwrap();

        assert!(storage.get_token(&format!("t1_{suffix}")).await.unwrap().is_none());
        assert!(storage.get_token(&format!("t2_{suffix}")).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_token_exists_positive_and_negative() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_exists_{}", uuid::Uuid::new_v4());

        ensure_test_user(&pool, "@exists:test.com").await;

        assert!(!storage.token_exists(token_str).await.unwrap());
        storage.create_token(token_str, "@exists:test.com", None, None).await.unwrap();
        assert!(storage.token_exists(token_str).await.unwrap());
    }

    #[tokio::test]
    async fn test_blacklist_add_and_check() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let token_str = &format!("test_blacklist_{}", uuid::Uuid::new_v4());

        ensure_test_user(&pool, "@blacklist:test.com").await;
        storage.create_token(token_str, "@blacklist:test.com", None, None).await.unwrap();
        storage.add_to_blacklist(token_str, "@blacklist:test.com", Some("test_reason")).await.unwrap();

        assert!(storage.is_in_blacklist(token_str).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_user_device_tokens_targets_specific_device() {
        let pool = test_pool().await;
        let storage = AccessTokenStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user = &format!("@device_revoke_{suffix}:test.com");

        ensure_test_user(&pool, user).await;
        ensure_test_device(&pool, user, "DEV_A").await;
        ensure_test_device(&pool, user, "DEV_B").await;

        storage.create_token(&format!("d1_{suffix}"), user, Some("DEV_A"), None).await.unwrap();
        storage.create_token(&format!("d2_{suffix}"), user, Some("DEV_B"), None).await.unwrap();
        storage.delete_user_device_tokens(user, "DEV_A").await.unwrap();

        assert!(storage.get_token(&format!("d1_{suffix}")).await.unwrap().is_none());
        assert!(storage.get_token(&format!("d2_{suffix}")).await.unwrap().is_some());
    }
}
