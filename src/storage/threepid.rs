use crate::common::error::ApiError;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserThreepid {
    pub id: i64,
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub validated_ts: Option<i64>,
    pub added_ts: i64,
    pub is_verified: bool,
    pub verification_token: Option<String>,
    pub verification_expires_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThreepidRequest {
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub verification_token: Option<String>,
    pub verification_expires_ts: Option<i64>,
}

#[derive(Clone)]
pub struct ThreepidStorage {
    pool: Arc<PgPool>,
}

impl ThreepidStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn add_threepid(
        &self,
        request: CreateThreepidRequest,
    ) -> Result<UserThreepid, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let threepid = sqlx::query_as::<_, UserThreepid>(
            r#"
            INSERT INTO user_threepids (user_id, medium, address, added_ts, is_verified, verification_token, verification_expires_ts)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING id, user_id, medium, address, validated_ts, added_ts, is_verified, verification_token, verification_expires_ts
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.medium)
        .bind(&request.address)
        .bind(now)
        .bind(&request.verification_token)
        .bind(request.verification_expires_ts)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to add threepid: {}", e)))?;

        Ok(threepid)
    }

    pub async fn get_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as::<_, UserThreepid>(
            r#"
            SELECT id, user_id, medium, address, validated_ts, added_ts, is_verified, verification_token, verification_expires_ts
            FROM user_threepids
            WHERE user_id = $1 AND medium = $2 AND address = $3
            "#,
        )
        .bind(user_id)
        .bind(medium)
        .bind(address)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get threepid: {}", e)))?;

        Ok(threepid)
    }

    pub async fn get_threepids_by_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<UserThreepid>, ApiError> {
        let threepids = sqlx::query_as::<_, UserThreepid>(
            r#"
            SELECT id, user_id, medium, address, validated_ts, added_ts, is_verified, verification_token, verification_expires_ts
            FROM user_threepids
            WHERE user_id = $1
            ORDER BY added_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get threepids: {}", e)))?;

        Ok(threepids)
    }

    pub async fn get_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let threepid = sqlx::query_as::<_, UserThreepid>(
            r#"
            SELECT id, user_id, medium, address, validated_ts, added_ts, is_verified, verification_token, verification_expires_ts
            FROM user_threepids
            WHERE medium = $1 AND address = $2
            "#,
        )
        .bind(medium)
        .bind(address)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get threepid by address: {}", e)))?;

        Ok(threepid)
    }

    pub async fn verify_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
    ) -> Result<bool, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            UPDATE user_threepids
            SET is_verified = TRUE, validated_ts = $4, verification_token = NULL, verification_expires_ts = NULL
            WHERE user_id = $1 AND medium = $2 AND address = $3
            "#,
        )
        .bind(user_id)
        .bind(medium)
        .bind(address)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to verify threepid: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn verify_threepid_by_token(
        &self,
        token: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let threepid = sqlx::query_as::<_, UserThreepid>(
            r#"
            UPDATE user_threepids
            SET is_verified = TRUE, validated_ts = $2, verification_token = NULL, verification_expires_ts = NULL
            WHERE verification_token = $1 AND verification_expires_ts > $2
            RETURNING id, user_id, medium, address, validated_ts, added_ts, is_verified, verification_token, verification_expires_ts
            "#,
        )
        .bind(token)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to verify threepid by token: {}", e)))?;

        Ok(threepid)
    }

    pub async fn remove_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"
            DELETE FROM user_threepids
            WHERE user_id = $1 AND medium = $2 AND address = $3
            "#,
        )
        .bind(user_id)
        .bind(medium)
        .bind(address)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to remove threepid: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn remove_threepids_by_user(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r#"
            DELETE FROM user_threepids
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to remove threepids: {}", e)))?;

        Ok(result.rows_affected())
    }

    pub async fn cleanup_expired_verifications(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            DELETE FROM user_threepids
            WHERE is_verified = FALSE AND verification_expires_ts IS NOT NULL AND verification_expires_ts < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to cleanup expired verifications: {}", e)))?;

        Ok(result.rows_affected())
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
            verification_expires_ts: Some(1234567890000),
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
            validated_ts: Some(1234567890000),
            added_ts: 1234567800000,
            is_verified: true,
            verification_token: None,
            verification_expires_ts: None,
        };
        assert_eq!(threepid.id, 1);
        assert!(threepid.is_verified);
    }
}
