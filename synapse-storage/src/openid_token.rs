use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OpenIdToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: i64,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOpenIdTokenRequest {
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub expires_at: i64,
}

#[async_trait]
pub trait OpenIdTokenStoreApi: Send + Sync {
    async fn create_token(&self, request: CreateOpenIdTokenRequest) -> Result<OpenIdToken, ApiError>;
    async fn get_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError>;
    async fn validate_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError>;
    async fn revoke_token(&self, token: &str) -> Result<bool, ApiError>;
    async fn revoke_user_tokens(&self, user_id: &str) -> Result<u64, ApiError>;
    async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError>;
    async fn get_tokens_by_user(&self, user_id: &str) -> Result<Vec<OpenIdToken>, ApiError>;
}

#[derive(Clone)]
pub struct OpenIdTokenStorage {
    pool: Arc<PgPool>,
}

impl OpenIdTokenStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_token(&self, request: CreateOpenIdTokenRequest) -> Result<OpenIdToken, ApiError> {
        let now = current_timestamp_millis();

        let token = sqlx::query_as::<_, OpenIdToken>(
            r"
            INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid)
            VALUES ($1, $2, $3, $4, $5, TRUE)
            RETURNING id, token, user_id, device_id, created_ts, expires_at, is_valid
            ",
        )
        .bind(&request.token)
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(now)
        .bind(request.expires_at)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create OpenID token", &e))?;

        Ok(token)
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        let token_data = sqlx::query_as::<_, OpenIdToken>(
            r"
            SELECT id, token, user_id, device_id, created_ts, expires_at, is_valid
            FROM openid_tokens
            WHERE token = $1 AND is_valid = TRUE
            ",
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get OpenID token", &e))?;

        Ok(token_data)
    }

    pub async fn validate_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        let now = current_timestamp_millis();

        let token_data = sqlx::query_as::<_, OpenIdToken>(
            r"
            SELECT id, token, user_id, device_id, created_ts, expires_at, is_valid
            FROM openid_tokens
            WHERE token = $1 AND is_valid = TRUE AND expires_at > $2
            ",
        )
        .bind(token)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to validate OpenID token", &e))?;

        Ok(token_data)
    }

    pub async fn revoke_token(&self, token: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r"
            UPDATE openid_tokens
            SET is_valid = FALSE
            WHERE token = $1
            ",
        )
        .bind(token)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to revoke OpenID token", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn revoke_user_tokens(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r"
            UPDATE openid_tokens
            SET is_valid = FALSE
            WHERE user_id = $1 AND is_valid = TRUE
            ",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to revoke user OpenID tokens", &e))?;

        Ok(result.rows_affected())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let now = current_timestamp_millis();

        let result = sqlx::query(
            r"
            DELETE FROM openid_tokens
            WHERE expires_at < $1 OR is_valid = FALSE
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to cleanup expired OpenID tokens", &e))?;

        Ok(result.rows_affected())
    }

    pub async fn get_tokens_by_user(&self, user_id: &str) -> Result<Vec<OpenIdToken>, ApiError> {
        let tokens = sqlx::query_as::<_, OpenIdToken>(
            r"
            SELECT id, token, user_id, device_id, created_ts, expires_at, is_valid
            FROM openid_tokens
            WHERE user_id = $1
            ORDER BY created_ts DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get user OpenID tokens", &e))?;

        Ok(tokens)
    }
}

#[async_trait]
impl OpenIdTokenStoreApi for OpenIdTokenStorage {
    async fn create_token(&self, request: CreateOpenIdTokenRequest) -> Result<OpenIdToken, ApiError> {
        self.create_token(request).await
    }

