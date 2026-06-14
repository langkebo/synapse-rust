use crate::common::ApiError;
use crate::storage::oidc_user_mapping::OidcUserMappingStorage;
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
        OidcUserMappingStorage::get_bound_user_id(&self.pool, issuer, subject)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to query OIDC user mapping", &e))
    }

    #[instrument(skip(self))]
    pub async fn record_authentication(
        &self,
        issuer: &str,
        subject: &str,
        authenticated_ts: i64,
    ) -> Result<(), ApiError> {
        OidcUserMappingStorage::update_last_authenticated(&self.pool, issuer, subject, authenticated_ts)
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
        OidcUserMappingStorage::insert_mapping(&self.pool, issuer, subject, user_id, first_seen_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to insert OIDC user mapping", &e))?;
        Ok(())
    }
}
