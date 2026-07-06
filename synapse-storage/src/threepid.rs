use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::error::ApiError;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserThreepid {
    pub id: i64,
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub validated_at: Option<i64>,
    pub added_ts: i64,
    pub is_verified: bool,
    pub verification_token: Option<String>,
    pub verification_expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThreepidRequest {
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub verification_token: Option<String>,
    #[serde(rename = "verification_expires_ts")]
    pub verification_expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreepidValidationSession {
    pub id: i64,
    pub session_id: String,
    pub medium: String,
    pub address: String,
    pub client_secret: String,
    pub token: String,
    pub send_attempt: i32,
    pub next_link: Option<String>,
    pub is_validated: bool,
    pub validated_at: Option<i64>,
    pub created_ts: i64,
    pub expires_at: i64,
}

#[derive(Clone)]
pub struct ThreepidStorage {
    pool: Arc<PgPool>,
}

#[async_trait]
pub trait ThreepidStoreApi: Send + Sync {
    async fn get_verified_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError>;

    async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError>;

    async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError>;

    async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError>;
}

impl ThreepidStorage {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: Arc::new(pool.clone()) }
    }

    pub async fn add_threepid(&self, request: CreateThreepidRequest) -> Result<UserThreepid, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let threepid = sqlx::query_as::<_, UserThreepid>(
            r"
            INSERT INTO user_threepids (user_id, medium, address, added_ts, is_verified, verification_token, verification_expires_at)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified,
                verification_token,
                verification_expires_at
            ",
        )
        .bind(&request.user_id)
        .bind(&request.medium)
        .bind(&request.address)
        .bind(now)
        .bind(&request.verification_token)
        .bind(request.verification_expires_at)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to add threepid", &e))?;

        Ok(threepid)
    }

    pub async fn get_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as::<_, UserThreepid>(
            r"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified,
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE user_id = $1 AND medium = $2 AND address = $3
            ",
        )
        .bind(user_id)
        .bind(medium)
        .bind(address)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get threepid", &e))?;

        Ok(threepid)
    }

    pub async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        let threepids = sqlx::query_as::<_, UserThreepid>(
            r"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified,
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE user_id = $1
            ORDER BY added_ts DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get threepids", &e))?;

        Ok(threepids)
    }

    pub async fn get_pending_threepids(&self, limit: i64) -> Result<Vec<UserThreepid>, ApiError> {
        let threepids = sqlx::query_as::<_, UserThreepid>(
            r"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified,
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE validated_at < added_ts
            ORDER BY added_ts DESC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get pending threepids", &e))?;

        Ok(threepids)
    }

    pub async fn get_threepid_by_address(&self, medium: &str, address: &str) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as::<_, UserThreepid>(
            r"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified,
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE medium = $1 AND address = $2
            ",
        )
        .bind(medium)
        .bind(address)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get threepid by address", &e))?;

        Ok(threepid)
    }

    pub async fn get_verified_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as::<_, UserThreepid>(
            r"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified,
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE medium = $1 AND address = $2 AND is_verified = TRUE
            ",
        )
        .bind(medium)
        .bind(address)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get verified threepid by address", &e))?;

        Ok(threepid)
    }

    pub async fn verify_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r"
            UPDATE user_threepids
            SET is_verified = TRUE, validated_at = $4, verification_token = NULL, verification_expires_at = NULL
            WHERE user_id = $1 AND medium = $2 AND address = $3
            ",
        )
        .bind(user_id)
        .bind(medium)
        .bind(address)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to verify threepid", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn verify_threepid_by_token(&self, token: &str) -> Result<Option<UserThreepid>, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let threepid = sqlx::query_as::<_, UserThreepid>(
            r"
            UPDATE user_threepids
            SET is_verified = TRUE, validated_at = $2, verification_token = NULL, verification_expires_at = NULL
            WHERE verification_token = $1 AND verification_expires_at > $2
            RETURNING
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified,
                verification_token,
                verification_expires_at
            ",
        )
        .bind(token)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to verify threepid by token", &e))?;

        Ok(threepid)
    }

    pub async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r"
            DELETE FROM user_threepids
            WHERE user_id = $1 AND medium = $2 AND address = $3
            ",
        )
        .bind(user_id)
        .bind(medium)
        .bind(address)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to remove threepid", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r"
            INSERT INTO user_threepids (user_id, medium, address, validated_at, added_ts, is_verified)
            VALUES ($1, $2, $3, $4, $5, TRUE)
            ON CONFLICT (medium, address) DO UPDATE
            SET validated_at = EXCLUDED.validated_at,
                is_verified = TRUE
            WHERE user_threepids.user_id = EXCLUDED.user_id
            ",
        )
        .bind(user_id)
        .bind(medium)
        .bind(address)
        .bind(validated_at)
        .bind(added_ts)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to add verified threepid", &e))?;

        Ok(result.rows_affected())
    }

    pub async fn remove_threepids_by_user(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r"
            DELETE FROM user_threepids
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to remove threepids", &e))?;

        Ok(result.rows_affected())
    }

    pub async fn cleanup_expired_verifications(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r"
            DELETE FROM user_threepids
            WHERE is_verified = FALSE AND verification_expires_at IS NOT NULL AND verification_expires_at < $1
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to cleanup expired verifications", &e))?;

        Ok(result.rows_affected())
    }

    // === Validation Session Methods (Architecture Gap #2: 3PID Verification) ===

    #[allow(clippy::too_many_arguments)]
    pub async fn create_validation_session(
        &self,
        session_id: &str,
        medium: &str,
        address: &str,
        client_secret: &str,
        token: &str,
        next_link: Option<&str>,
        created_ts: i64,
        expires_at: i64,
    ) -> Result<i64, ApiError> {
        sqlx::query_as::<_, (i64,)>(
            r"
            INSERT INTO threepid_validation_session
            (session_id, medium, address, client_secret, token, next_link, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            ",
        )
        .bind(session_id)
        .bind(medium)
        .bind(address)
        .bind(client_secret)
        .bind(token)
        .bind(next_link)
        .bind(created_ts)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await
        .map(|r: (i64,)| r.0)
        .map_err(|e| ApiError::internal_with_log("Failed to create validation session", &e))
    }

    pub async fn get_validation_session(
        &self,
        session_id: &str,
        client_secret: &str,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        sqlx::query_as::<_, ThreepidValidationSession>(
            r"
            SELECT id, session_id, medium, address, client_secret, token,
                send_attempt, next_link, is_validated, validated_at, created_ts, expires_at
            FROM threepid_validation_session
            WHERE session_id = $1 AND client_secret = $2 AND token = $3
            AND is_validated = FALSE AND expires_at > $4
            ",
        )
        .bind(session_id)
        .bind(client_secret)
        .bind(token)
        .bind(chrono::Utc::now().timestamp_millis())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get validation session", &e))
    }

    pub async fn get_validation_session_by_token(
        &self,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        sqlx::query_as::<_, ThreepidValidationSession>(
            r"
            SELECT id, session_id, medium, address, client_secret, token,
                send_attempt, next_link, is_validated, validated_at, created_ts, expires_at
            FROM threepid_validation_session WHERE token = $1
            ",
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get validation session by token", &e))
    }

    pub async fn mark_validation_validated(&self, id: i64) -> Result<(), ApiError> {
        sqlx::query(
            r"
            UPDATE threepid_validation_session
            SET is_validated = TRUE, validated_at = $2
            WHERE id = $1 AND is_validated = FALSE
            ",
        )
        .bind(id)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to mark session validated", &e))?;

        Ok(())
    }

    pub async fn increment_validation_send_attempt(&self, id: i64) -> Result<(), ApiError> {
        sqlx::query("UPDATE threepid_validation_session SET send_attempt = send_attempt + 1 WHERE id = $1")
            .bind(id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to increment send attempt", &e))?;

        Ok(())
    }

    pub async fn cleanup_expired_validation_sessions(&self) -> Result<u64, ApiError> {
        sqlx::query("DELETE FROM threepid_validation_session WHERE expires_at < $1")
            .bind(chrono::Utc::now().timestamp_millis() - 86_400_000)
            .execute(&*self.pool)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup sessions", &e))
    }
}

#[async_trait]
impl ThreepidStoreApi for ThreepidStorage {
    async fn get_verified_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        self.get_verified_threepid_by_address(medium, address).await
    }

    async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        self.get_threepids_by_user(user_id).await
    }

    async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError> {
        self.add_verified_threepid(user_id, medium, address, validated_at, added_ts).await
    }

    async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        self.remove_threepid(user_id, medium, address).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_threepid_request() {
        let request = CreateThreepidRequest {
            user_id: "@test:example.com".to_string(),
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
            verification_token: Some("token123".to_string()),
            verification_expires_at: Some(1234567890000),
        };
        assert_eq!(request.medium, "email");
        assert_eq!(request.address, "test@example.com");
    }

    #[test]
    fn test_user_threepid_struct() {
        let threepid = UserThreepid {
            id: 1,
            user_id: "@test:example.com".to_string(),
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
            validated_at: Some(1234567890000),
            added_ts: 1234567800000,
            is_verified: true,
            verification_token: None,
            verification_expires_at: None,
        };
        assert_eq!(threepid.id, 1);
        assert!(threepid.is_verified);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;

    async fn test_pool() -> PgPool {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database")
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
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
    async fn test_add_threepid() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@add_{uuid}:test.com");
        let address = format!("add_{uuid}@test.com");

        // Cleanup at start
        let _ = storage.remove_threepid(&user_id, "email", &address).await;

        ensure_test_user(&pool, &user_id).await;

        let request = CreateThreepidRequest {
            user_id: user_id.clone(),
            medium: "email".to_string(),
            address: address.clone(),
            verification_token: None,
            verification_expires_at: None,
        };

        let result = storage.add_threepid(request).await.expect("add_threepid should succeed");

        assert!(result.id > 0);
        assert_eq!(result.user_id, user_id);
        assert_eq!(result.medium, "email");
        assert_eq!(result.address, address);
        assert!(!result.is_verified);
        assert!(result.added_ts > 0);

        // Cleanup at end
        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_get_threepid_found() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@getf_{uuid}:test.com");
        let address = format!("getf_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add should succeed");

        let found = storage.get_threepid(&user_id, "email", &address).await.expect("get_threepid should succeed");

        assert!(found.is_some(), "threepid should be found");
        let threepid = found.unwrap();
        assert_eq!(threepid.user_id, user_id);
        assert_eq!(threepid.medium, "email");
        assert_eq!(threepid.address, address);

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_get_threepid_not_found() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@nofind_{uuid}:test.com");

        ensure_test_user(&pool, &user_id).await;

        let result = storage
            .get_threepid(&user_id, "email", &format!("nofind_{uuid}@test.com"))
            .await
            .expect("get_threepid should succeed");

        assert!(result.is_none(), "nonexistent threepid should return None");
    }

    #[tokio::test]
    async fn test_get_threepids_by_user() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@list_{uuid}:test.com");
        let addr1 = format!("list1_{uuid}@test.com");
        let addr2 = format!("list2_{uuid}@test.com");

        // Cleanup
        let _ = storage.remove_threepid(&user_id, "email", &addr1).await;
        let _ = storage.remove_threepid(&user_id, "email", &addr2).await;
        ensure_test_user(&pool, &user_id).await;

        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: addr1.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add 1 should succeed");

        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: addr2.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add 2 should succeed");

        let threepids = storage.get_threepids_by_user(&user_id).await.expect("get_threepids_by_user should succeed");

        assert!(threepids.len() >= 2, "expected at least 2 threepids, got {}", threepids.len());
        for t in &threepids {
            assert_eq!(t.user_id, user_id);
        }

        let _ = storage.remove_threepid(&user_id, "email", &addr1).await;
        let _ = storage.remove_threepid(&user_id, "email", &addr2).await;
    }

    #[tokio::test]
    async fn test_get_threepid_by_address() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@addr_{uuid}:test.com");
        let address = format!("addr_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add should succeed");

        let found =
            storage.get_threepid_by_address("email", &address).await.expect("get_threepid_by_address should succeed");

        assert!(found.is_some(), "threepid should be found by address");
        assert_eq!(found.unwrap().address, address);

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_verify_threepid() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@verify_{uuid}:test.com");
        let address = format!("verify_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add should succeed");

        let verified =
            storage.verify_threepid(&user_id, "email", &address).await.expect("verify_threepid should succeed");

        assert!(verified, "verify should return true");

        let found = storage
            .get_threepid(&user_id, "email", &address)
            .await
            .expect("get should succeed")
            .expect("threepid should exist");

        assert!(found.is_verified, "threepid should be verified");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_remove_threepid() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@remove_{uuid}:test.com");
        let address = format!("remove_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add should succeed");

        let removed =
            storage.remove_threepid(&user_id, "email", &address).await.expect("remove_threepid should succeed");

        assert!(removed, "remove should return true");

        let found = storage.get_threepid(&user_id, "email", &address).await.expect("get should succeed");

        assert!(found.is_none(), "threepid should be gone after removal");
    }

    #[tokio::test]
    async fn test_add_verified_threepid() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@addv_{uuid}:test.com");
        let address = format!("addv_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let rows = storage
            .add_verified_threepid(&user_id, "email", &address, now, now)
            .await
            .expect("add_verified_threepid should succeed");

        assert_eq!(rows, 1, "should insert one row");

        let found = storage
            .get_threepid_by_address("email", &address)
            .await
            .expect("get should succeed")
            .expect("threepid should exist");

        assert!(found.is_verified, "threepid should be verified");
        assert_eq!(found.validated_at, Some(now));

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_get_verified_threepid_by_address() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@gvaddr_{uuid}:test.com");
        let verified_addr = format!("gv_verified_{uuid}@test.com");
        let unverified_addr = format!("gv_unverified_{uuid}@test.com");

        // Cleanup
        let _ = storage.remove_threepid(&user_id, "email", &verified_addr).await;
        let _ = storage.remove_threepid(&user_id, "email", &unverified_addr).await;
        ensure_test_user(&pool, &user_id).await;

        let now = chrono::Utc::now().timestamp_millis();

        // Add verified threepid
        storage
            .add_verified_threepid(&user_id, "email", &verified_addr, now, now)
            .await
            .expect("add verified should succeed");

        // Add unverified threepid
        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: unverified_addr.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add unverified should succeed");

        // Query only verified
        let verified = storage
            .get_verified_threepid_by_address("email", &verified_addr)
            .await
            .expect("get verified should succeed");

        assert!(verified.is_some(), "verified threepid should be found by get_verified_threepid_by_address");

        // Unverified should not be found by verified-only query
        let not_found = storage
            .get_verified_threepid_by_address("email", &unverified_addr)
            .await
            .expect("get verified should succeed");

        assert!(not_found.is_none(), "unverified threepid should not be found by get_verified_threepid_by_address");

        let _ = storage.remove_threepid(&user_id, "email", &verified_addr).await;
        let _ = storage.remove_threepid(&user_id, "email", &unverified_addr).await;
    }

    #[tokio::test]
    async fn test_create_validation_session() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let session_id = format!("session_{uuid}");
        let address = format!("vsession_{uuid}@test.com");
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 3600_000;

        // Cleanup: delete any matching validation sessions
        let _ = sqlx::query("DELETE FROM threepid_validation_session WHERE session_id = $1")
            .bind(&session_id)
            .execute(&pool)
            .await;

        let id = storage
            .create_validation_session(
                &session_id,
                "email",
                &address,
                "test_client_secret",
                "test_token_value",
                None,
                now,
                expires_at,
            )
            .await
            .expect("create_validation_session should succeed");

        assert!(id > 0, "should return a valid session id");

        // Cleanup
        let _ = sqlx::query("DELETE FROM threepid_validation_session WHERE id = $1").bind(id).execute(&pool).await;
    }

    #[tokio::test]
    async fn test_get_pending_threepids() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@pend_{uuid}:test.com");
        let address = format!("pend_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        // Use add_verified_threepid with validated_at < added_ts to create a row
        // that matches the get_pending_threepids WHERE validated_at < added_ts filter.
        // Note: the query does not filter on is_verified, so a "verified" threepid
        // with validated_at < added_ts will appear in pending results.
        storage.add_verified_threepid(&user_id, "email", &address, 1, 1000).await.expect("add should succeed");

        let pending = storage.get_pending_threepids(10).await.expect("get_pending_threepids should succeed");

        assert!(pending.len() >= 1, "expected at least 1 pending threepid, got {}", pending.len());

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_verifications() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@cleanup_{uuid}:test.com");
        let address = format!("cleanup_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        // Add a threepid with an expired verification_expires_at
        let past_time = chrono::Utc::now().timestamp_millis() - 3600_000;
        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: Some("expired_token".to_string()),
                verification_expires_at: Some(past_time),
            })
            .await
            .expect("add should succeed");

        let cleaned =
            storage.cleanup_expired_verifications().await.expect("cleanup_expired_verifications should succeed");

        assert!(cleaned >= 1, "should clean at least 1 expired verification, got {}", cleaned);

        // Verify threepid was removed
        let found = storage.get_threepid(&user_id, "email", &address).await.expect("get should succeed");

        assert!(found.is_none(), "expired threepid should be cleaned up");
    }

    #[tokio::test]
    async fn test_verify_threepid_by_token() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@vbt_{uuid}:test.com");
        let address = format!("vbt_{uuid}@test.com");
        let token = format!("tok_{uuid}");
        let future_expires = chrono::Utc::now().timestamp_millis() + 3600_000;

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        // Add threepid with verification token and future expiry
        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: Some(token.clone()),
                verification_expires_at: Some(future_expires),
            })
            .await
            .expect("add should succeed");

        let verified = storage.verify_threepid_by_token(&token).await.expect("verify_threepid_by_token should succeed");

        assert!(verified.is_some(), "verify_threepid_by_token should return the threepid");

        let threepid = verified.unwrap();
        assert!(threepid.is_verified, "threepid should now be verified");
        assert_eq!(threepid.user_id, user_id);

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_threepid_round_trip() {
        let pool = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@rt_{uuid}:test.com");
        let address = format!("rt_{uuid}@test.com");

        // Cleanup at start
        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        // 1. Add
        let added = storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: None,
                verification_expires_at: None,
            })
            .await
            .expect("add should succeed");

        assert!(added.id > 0);
        assert!(!added.is_verified);

        // 2. Get
        let found = storage
            .get_threepid(&user_id, "email", &address)
            .await
            .expect("get should succeed")
            .expect("threepid should exist");

        assert_eq!(found.id, added.id);
        assert_eq!(found.user_id, user_id);

        // 3. Verify
        let verified = storage.verify_threepid(&user_id, "email", &address).await.expect("verify should succeed");

        assert!(verified);

        let after_verify = storage
            .get_threepid(&user_id, "email", &address)
            .await
            .expect("get should succeed")
            .expect("threepid should still exist");

        assert!(after_verify.is_verified, "should be verified");
        assert!(after_verify.validated_at.is_some());

        // 4. Remove
        let removed = storage.remove_threepid(&user_id, "email", &address).await.expect("remove should succeed");

        assert!(removed);

        let after_remove = storage.get_threepid(&user_id, "email", &address).await.expect("get should succeed");

        assert!(after_remove.is_none(), "threepid should be removed after full lifecycle");
    }
}