    async fn get_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        self.get_token(token).await
    }

    async fn validate_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        self.validate_token(token).await
    }

    async fn revoke_token(&self, token: &str) -> Result<bool, ApiError> {
        self.revoke_token(token).await
    }

    async fn revoke_user_tokens(&self, user_id: &str) -> Result<u64, ApiError> {
        self.revoke_user_tokens(user_id).await
    }

    async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        self.cleanup_expired_tokens().await
    }

    async fn get_tokens_by_user(&self, user_id: &str) -> Result<Vec<OpenIdToken>, ApiError> {
        self.get_tokens_by_user(user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_openid_token_request() {
        let request = CreateOpenIdTokenRequest {
            token: "openid_token_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            expires_at: 1234567890000,
        };
        assert_eq!(request.token, "openid_token_123");
        assert_eq!(request.user_id, "@test:example.com");
    }

    #[test]
    fn test_openid_token_struct() {
        let token = OpenIdToken {
            id: 1,
            token: "openid_token_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            expires_at: 1234571490000,
            is_valid: true,
        };
        assert_eq!(token.token, "openid_token_123");
        assert!(token.is_valid);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let now = current_timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
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

    #[tokio::test]
    async fn test_create_token_returns_valid_record() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let user_id = &format!("@openid_test_create_{}:localhost", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let token_str = format!("tok_create_{}", uuid::Uuid::new_v4());
        let far_future = current_timestamp_millis() + 86400000;

        ensure_test_user(&pool, user_id).await;

        let request = CreateOpenIdTokenRequest {
            token: token_str.clone(),
            user_id: user_id.clone(),
            device_id: Some("DEVICE_CREATE".to_string()),
            expires_at: far_future,
        };

        let result = storage.create_token(request).await.expect("create_token should succeed");

        assert!(result.id > 0);
        assert_eq!(result.token, token_str);
        assert_eq!(result.user_id, *user_id);
        assert_eq!(result.device_id.as_deref(), Some("DEVICE_CREATE"));
        assert!(result.created_ts > 0);
        assert_eq!(result.expires_at, far_future);
        assert!(result.is_valid);

        // Cleanup
        sqlx::query("DELETE FROM openid_tokens WHERE token = $1")
            .bind(&token_str)
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }

    #[tokio::test]
    async fn test_get_token_finds_valid_token() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let user_id = &format!("@openid_test_get_{}:localhost", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let token_str = format!("tok_get_{}", uuid::Uuid::new_v4());
        let far_future = current_timestamp_millis() + 86400000;

        ensure_test_user(&pool, user_id).await;

        // Insert directly so we control the state
        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&token_str)
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(current_timestamp_millis())
        .bind(far_future)
        .bind(true)
        .execute(&*pool)
        .await
        .expect("insert failed");

        let found =
            storage.get_token(&token_str).await.expect("get_token should succeed").expect("token should be found");

        assert_eq!(found.token, token_str);
        assert_eq!(found.user_id, *user_id);
        assert!(found.is_valid);

        // Cleanup
        sqlx::query("DELETE FROM openid_tokens WHERE token = $1")
            .bind(&token_str)
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }

    #[tokio::test]
    async fn test_get_token_returns_none_for_nonexistent() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);

        let result = storage.get_token("nonexistent_token_12345_x").await.expect("query should succeed");

        assert!(result.is_none(), "nonexistent token should return None");
    }

    #[tokio::test]
    async fn test_validate_token_returns_token_if_not_expired() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let user_id = &format!("@openid_test_val_{}:localhost", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let token_str = format!("tok_val_{}", uuid::Uuid::new_v4());
        let far_future = current_timestamp_millis() + 86400000;

        ensure_test_user(&pool, user_id).await;

        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&token_str)
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(current_timestamp_millis())
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        let result = storage.validate_token(&token_str).await.expect("validate_token should succeed");

        assert!(result.is_some(), "valid non-expired token should be returned");
        assert_eq!(result.unwrap().token, token_str);

        // Cleanup
        sqlx::query("DELETE FROM openid_tokens WHERE token = $1")
            .bind(&token_str)
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }

    #[tokio::test]
    async fn test_validate_token_returns_none_for_expired_token() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let user_id = &format!("@openid_test_exp_{}:localhost", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let token_str = format!("tok_exp_{}", uuid::Uuid::new_v4());
        let past = current_timestamp_millis() - 3600000; // 1 hour ago

        ensure_test_user(&pool, user_id).await;

        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&token_str)
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(current_timestamp_millis())
        .bind(past)
        .execute(&*pool)
        .await
        .expect("insert failed");

        let result = storage.validate_token(&token_str).await.expect("validate_token should succeed");

        assert!(result.is_none(), "expired token should return None");

        // Cleanup
        sqlx::query("DELETE FROM openid_tokens WHERE token = $1")
            .bind(&token_str)
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }

    #[tokio::test]
    async fn test_revoke_token_returns_true_and_makes_token_not_found() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let user_id = &format!("@openid_test_revoke_{}:localhost", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let token_str = format!("tok_revoke_{}", uuid::Uuid::new_v4());
        let far_future = current_timestamp_millis() + 86400000;

        ensure_test_user(&pool, user_id).await;

        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&token_str)
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(current_timestamp_millis())
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        let revoked = storage.revoke_token(&token_str).await.expect("revoke_token should succeed");

        assert!(revoked, "revoking existing token should return true");

        // Token should no longer be found by get_token (it checks is_valid=TRUE)
        let found = storage.get_token(&token_str).await.expect("get_token should succeed");

        assert!(found.is_none(), "revoked token should not be found by get_token");

        // Cleanup
        sqlx::query("DELETE FROM openid_tokens WHERE token = $1")
            .bind(&token_str)
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }

    #[tokio::test]
    async fn test_revoke_token_returns_false_for_nonexistent() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);

        let revoked = storage.revoke_token("nonexistent_revoke_token_xyz").await.expect("revoke_token should succeed");

        assert!(!revoked, "revoking nonexistent token should return false");
    }

    #[tokio::test]
    async fn test_revoke_user_tokens_revokes_all_for_user() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = &format!("@openid_bulk_revoke_{suffix}:test.com");
        let far_future = current_timestamp_millis() + 86400000;
        let now = current_timestamp_millis();

        ensure_test_user(&pool, user_id).await;

        // Create two tokens
        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&format!("tok_revu1_{suffix}"))
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(now)
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&format!("tok_revu2_{suffix}"))
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(now)
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        let count = storage.revoke_user_tokens(user_id).await.expect("revoke_user_tokens should succeed");

        assert!(count >= 2, "should revoke at least 2 tokens, got {count}");

        // Both tokens should no longer be found
        assert!(storage.get_token(&format!("tok_revu1_{suffix}")).await.unwrap().is_none());
        assert!(storage.get_token(&format!("tok_revu2_{suffix}")).await.unwrap().is_none());

        // Cleanup
        sqlx::query("DELETE FROM openid_tokens WHERE token LIKE $1")
            .bind(&format!("tok_revu%_{suffix}"))
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }

    #[tokio::test]
    async fn test_cleanup_expired_tokens_removes_expired_and_invalid() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = &format!("@openid_cleanup_{suffix}:test.com");
        let far_future = current_timestamp_millis() + 86400000;
        let past = current_timestamp_millis() - 3600000;
        let now = current_timestamp_millis();

        ensure_test_user(&pool, user_id).await;

        // Insert expired token
        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&format!("tok_clean_exp_{suffix}"))
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(now)
        .bind(past)
        .execute(&*pool)
        .await
        .expect("insert failed");

        // Insert invalid token (not expired but invalid)
        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, FALSE)",
        )
        .bind(&format!("tok_clean_inv_{suffix}"))
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(now)
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        // Insert a valid non-expired token that should survive cleanup
        let valid_token = format!("tok_clean_valid_{suffix}");
        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&valid_token)
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(now)
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        let count = storage.cleanup_expired_tokens().await.expect("cleanup_expired_tokens should succeed");

        assert!(count >= 2, "should clean up at least 2 tokens, got {count}");

        // The valid token should still be found
        let found = storage.get_token(&valid_token).await.expect("get_token should succeed");

        assert!(found.is_some(), "valid non-expired token should survive cleanup");

        // Cleanup remaining test data
        sqlx::query("DELETE FROM openid_tokens WHERE token LIKE $1")
            .bind(&format!("tok_clean%_{suffix}"))
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }

    #[tokio::test]
    async fn test_get_tokens_by_user_returns_ordered_desc() {
        let pool = test_pool().await;
        let storage = OpenIdTokenStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = &format!("@openid_list_{suffix}:test.com");
        let far_future = current_timestamp_millis() + 86400000;

        ensure_test_user(&pool, user_id).await;

        // Insert tokens with staggered timestamps
        let base_ts = current_timestamp_millis();
        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&format!("tok_list_1_{suffix}"))
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(base_ts)
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        sqlx::query(
            "INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid) VALUES ($1, $2, $3, $4, $5, TRUE)",
        )
        .bind(&format!("tok_list_2_{suffix}"))
        .bind(user_id)
        .bind::<Option<String>>(None)
        .bind(base_ts + 1000)
        .bind(far_future)
        .execute(&*pool)
        .await
        .expect("insert failed");

        let tokens = storage.get_tokens_by_user(user_id).await.expect("get_tokens_by_user should succeed");

        assert!(tokens.len() >= 2, "should return at least 2 tokens, got {}", tokens.len());

        // Verify descending order by created_ts
        for i in 1..tokens.len() {
            assert!(
                tokens[i - 1].created_ts >= tokens[i].created_ts,
                "tokens should be ordered by created_ts DESC, but {} >= {}",
                tokens[i - 1].created_ts,
                tokens[i].created_ts,
            );
        }

        // All returned tokens should belong to the requested user
        assert!(tokens.iter().all(|t| t.user_id == *user_id));

        // Cleanup
        sqlx::query("DELETE FROM openid_tokens WHERE token LIKE $1")
            .bind(&format!("tok_list%_{suffix}"))
            .execute(&*pool)
            .await
            .expect("cleanup failed");
    }
}
