use crate::common::error::ApiError;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

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

#[derive(Clone)]
pub struct OpenIdTokenStorage {
    pool: Arc<PgPool>,
}

impl OpenIdTokenStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_token(
        &self,
        request: CreateOpenIdTokenRequest,
    ) -> Result<OpenIdToken, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let token = sqlx::query_as::<_, OpenIdToken>(
            r#"
            INSERT INTO openid_tokens (token, user_id, device_id, created_ts, expires_at, is_valid)
            VALUES ($1, $2, $3, $4, $5, TRUE)
            RETURNING id, token, user_id, device_id, created_ts, expires_at, is_valid
            "#,
        )
        .bind(&request.token)
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(now)
        .bind(request.expires_at)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create OpenID token: {}", e)))?;

        Ok(token)
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        let token_data = sqlx::query_as::<_, OpenIdToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_at, is_valid
            FROM openid_tokens
            WHERE token = $1 AND is_valid = TRUE
            "#,
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get OpenID token: {}", e)))?;

        Ok(token_data)
    }

    pub async fn validate_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let token_data = sqlx::query_as::<_, OpenIdToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_at, is_valid
            FROM openid_tokens
            WHERE token = $1 AND is_valid = TRUE AND expires_at > $2
            "#,
        )
        .bind(token)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to validate OpenID token: {}", e)))?;

        Ok(token_data)
    }

    pub async fn revoke_token(&self, token: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"
            UPDATE openid_tokens
            SET is_valid = FALSE
            WHERE token = $1
            "#,
        )
        .bind(token)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to revoke OpenID token: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn revoke_user_tokens(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r#"
            UPDATE openid_tokens
            SET is_valid = FALSE
            WHERE user_id = $1 AND is_valid = TRUE
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to revoke user OpenID tokens: {}", e)))?;

        Ok(result.rows_affected())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            DELETE FROM openid_tokens
            WHERE expires_at < $1 OR is_valid = FALSE
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to cleanup expired OpenID tokens: {}", e))
        })?;

        Ok(result.rows_affected())
    }

    pub async fn get_tokens_by_user(&self, user_id: &str) -> Result<Vec<OpenIdToken>, ApiError> {
        let tokens = sqlx::query_as::<_, OpenIdToken>(
            r#"
            SELECT id, token, user_id, device_id, created_ts, expires_at, is_valid
            FROM openid_tokens
            WHERE user_id = $1
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user OpenID tokens: {}", e)))?;

        Ok(tokens)
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
