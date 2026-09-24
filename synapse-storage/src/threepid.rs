use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::crypto::hash_token;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;

/// The `UserThreepid` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserThreepid {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
    /// The `validated_at` field.
    pub validated_at: Option<i64>,
    /// The `added_ts` field.
    pub added_ts: i64,
    /// The `is_verified` field.
    pub is_verified: bool,
    /// The `verification_token` field.
    pub verification_token: Option<String>,
    /// The `verification_expires_at` field.
    pub verification_expires_at: Option<i64>,
}

/// The `CreateThreepidRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThreepidRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
    /// The `verification_token` field.
    pub verification_token: Option<String>,
    #[serde(rename = "verification_expires_ts")]
    /// The `verification_expires_at` field.
    pub verification_expires_at: Option<i64>,
}

/// The `ThreepidValidationSession` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreepidValidationSession {
    /// The `id` field.
    pub id: i64,
    /// The `session_id` field.
    pub session_id: String,
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
    /// The `client_secret` field.
    pub client_secret: String,
    /// The `token` field.
    pub token: String,
    /// The `send_attempt` field.
    pub send_attempt: i32,
    /// The `next_link` field.
    pub next_link: Option<String>,
    /// The `is_validated` field.
    pub is_validated: bool,
    /// The `validated_at` field.
    pub validated_at: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
}

/// The `ThreepidStorage` struct.
#[derive(Clone)]
pub struct ThreepidStorage {
    pool: Arc<PgPool>,
}

/// The `ThreepidStoreApi` trait.
#[async_trait]
pub trait ThreepidStoreApi: Send + Sync {
    /// See [`get_verified_threepid_by_address`].
    async fn get_verified_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError>;

    /// See [`get_threepids_by_user`].
    async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError>;

    /// See [`add_verified_threepid`].
    async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError>;

    /// See [`remove_threepid`].
    async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError>;

    /// See [`add_threepid`].
    async fn add_threepid(&self, request: CreateThreepidRequest) -> Result<UserThreepid, ApiError>;

    /// See [`verify_threepid`].
    async fn verify_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError>;

    // Validation session methods (used by route handlers)
    #[allow(clippy::too_many_arguments)]
    /// See [`create_validation_session`].
    async fn create_validation_session(
        &self,
        session_id: &str,
        medium: &str,
        address: &str,
        client_secret: &str,
        token: &str,
        next_link: Option<&str>,
        created_ts: i64,
        expires_at: i64,
    ) -> Result<i64, ApiError>;

