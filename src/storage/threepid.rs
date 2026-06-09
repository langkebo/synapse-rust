use crate::common::error::ApiError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

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

impl ThreepidStorage {
    pub fn new(pool: impl AsRef<PgPool>) -> Self {
        Self {
            pool: Arc::new(pool.as_ref().clone()),
        }
    }

    pub async fn add_threepid(&self, request: CreateThreepidRequest) -> Result<UserThreepid, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            INSERT INTO user_threepids (user_id, medium, address, added_ts, is_verified, verification_token, verification_expires_at)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING
                id AS "id!", user_id AS "user_id!", medium AS "medium!", address AS "address!",
                validated_at AS "validated_at?", added_ts AS "added_ts!",
                COALESCE(is_verified, FALSE) AS "is_verified!",
                verification_token AS "verification_token?", verification_expires_at AS "verification_expires_at?"
            "#,
            &request.user_id,
            &request.medium,
            &request.address,
            now,
            request.verification_token.as_deref(),
            request.verification_expires_at,
        )
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
        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id AS "id!", user_id AS "user_id!", medium AS "medium!", address AS "address!",
                validated_at AS "validated_at?", added_ts AS "added_ts!",
                COALESCE(is_verified, FALSE) AS "is_verified!",
                verification_token AS "verification_token?", verification_expires_at AS "verification_expires_at?"
            FROM user_threepids
            WHERE user_id = $1 AND medium = $2 AND address = $3
            "#,
            user_id,
            medium,
            address,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get threepid", &e))?;

        Ok(threepid)
    }

    pub async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        let threepids = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id AS "id!", user_id AS "user_id!", medium AS "medium!", address AS "address!",
                validated_at AS "validated_at?", added_ts AS "added_ts!",
                COALESCE(is_verified, FALSE) AS "is_verified!",
                verification_token AS "verification_token?", verification_expires_at AS "verification_expires_at?"
            FROM user_threepids
            WHERE user_id = $1
            ORDER BY added_ts DESC
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get threepids", &e))?;

        Ok(threepids)
    }

    pub async fn get_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id AS "id!", user_id AS "user_id!", medium AS "medium!", address AS "address!",
                validated_at AS "validated_at?", added_ts AS "added_ts!",
                COALESCE(is_verified, FALSE) AS "is_verified!",
                verification_token AS "verification_token?", verification_expires_at AS "verification_expires_at?"
            FROM user_threepids
            WHERE medium = $1 AND address = $2
            "#,
            medium,
            address,
        )
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
        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            SELECT
                id AS "id!", user_id AS "user_id!", medium AS "medium!", address AS "address!",
                validated_at AS "validated_at?", added_ts AS "added_ts!",
                COALESCE(is_verified, FALSE) AS "is_verified!",
                verification_token AS "verification_token?", verification_expires_at AS "verification_expires_at?"
            FROM user_threepids
            WHERE medium = $1 AND address = $2 AND is_verified = TRUE
            "#,
            medium,
            address,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get verified threepid by address", &e))?;

        Ok(threepid)
    }

    pub async fn verify_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

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
        .map_err(|e| ApiError::internal_with_log("Failed to verify threepid", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn verify_threepid_by_token(&self, token: &str) -> Result<Option<UserThreepid>, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let threepid = sqlx::query_as!(
            UserThreepid,
            r#"
            UPDATE user_threepids
            SET is_verified = TRUE, validated_at = $2, verification_token = NULL, verification_expires_at = NULL
            WHERE verification_token = $1 AND verification_expires_at > $2
            RETURNING
                id AS "id!", user_id AS "user_id!", medium AS "medium!", address AS "address!",
                validated_at AS "validated_at?", added_ts AS "added_ts!",
                COALESCE(is_verified, FALSE) AS "is_verified!",
                verification_token AS "verification_token?", verification_expires_at AS "verification_expires_at?"
            "#,
            token,
            now,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to verify threepid by token", &e))?;

        Ok(threepid)
    }

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
        .map_err(|e| ApiError::internal_with_log("Failed to add verified threepid", &e))?;

        Ok(result.rows_affected())
    }

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
        .map_err(|e| ApiError::internal_with_log("Failed to remove threepids", &e))?;

        Ok(result.rows_affected())
    }

    pub async fn cleanup_expired_verifications(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query!(
            r"
            DELETE FROM user_threepids
            WHERE is_verified = FALSE AND verification_expires_at IS NOT NULL AND verification_expires_at < $1
            ",
            now,
        )
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
        let id = sqlx::query_scalar!(
            r#"INSERT INTO threepid_validation_session
            (session_id, medium, address, client_secret, token, next_link, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id AS "id!""#,
            session_id,
            medium,
            address,
            client_secret,
            token,
            next_link,
            created_ts,
            expires_at,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create validation session", &e))?;
        Ok(id)
    }

    pub async fn get_validation_session(
        &self,
        session_id: &str,
        client_secret: &str,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        sqlx::query_as!(
            ThreepidValidationSession,
            r#"
            SELECT
                id AS "id!", session_id AS "session_id!", medium AS "medium!", address AS "address!",
                client_secret AS "client_secret!", token AS "token!", send_attempt AS "send_attempt!",
                next_link AS "next_link?", is_validated AS "is_validated!", validated_at AS "validated_at?",
                created_ts AS "created_ts!", expires_at AS "expires_at!"
            FROM threepid_validation_session
            WHERE session_id = $1 AND client_secret = $2 AND token = $3
            AND is_validated = FALSE AND expires_at > $4
            "#,
            session_id,
            client_secret,
            token,
            chrono::Utc::now().timestamp_millis(),
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get validation session", &e))
    }

    pub async fn get_validation_session_by_token(
        &self,
        token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        sqlx::query_as!(
            ThreepidValidationSession,
            r#"
            SELECT
                id AS "id!", session_id AS "session_id!", medium AS "medium!", address AS "address!",
                client_secret AS "client_secret!", token AS "token!", send_attempt AS "send_attempt!",
                next_link AS "next_link?", is_validated AS "is_validated!", validated_at AS "validated_at?",
                created_ts AS "created_ts!", expires_at AS "expires_at!"
            FROM threepid_validation_session WHERE token = $1
            "#,
            token,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get validation session by token", &e))
    }

    pub async fn mark_validation_validated(&self, id: i64) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            UPDATE threepid_validation_session
            SET is_validated = TRUE, validated_at = $2
            WHERE id = $1
            ",
            id,
            chrono::Utc::now().timestamp_millis(),
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to mark session validated", &e))?;

        Ok(())
    }

    pub async fn increment_validation_send_attempt(&self, id: i64) -> Result<(), ApiError> {
        sqlx::query!("UPDATE threepid_validation_session SET send_attempt = send_attempt + 1 WHERE id = $1", id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to increment send attempt", &e))?;

        Ok(())
    }

    pub async fn cleanup_expired_validation_sessions(&self) -> Result<u64, ApiError> {
        let result = sqlx::query!("DELETE FROM threepid_validation_session WHERE expires_at < $1", chrono::Utc::now().timestamp_millis())
            .execute(&*self.pool)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup sessions", &e))?;
        Ok(result)
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