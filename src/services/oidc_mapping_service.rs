use crate::common::ApiError;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::instrument;

#[derive(Clone)]
pub struct OidcMappingService {
    pool: Arc<PgPool>,
}

impl OidcMappingService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    #[instrument(skip(self))]
    pub async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, ApiError> {
        sqlx::query_scalar!(
            "SELECT user_id FROM oidc_user_mapping WHERE issuer = $1 AND subject = $2",
            issuer,
            subject
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to query OIDC user mapping", &e))
    }

    #[instrument(skip(self))]
    pub async fn record_authentication(&self, issuer: &str, subject: &str, authenticated_ts: i64) -> Result<(), ApiError> {
        sqlx::query!(
            "UPDATE oidc_user_mapping SET last_authenticated_ts = $1, authentication_count = authentication_count + 1 WHERE issuer = $2 AND subject = $3",
            authenticated_ts,
            issuer,
            subject
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update OIDC user mapping", &e))?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn create_mapping(
        &self,
        issuer: &str,
        subject: &str,
        user_id: &str,
        first_seen_ts: i64,
    ) -> Result<(), ApiError> {
        sqlx::query!(
            "INSERT INTO oidc_user_mapping (issuer, subject, user_id, first_seen_ts, last_authenticated_ts, authentication_count) VALUES ($1, $2, $3, $4, $4, 1)",
            issuer,
            subject,
            user_id,
            first_seen_ts
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to insert OIDC user mapping", &e))?;
        Ok(())
    }
}