    /// See [`get_validation_session`].
    async fn get_validation_session(
        &self,
        session_id: &str,
        client_secret: &str,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError>;

    /// See [`mark_validation_validated`].
    async fn mark_validation_validated(&self, id: i64) -> Result<(), ApiError>;
}

impl ThreepidStorage {
    /// See [`new`].
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: Arc::new(pool.clone()) }
    }

    /// See [`add_threepid`].
    pub async fn add_threepid(&self, request: CreateThreepidRequest) -> Result<UserThreepid, ApiError> {
        let now = current_timestamp_millis();

        // C18: `is_verified` is nullable in the catalog (`BOOLEAN DEFAULT false`, no
        // NOT NULL) while the struct field is a plain `bool` ⇒ `AS "is_verified!"`.
        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            INSERT INTO user_threepids (user_id, medium, address, added_ts, is_verified, verification_token, verification_expires_at)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified AS "is_verified!",
                verification_token,
                verification_expires_at
            "#,
            request.user_id.as_str(),
            request.medium.as_str(),
            request.address.as_str(),
            now,
            request.verification_token.as_deref(),
            request.verification_expires_at,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to add threepid", e))?;

        Ok(threepid)
    }

    /// See [`get_threepid`].
    pub async fn get_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified AS "is_verified!",
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE user_id = $1 AND medium = $2 AND address = $3
            "#,
            user_id,
            medium,
            address,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get threepid", e))?;

        Ok(threepid)
    }

    /// See [`get_threepids_by_user`].
    pub async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        let threepids = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified AS "is_verified!",
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE user_id = $1
            ORDER BY added_ts DESC
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get threepids", e))?;

        Ok(threepids)
    }

    /// See [`get_pending_threepids`].
    ///
    /// "Pending" means the 3PID has never been validated: `add_threepid` (the only
    /// production write path, `:157`) inserts `validated_at IS NULL`. The predicate used
    /// to be the bare `validated_at < added_ts`, which is NULL for every row that path
    /// produces, so the method returned nothing for exactly the rows it exists to list
    /// (D-34). Rows that *were* validated after being added are excluded.
    pub async fn get_pending_threepids(&self, limit: i64) -> Result<Vec<UserThreepid>, ApiError> {
        let threepids = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified AS "is_verified!",
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE validated_at IS NULL OR validated_at < added_ts
            ORDER BY added_ts DESC
            LIMIT $1
            "#,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get pending threepids", e))?;

        Ok(threepids)
    }

    /// See [`get_threepid_by_address`].
    pub async fn get_threepid_by_address(&self, medium: &str, address: &str) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified AS "is_verified!",
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE medium = $1 AND address = $2
            "#,
            medium,
            address,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get threepid by address", e))?;

        Ok(threepid)
    }

    /// See [`get_verified_threepid_by_address`].
    pub async fn get_verified_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id,
                user_id,
                medium,
                address,
                validated_at,
                added_ts,
                is_verified AS "is_verified!",
                verification_token,
                verification_expires_at
            FROM user_threepids
            WHERE medium = $1 AND address = $2 AND is_verified = TRUE
            "#,
            medium,
            address,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get verified threepid by address", e))?;

        Ok(threepid)
    }

    /// See [`verify_threepid`].
    pub async fn verify_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        let now = current_timestamp_millis();

        let result = sqlx::query!(
            r"
            UPDATE user_threepids
            SET is_verified = TRUE, validated_at = $4, verification_token = NULL, verification_expires_at = NULL
            WHERE user_id = $1 AND medium = $2 AND address = $3
            ",
            user_id,
            medium,
            address,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to verify threepid", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`verify_threepid_by_token`].
    pub async fn verify_threepid_by_token(&self, token: &str) -> Result<Option<UserThreepid>, ApiError> {
        let now = current_timestamp_millis();

        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
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
                is_verified AS "is_verified!",
                verification_token,
                verification_expires_at
            "#,
            token,
            now,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to verify threepid by token", e))?;

        Ok(threepid)
    }

    /// See [`remove_threepid`].
    pub async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r"
            DELETE FROM user_threepids
            WHERE user_id = $1 AND medium = $2 AND address = $3
            ",
            user_id,
            medium,
            address,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to remove threepid", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`add_verified_threepid`].
    pub async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r"
            INSERT INTO user_threepids (user_id, medium, address, validated_at, added_ts, is_verified)
            VALUES ($1, $2, $3, $4, $5, TRUE)
            ON CONFLICT (medium, address) DO UPDATE
            SET validated_at = EXCLUDED.validated_at,
                is_verified = TRUE
            WHERE user_threepids.user_id = EXCLUDED.user_id
            ",
            user_id,
            medium,
            address,
            validated_at,
            added_ts,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to add verified threepid", e))?;

        Ok(result.rows_affected())
    }

    /// See [`remove_threepids_by_user`].
    pub async fn remove_threepids_by_user(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r"
            DELETE FROM user_threepids
            WHERE user_id = $1
            ",
            user_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to remove threepids", e))?;

        Ok(result.rows_affected())
    }

    /// See [`cleanup_expired_verifications`].
    pub async fn cleanup_expired_verifications(&self) -> Result<u64, ApiError> {
        let now = current_timestamp_millis();

        let result = sqlx::query!(
            r"
            DELETE FROM user_threepids
            WHERE is_verified = FALSE AND verification_expires_at IS NOT NULL AND verification_expires_at < $1
            ",
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to cleanup expired verifications", e))?;

        Ok(result.rows_affected())
    }

    // === Validation Session Methods (Architecture Gap #2: 3PID Verification) ===

    /// See [`create_validation_session`].
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
        // Store only the HMAC token hash, never the raw token, so a DB leak
        // cannot be replayed against `submitToken` (审查 #30).
        let token_hash = hash_token(token);
        sqlx::query_scalar!(
            r"
            INSERT INTO threepid_validation_session
            (session_id, medium, address, client_secret, token, next_link, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            ",
            session_id,
            medium,
            address,
            client_secret,
            token_hash.as_str(),
            next_link,
            created_ts,
            expires_at,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create validation session", e))
    }

    /// See [`get_validation_session`].
    pub async fn get_validation_session(
        &self,
        session_id: &str,
        client_secret: &str,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        let token_hash = hash_token(token);
        sqlx::query_as!(
            ThreepidValidationSession,
            r"
            SELECT id, session_id, medium, address, client_secret, token,
                send_attempt, next_link, is_validated, validated_at, created_ts, expires_at
            FROM threepid_validation_session
            WHERE session_id = $1 AND client_secret = $2 AND token = $3
            AND is_validated = FALSE AND expires_at > $4
            ",
            session_id,
            client_secret,
            token_hash.as_str(),
            current_timestamp_millis(),
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get validation session", e))
    }

    /// See [`get_validation_session_by_token`].
    pub async fn get_validation_session_by_token(
        &self,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        let token_hash = hash_token(token);
        sqlx::query_as!(
            ThreepidValidationSession,
            r"
            SELECT id, session_id, medium, address, client_secret, token,
                send_attempt, next_link, is_validated, validated_at, created_ts, expires_at
            FROM threepid_validation_session WHERE token = $1
            ",
            token_hash.as_str(),
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get validation session by token", e))
    }

    /// See [`mark_validation_validated`].
    pub async fn mark_validation_validated(&self, id: i64) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            UPDATE threepid_validation_session
            SET is_validated = TRUE, validated_at = $2
            WHERE id = $1 AND is_validated = FALSE
            ",
            id,
            current_timestamp_millis(),
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to mark session validated", e))?;

        Ok(())
    }

    /// See [`increment_validation_send_attempt`].
    pub async fn increment_validation_send_attempt(&self, id: i64) -> Result<(), ApiError> {
        sqlx::query!("UPDATE threepid_validation_session SET send_attempt = send_attempt + 1 WHERE id = $1", id,)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to increment send attempt", e))?;

        Ok(())
    }

    /// See [`cleanup_expired_validation_sessions`].
    pub async fn cleanup_expired_validation_sessions(&self) -> Result<u64, ApiError> {
        sqlx::query!("DELETE FROM threepid_validation_session WHERE expires_at < $1", current_timestamp_millis())
            .execute(&*self.pool)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| ApiError::internal_with_cause("Failed to cleanup sessions", e))
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

    async fn add_threepid(&self, request: CreateThreepidRequest) -> Result<UserThreepid, ApiError> {
        self.add_threepid(request).await
    }

    async fn verify_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        self.verify_threepid(user_id, medium, address).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_validation_session(
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
        self.create_validation_session(
            session_id,
            medium,
            address,
            client_secret,
            token,
            next_link,
            created_ts,
            expires_at,
        )
        .await
    }

    async fn get_validation_session(
        &self,
        session_id: &str,
        client_secret: &str,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        self.get_validation_session(session_id, client_secret, token).await
    }

    async fn mark_validation_validated(&self, id: i64) -> Result<(), ApiError> {
        self.mark_validation_validated(id).await
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
    use sqlx::PgPool;

    /// Shared `public` is deliberately replaced by a per-test schema here:
    /// `cleanup_expired_verifications()` is a schema-wide `DELETE FROM user_threepids`,
    /// so a sibling test's still-valid fixture row could be removed before its assertion.
    ///
    /// Eliminating the shared state removes the race instead of serialising around it
    /// (AGENTS.md rule 7). The guard is returned with the pool so the schema outlives
    /// the whole test — dropping it early spawns a background `DROP SCHEMA`.
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, PgPool) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = (*isolated.pool()).clone();
        (isolated, pool)
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
    async fn test_add_threepid() {
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
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
        let (_isolated, pool) = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@addv_{uuid}:test.com");
        let address = format!("addv_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        let now = current_timestamp_millis();
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
        let (_isolated, pool) = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@gvaddr_{uuid}:test.com");
        let verified_addr = format!("gv_verified_{uuid}@test.com");
        let unverified_addr = format!("gv_unverified_{uuid}@test.com");

        // Cleanup
        let _ = storage.remove_threepid(&user_id, "email", &verified_addr).await;
        let _ = storage.remove_threepid(&user_id, "email", &unverified_addr).await;
        ensure_test_user(&pool, &user_id).await;

        let now = current_timestamp_millis();

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
        let (_isolated, pool) = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let session_id = format!("session_{uuid}");
        let address = format!("vsession_{uuid}@test.com");
        let now = current_timestamp_millis();
        let expires_at = now + 3_600_000;

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

    /// D-34: the pending list must contain the rows the production write path creates.
    ///
    /// `add_threepid` inserts `validated_at IS NULL`; the old predicate
    /// (`validated_at < added_ts`) is NULL for such rows, so the method returned nothing.
    #[tokio::test]
    async fn test_get_pending_threepids() {
        let (_isolated, pool) = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@pend_{uuid}:test.com");
        let address = format!("pend_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        // The real write path — no synthetic `validated_at`, no `is_verified` juggling.
        storage
            .add_threepid(CreateThreepidRequest {
                user_id: user_id.clone(),
                medium: "email".to_string(),
                address: address.clone(),
                verification_token: Some("pending_token".to_string()),
                verification_expires_at: None,
            })
            .await
            .expect("add_threepid should succeed");

        let pending = storage.get_pending_threepids(10).await.expect("get_pending_threepids should succeed");

        // Exact: `get_pending_threepids()` scans the whole table and `test_pool()` is now
        // per-test isolated (see above), so this is the only row it can return.
        assert_eq!(pending.len(), 1, "expected exactly 1 pending threepid, got {}", pending.len());
        assert_eq!(pending[0].address, address);
        assert!(pending[0].validated_at.is_none(), "a never-validated 3PID must be pending");
        assert!(!pending[0].is_verified, "add_threepid writes is_verified = FALSE");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    /// Negative case for D-34: a 3PID validated *after* it was added is not pending.
    ///
    /// This is what the `validated_at IS NULL OR validated_at < added_ts` predicate has to
    /// keep excluding — widening it to "any unverified row" would reintroduce them.
    #[tokio::test]
    async fn test_get_pending_threepids_excludes_validated_rows() {
        let (_isolated, pool) = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@done_{uuid}:test.com");
        let address = format!("done_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        // added_ts = 1000, validated_at = 2000 ⇒ validated after being added.
        storage.add_verified_threepid(&user_id, "email", &address, 2_000, 1_000).await.expect("add should succeed");

        let pending = storage.get_pending_threepids(10).await.expect("get_pending_threepids should succeed");
        assert!(pending.is_empty(), "a validated 3PID must not be listed as pending, got {pending:?}");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_verifications() {
        let (_isolated, pool) = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@cleanup_{uuid}:test.com");
        let address = format!("cleanup_{uuid}@test.com");

        let _ = storage.remove_threepid(&user_id, "email", &address).await;
        ensure_test_user(&pool, &user_id).await;

        // Add a threepid with an expired verification_expires_at
        let past_time = current_timestamp_millis() - 3_600_000;
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

        // Exact: per-test schema (see `test_pool`) — this test inserted the only expired,
        // unverified threepid in it, so no sibling can inflate or drain the count.
        assert_eq!(cleaned, 1, "should clean exactly the 1 expired verification, got {}", cleaned);

        // Verify threepid was removed
        let found = storage.get_threepid(&user_id, "email", &address).await.expect("get should succeed");

        assert!(found.is_none(), "expired threepid should be cleaned up");
    }

    #[tokio::test]
    async fn test_verify_threepid_by_token() {
        let (_isolated, pool) = test_pool().await;
        let storage = ThreepidStorage::new(&pool);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@vbt_{uuid}:test.com");
        let address = format!("vbt_{uuid}@test.com");
        let token = format!("tok_{uuid}");
        let future_expires = current_timestamp_millis() + 3_600_000;

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
        let (_isolated, pool) = test_pool().await;
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
