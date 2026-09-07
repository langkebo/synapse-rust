use crate::auth::{CredentialAuth, TokenAuth};
use crate::uia_service::UiaService;
use crate::UserService;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::error::ApiError;
#[cfg(feature = "privacy-ext")]
use synapse_common::map_database;
use synapse_storage::{ThreepidStoreApi, User, UserThreepid};

/// The `AccountIdentityService` struct.
#[derive(Clone)]
pub struct AccountIdentityService {
    user_service: Arc<UserService>,
    threepid_storage: Arc<dyn ThreepidStoreApi>,
    #[cfg(feature = "privacy-ext")]
    privacy_storage: Arc<dyn synapse_storage::privacy::PrivacyStoreApi>,
}

impl AccountIdentityService {
    /// See [`new`].
    #[cfg(feature = "privacy-ext")]
    pub fn new(
        user_service: Arc<UserService>,
        threepid_storage: Arc<dyn ThreepidStoreApi>,
        privacy_storage: Arc<dyn synapse_storage::privacy::PrivacyStoreApi>,
    ) -> Self {
        Self { user_service, threepid_storage, privacy_storage }
    }

    /// See [`new`].
    #[cfg(not(feature = "privacy-ext"))]
    pub fn new(user_service: Arc<UserService>, threepid_storage: Arc<dyn ThreepidStoreApi>) -> Self {
        Self { user_service, threepid_storage }
    }

    /// See [`can_view_profile_for_requester_batch`].
    #[cfg(feature = "privacy-ext")]
    pub async fn can_view_profile_for_requester_batch(
        &self,
        requester_id: Option<&str>,
        user_ids: &[String],
    ) -> Result<HashMap<String, bool>, ApiError> {
        self.privacy_storage
            .batch_can_view_profile(requester_id, user_ids)
            .await
            .map_err(map_database!("can_view_profile_for_requester_batch"))
    }

    /// See [`can_view_profile_for_requester_batch`].
    #[cfg(not(feature = "privacy-ext"))]
    pub async fn can_view_profile_for_requester_batch(
        &self,
        _requester_id: Option<&str>,
        user_ids: &[String],
    ) -> Result<HashMap<String, bool>, ApiError> {
        Ok(user_ids.iter().cloned().map(|user_id| (user_id, true)).collect())
    }

    /// See [`ensure_active_user_exists`].
    pub async fn ensure_active_user_exists(&self, user_id: &str) -> Result<(), ApiError> {
        let user_exists = self.user_service.user_exists(user_id).await?;
        if !user_exists {
            return Err(ApiError::not_found("User not found".to_string()));
        }
        Ok(())
    }

    /// See [`user_exists`].
    pub async fn user_exists(&self, user_id: &str) -> Result<bool, ApiError> {
        self.user_service.user_exists(user_id).await
    }

    /// See [`get_user_by_id`].
    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, ApiError> {
        self.user_service.get_user(user_id).await
    }

    /// See [`get_user_by_identifier`].
    pub async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, ApiError> {
        self.user_service.get_user_by_identifier(identifier).await
    }

    /// See [`get_user_by_username`].
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, ApiError> {
        self.user_service.get_user_by_username(username).await
    }

