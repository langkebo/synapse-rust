use synapse_common::ApiError;
use synapse_storage::oidc_user_mapping::OidcUserMappingStorage;
use tracing::instrument;

#[derive(Clone)]
pub struct OidcMappingService {
    storage: OidcUserMappingStorage,
}

impl OidcMappingService {
    pub fn new(storage: OidcUserMappingStorage) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .get_bound_user_id(issuer, subject)
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
        self.storage
            .update_last_authenticated(issuer, subject, authenticated_ts)
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
        self.storage
            .insert_mapping(issuer, subject, user_id, first_seen_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to insert OIDC user mapping", &e))?;
        Ok(())
    }
}
