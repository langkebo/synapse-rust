//! OIDC subject ↔ Matrix user bindings (`oidc_user_mapping`).
//!
//! The OIDC provider flow needs the binding to decide between "subsequent
//! login" and "first login / localpart collision"; the route calls this narrow
//! service instead of holding the mapping store (B4-5c).

use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_storage::oidc_user_mapping::OidcUserMappingStoreApi;

/// Service over the `oidc_user_mapping` table.
pub struct OidcUserMappingService {
    storage: Arc<dyn OidcUserMappingStoreApi>,
}

impl OidcUserMappingService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn OidcUserMappingStoreApi>) -> Self {
        Self { storage }
    }

    /// Matrix user bound to `(issuer, subject)`, if any.
    pub async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .get_bound_user_id(issuer, subject)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to query OIDC user mapping", e))
    }

    /// Record a new `(issuer, subject) → user_id` binding.
    pub async fn insert_mapping(
        &self,
        issuer: &str,
        subject: &str,
        user_id: &str,
        now_ts: i64,
    ) -> Result<(), ApiError> {
        self.storage
            .insert_mapping(issuer, subject, user_id, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to insert OIDC user mapping", e))
    }

    /// Bump `last_authenticated_ts` on an existing binding.
    pub async fn update_last_authenticated(&self, issuer: &str, subject: &str, now_ts: i64) -> Result<(), ApiError> {
        self.storage
            .update_last_authenticated(issuer, subject, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update OIDC user mapping", e))
    }
}