    /// See [`search_users`].
    pub async fn search_users(
        &self,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<synapse_storage::UserSearchResult>, ApiError> {
        self.user_service.search_users(search_term, limit).await
    }

    /// See [`get_users_paginated`].
    pub async fn get_users_paginated(
        &self,
        limit: i64,
        created_ts_cursor: Option<i64>,
        user_id_cursor: Option<&str>,
    ) -> Result<Vec<User>, ApiError> {
        self.user_service.get_users_paginated(limit, created_ts_cursor, user_id_cursor).await
    }

    /// See [`get_user_count`].
    pub async fn get_user_count(&self) -> Result<i64, ApiError> {
        self.user_service.get_user_count().await
    }

    /// See [`get_non_deactivated_user_count`].
    pub async fn get_non_deactivated_user_count(&self) -> Result<i64, ApiError> {
        self.user_service.get_non_deactivated_user_count().await
    }

    /// See [`get_non_deactivated_user_count_by_app_service`].
    pub async fn get_non_deactivated_user_count_by_app_service(&self) -> Result<HashMap<String, i64>, ApiError> {
        self.user_service.get_non_deactivated_user_count_by_app_service().await
    }

    /// See [`search_directory_users`].
    #[tracing::instrument(skip(self))]
    pub async fn search_directory_users(
        &self,
        search_term: &str,
        limit: i64,
        exact_only: bool,
    ) -> Result<Vec<synapse_storage::UserDirectorySearchResult>, ApiError> {
        self.user_service.search_directory_users(search_term, limit, exact_only).await
    }

    /// See [`get_daily_active_users`].
    #[tracing::instrument(skip(self))]
    pub async fn get_daily_active_users(&self) -> Result<i64, ApiError> {
        self.user_service
            .store()
            .get_daily_active_users()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get daily active users", &e))
    }

    /// See [`get_monthly_active_users`].
    #[tracing::instrument(skip(self))]
    pub async fn get_monthly_active_users(&self) -> Result<i64, ApiError> {
        self.user_service
            .store()
            .get_monthly_active_users()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get monthly active users", &e))
    }

    /// See [`get_r30_users`].
    #[tracing::instrument(skip(self))]
    pub async fn get_r30_users(&self) -> Result<i64, ApiError> {
        self.user_service
            .store()
            .get_r30_users()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get r30 users", &e))
    }

    /// See [`resolve_password_reset_user_id_by_email`].
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

    /// See [`require_deactivate_account_uia`].
    pub async fn require_deactivate_account_uia(
        &self,
        uia_service: &UiaService,
        auth: Option<&Value>,
        user_id: &str,
        token_auth: &Arc<dyn TokenAuth>,
        credential_auth: &Arc<dyn CredentialAuth>,
    ) -> Result<(), Value> {
        uia_service
            .require_uia(
                auth,
                user_id,
                UiaService::get_deactivate_account_flows(),
                token_auth,
                credential_auth,
                &*self.threepid_storage,
            )
            .await
    }

    /// See [`require_cross_signing_uia`].
    pub async fn require_cross_signing_uia(
        &self,
        uia_service: &UiaService,
        auth: Option<&Value>,
        user_id: &str,
        token_auth: &Arc<dyn TokenAuth>,
        credential_auth: &Arc<dyn CredentialAuth>,
    ) -> Result<(), Value> {
        uia_service
            .require_uia(
                auth,
                user_id,
                UiaService::get_cross_signing_flows(),
                token_auth,
                credential_auth,
                &*self.threepid_storage,
            )
            .await
    }

    /// See [`get_user_threepids`].
    pub async fn get_user_threepids(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        self.threepid_storage.get_threepids_by_user(user_id).await
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
        self.threepid_storage.add_verified_threepid(user_id, medium, address, validated_at, added_ts).await
    }

    /// See [`remove_threepid`].
    pub async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        self.threepid_storage.remove_threepid(user_id, medium, address).await
    }

    async fn lookup_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        self.user_service.store().get_user_by_email(email).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::current_timestamp_millis;
    use synapse_storage::test_mocks::{shared_fake_user_store, InMemoryThreepidStore};

    fn make_service(threepid_store: Arc<InMemoryThreepidStore>) -> AccountIdentityService {
        let user_store = shared_fake_user_store();
        let user_service = Arc::new(crate::UserService::new(user_store));
        #[cfg(feature = "privacy-ext")]
        {
            let pool = sqlx::PgPool::connect_lazy("postgresql://synapse:synapse@localhost:15432/synapse_test")
                .expect("connect_lazy should not perform I/O");
            let privacy_storage: Arc<dyn synapse_storage::privacy::PrivacyStoreApi> =
                Arc::new(synapse_storage::privacy::PrivacyStorage::new(std::sync::Arc::new(pool)));
            AccountIdentityService::new(user_service, threepid_store, privacy_storage)
        }
        #[cfg(not(feature = "privacy-ext"))]
        AccountIdentityService::new(user_service, threepid_store)
    }

    // ── ensure_active_user_exists ───────────────────────────────────

    #[tokio::test]
    async fn ensure_active_user_exists_returns_not_found() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        let err = svc.ensure_active_user_exists("@unknown:example.com").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── get_non_deactivated_user_count ──────────────────────────────

