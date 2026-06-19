use crate::auth::AuthService;
use crate::uia_service::UiaService;
use serde_json::Value;
use std::collections::HashMap;
use synapse_common::error::ApiError;
use synapse_storage::{ThreepidStorage, User, UserStorage, UserThreepid};

#[derive(Clone)]
pub struct AccountIdentityService {
    user_storage: UserStorage,
    threepid_storage: ThreepidStorage,
    #[cfg(feature = "privacy-ext")]
    privacy_storage: synapse_storage::privacy::PrivacyStorage,
}

impl AccountIdentityService {
    #[cfg(feature = "privacy-ext")]
    pub fn new(
        user_storage: UserStorage,
        threepid_storage: ThreepidStorage,
        privacy_storage: synapse_storage::privacy::PrivacyStorage,
    ) -> Self {
        Self { user_storage, threepid_storage, privacy_storage }
    }

    #[cfg(not(feature = "privacy-ext"))]
    pub fn new(user_storage: UserStorage, threepid_storage: ThreepidStorage) -> Self {
        Self { user_storage, threepid_storage }
    }

    #[cfg(feature = "privacy-ext")]
    pub async fn can_view_profile_for_requester_batch(
        &self,
        requester_id: Option<&str>,
        user_ids: &[String],
    ) -> Result<HashMap<String, bool>, ApiError> {
        self.privacy_storage.batch_can_view_profile(requester_id, user_ids).await.map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    #[cfg(not(feature = "privacy-ext"))]
    pub async fn can_view_profile_for_requester_batch(
        &self,
        _requester_id: Option<&str>,
        user_ids: &[String],
    ) -> Result<HashMap<String, bool>, ApiError> {
        Ok(user_ids.iter().cloned().map(|user_id| (user_id, true)).collect())
    }

    pub async fn ensure_active_user_exists(&self, user_id: &str) -> Result<(), ApiError> {
        let user_exists = self.user_storage.user_exists(user_id).await.map_err(|e| {
            tracing::error!("Failed to check user existence: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        if !user_exists {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        Ok(())
    }

    pub async fn resolve_password_reset_user_id_by_email(&self, email: &str, request_id: &str) -> Option<String> {
        match self.threepid_storage.get_verified_threepid_by_address("email", email).await {
            Ok(Some(threepid)) => Some(threepid.user_id),
            Ok(None) => match self.lookup_user_by_email(email).await {
                Ok(user) => user.map(|u| u.user_id),
                Err(e) => {
                    tracing::warn!(
                        target: "security_audit",
                        request_id = %request_id,
                        event = "password_reset_email_lookup_failed",
                        email = %email,
                        error = %e,
                        "Failed to resolve email owner during password reset request"
                    );
                    None
                }
            },
            Err(e) => {
                tracing::warn!(
                    target: "security_audit",
                    request_id = %request_id,
                    event = "password_reset_threepid_lookup_failed",
                    email = %email,
                    error = %e,
                    "Failed to resolve verified threepid during password reset request"
                );
                None
            }
        }
    }

    pub async fn require_deactivate_account_uia(
        &self,
        uia_service: &UiaService,
        auth: Option<&Value>,
        user_id: &str,
        auth_service: &AuthService,
    ) -> Result<(), Value> {
        uia_service
            .require_uia(
                auth,
                user_id,
                UiaService::get_deactivate_account_flows(),
                auth_service,
                &self.threepid_storage,
            )
            .await
    }

    pub async fn require_cross_signing_uia(
        &self,
        uia_service: &UiaService,
        auth: Option<&Value>,
        user_id: &str,
        auth_service: &AuthService,
    ) -> Result<(), Value> {
        uia_service
            .require_uia(auth, user_id, UiaService::get_cross_signing_flows(), auth_service, &self.threepid_storage)
            .await
    }

    pub async fn get_user_threepids(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        self.threepid_storage.get_threepids_by_user(user_id).await
    }

    pub async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError> {
        self.threepid_storage.add_verified_threepid(user_id, medium, address, validated_at, added_ts).await
    }

    pub async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        self.threepid_storage.remove_threepid(user_id, medium, address).await
    }

    async fn lookup_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        self.user_storage.get_user_by_email(email).await
    }
}