    #[tokio::test]
    async fn get_non_deactivated_user_count_returns_seeded_active_count() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        // shared_fake_user_store seeds @alice:example.com as non-deactivated.
        let count = svc.get_non_deactivated_user_count().await.expect("count should succeed");
        assert_eq!(count, 1, "default seeded store has one non-deactivated user");
    }

    // ── get_non_deactivated_user_count_by_app_service ──────────────

    #[tokio::test]
    async fn get_non_deactivated_user_count_by_app_service_groups_correctly() {
        let user_store = shared_fake_user_store();
        // Seed additional users with appservice_ids.
        // shared_fake_user_store seeds @alice:example.com (appservice_id=None, active).
        user_store.seed_user(make_as_user("@bob:example.com", false, Some("as1"))).await;
        user_store.seed_user(make_as_user("@carol:example.com", true, Some("as1"))).await;
        user_store.seed_user(make_as_user("@dave:example.com", false, Some("as2"))).await;

        let user_service = Arc::new(crate::UserService::new(user_store));
        #[cfg(feature = "privacy-ext")]
        let svc = {
            let pool = sqlx::PgPool::connect_lazy("postgresql://synapse:synapse@localhost:15432/synapse_test")
                .expect("connect_lazy should not perform I/O");
            let privacy_storage: Arc<dyn synapse_storage::privacy::PrivacyStoreApi> =
                Arc::new(synapse_storage::privacy::PrivacyStorage::new(std::sync::Arc::new(pool)));
            AccountIdentityService::new(user_service, Arc::new(InMemoryThreepidStore::new()), privacy_storage)
        };
        #[cfg(not(feature = "privacy-ext"))]
        let svc = AccountIdentityService::new(user_service, Arc::new(InMemoryThreepidStore::new()));

        let counts =
            svc.get_non_deactivated_user_count_by_app_service().await.expect("count by app service should succeed");

        // Local (alice) under empty string key; as1 has bob (carol deactivated); as2 has dave.
        assert_eq!(counts.get("").copied(), Some(1), "local: alice");
        assert_eq!(counts.get("as1").copied(), Some(1), "as1: bob (carol deactivated)");
        assert_eq!(counts.get("as2").copied(), Some(1), "as2: dave");
    }

    fn make_as_user(user_id: &str, is_deactivated: bool, appservice_id: Option<&str>) -> User {
        let mut user = User {
            user_id: user_id.to_string(),
            username: user_id.trim_start_matches('@').to_string(),
            password_hash: None,
            is_admin: false,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated,
            created_ts: 0,
            updated_ts: None,
            displayname: None,
            avatar_url: None,
            email: None,
            phone: None,
            generation: None,
            consent_version: None,
            appservice_id: None,
            user_type: None,
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: None,
            is_password_change_required: false,
            password_expires_at: None,
            failed_login_attempts: 0,
            locked_until: None,
            must_change_password: false,
        };
        user.appservice_id = appservice_id.map(|s| s.to_string());
        user
    }

    // ── user_exists ─────────────────────────────────────────────────

    #[tokio::test]
    async fn user_exists_returns_false_for_unknown() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        assert!(!svc.user_exists("@unknown:example.com").await.unwrap());
    }

    // ── get_user_by_* ───────────────────────────────────────────────

    #[tokio::test]
    async fn get_user_by_id_returns_none_for_unknown() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        assert!(svc.get_user_by_id("@unknown:example.com").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_user_by_identifier_returns_none_for_unknown() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        assert!(svc.get_user_by_identifier("@unknown:example.com").await.unwrap().is_none());
    }

    // ── resolve_password_reset_user_id_by_email ─────────────────────

    #[tokio::test]
    async fn resolve_password_reset_returns_user_id_when_threepid_found() {
        let store = Arc::new(InMemoryThreepidStore::new());
        store.seed_threepid("@alice:example.com", "email", "alice@example.com").await;
        let svc = make_service(store);
        let result = svc.resolve_password_reset_user_id_by_email("alice@example.com", "req-1").await;
        assert_eq!(result, Some("@alice:example.com".to_string()));
    }

    #[tokio::test]
    async fn resolve_password_reset_returns_none_when_no_match() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        let result = svc.resolve_password_reset_user_id_by_email("unknown@example.com", "req-1").await;
        assert_eq!(result, None);
    }

    // ── threepid passthrough methods ────────────────────────────────

    #[tokio::test]
    async fn get_user_threepids_returns_empty() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        let threepids = svc.get_user_threepids("@alice:example.com").await.unwrap();
        assert!(threepids.is_empty());
    }

    #[tokio::test]
    async fn add_verified_threepid_succeeds() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        let now = current_timestamp_millis();
        let result = svc.add_verified_threepid("@alice:example.com", "email", "alice@example.com", now, now).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn remove_threepid_returns_false_for_missing() {
        let svc = make_service(Arc::new(InMemoryThreepidStore::new()));
        let result = svc.remove_threepid("@alice:example.com", "email", "alice@example.com").await.unwrap();
        assert!(!result);
    }
}
