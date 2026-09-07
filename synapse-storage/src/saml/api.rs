use std::collections::HashMap;

use async_trait::async_trait;
use synapse_common::error::ApiError;

use super::models::*;
use super::repository::SamlStorage;

/// The `SamlStoreApi` trait.
#[async_trait]
pub trait SamlStoreApi: Send + Sync {
    /// See [`create_session`].
    async fn create_session(&self, request: CreateSamlSessionRequest) -> Result<SamlSession, ApiError>;
    /// See [`get_session`].
    async fn get_session(&self, session_id: &str) -> Result<Option<SamlSession>, ApiError>;
    /// See [`get_session_by_user`].
    async fn get_session_by_user(&self, user_id: &str) -> Result<Option<SamlSession>, ApiError>;
    /// See [`update_session_last_used`].
    async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError>;
    /// See [`invalidate_session`].
    async fn invalidate_session(&self, session_id: &str) -> Result<(), ApiError>;
    /// See [`cleanup_expired_sessions`].
    async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError>;
    /// See [`create_user_mapping`].
    async fn create_user_mapping(&self, request: CreateSamlUserMappingRequest) -> Result<SamlUserMapping, ApiError>;
    /// See [`get_user_mapping_by_name_id`].
    async fn get_user_mapping_by_name_id(
        &self,
        name_id: &str,
        issuer: &str,
    ) -> Result<Option<SamlUserMapping>, ApiError>;
    /// See [`get_user_mapping_by_user_id`].
    async fn get_user_mapping_by_user_id(&self, user_id: &str) -> Result<Option<SamlUserMapping>, ApiError>;
    /// See [`delete_user_mapping`].
    async fn delete_user_mapping(&self, name_id: &str, issuer: &str) -> Result<(), ApiError>;
    /// See [`list_user_mappings`].
    async fn list_user_mappings(&self, limit: i64, after: Option<&str>) -> Result<Vec<SamlUserMapping>, ApiError>;
    /// See [`get_user_mapping_any_issuer`].
    async fn get_user_mapping_any_issuer(&self, name_id: &str) -> Result<Option<SamlUserMapping>, ApiError>;
    /// See [`update_user_mapping_by_name_id`].
    async fn update_user_mapping_by_name_id(
        &self,
        name_id: &str,
        new_user_id: Option<&str>,
        attributes: Option<&serde_json::Value>,
    ) -> Result<Option<SamlUserMapping>, ApiError>;
    /// See [`delete_user_mapping_by_name_id`].
    async fn delete_user_mapping_by_name_id(&self, name_id: &str) -> Result<u64, ApiError>;
    /// See [`create_identity_provider`].
    async fn create_identity_provider(
        &self,
        request: CreateSamlIdentityProviderRequest,
    ) -> Result<SamlIdentityProvider, ApiError>;
    /// See [`get_identity_provider`].
    async fn get_identity_provider(&self, entity_id: &str) -> Result<Option<SamlIdentityProvider>, ApiError>;
    /// See [`get_all_identity_providers`].
    async fn get_all_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError>;
    /// See [`get_enabled_identity_providers`].
    async fn get_enabled_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError>;
    /// See [`update_idp_metadata`].
    async fn update_idp_metadata(
        &self,
        entity_id: &str,
        metadata_xml: &str,
        valid_until: Option<i64>,
    ) -> Result<(), ApiError>;
    /// See [`delete_identity_provider`].
    async fn delete_identity_provider(&self, entity_id: &str) -> Result<(), ApiError>;
    /// See [`create_auth_event`].
    async fn create_auth_event(&self, request: CreateSamlAuthEventRequest) -> Result<SamlAuthEvent, ApiError>;
    /// See [`get_auth_events_by_user`].
    async fn get_auth_events_by_user(&self, user_id: &str, limit: i64) -> Result<Vec<SamlAuthEvent>, ApiError>;
    /// See [`create_logout_request`].
    async fn create_logout_request(
        &self,
        request: CreateSamlLogoutRequestRequest,
    ) -> Result<SamlLogoutRequest, ApiError>;
    /// See [`get_logout_request`].
    async fn get_logout_request(&self, request_id: &str) -> Result<Option<SamlLogoutRequest>, ApiError>;
    /// See [`process_logout_request`].
    async fn process_logout_request(&self, request_id: &str) -> Result<(), ApiError>;
    /// See [`cleanup_old_auth_events`].
    async fn cleanup_old_auth_events(&self, days: i64) -> Result<u64, ApiError>;
    /// See [`get_all_config_overrides`].
    async fn get_all_config_overrides(&self) -> Result<HashMap<String, serde_json::Value>, ApiError>;
    /// See [`upsert_config_override`].
    async fn upsert_config_override(&self, key: &str, value: &serde_json::Value) -> Result<(), ApiError>;
    /// See [`delete_config_override`].
    async fn delete_config_override(&self, key: &str) -> Result<(), ApiError>;
    /// See [`save_pending_request`].
    async fn save_pending_request(&self, relay_state: &str, request_id: &str, expires_at: i64) -> Result<(), ApiError>;
    /// See [`get_and_delete_pending_request`].
    async fn get_and_delete_pending_request(&self, relay_state: &str) -> Result<Option<SamlPendingRequest>, ApiError>;
}

#[async_trait]
impl SamlStoreApi for SamlStorage {
    async fn create_session(&self, request: CreateSamlSessionRequest) -> Result<SamlSession, ApiError> {
        self.create_session(request).await
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<SamlSession>, ApiError> {
        self.get_session(session_id).await
    }

    async fn get_session_by_user(&self, user_id: &str) -> Result<Option<SamlSession>, ApiError> {
        self.get_session_by_user(user_id).await
    }

    async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError> {
        self.update_session_last_used(session_id).await
    }

    async fn invalidate_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.invalidate_session(session_id).await
    }

    async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        self.cleanup_expired_sessions().await
    }

    async fn create_user_mapping(&self, request: CreateSamlUserMappingRequest) -> Result<SamlUserMapping, ApiError> {
        self.create_user_mapping(request).await
    }

    async fn get_user_mapping_by_name_id(
        &self,
        name_id: &str,
        issuer: &str,
    ) -> Result<Option<SamlUserMapping>, ApiError> {
        self.get_user_mapping_by_name_id(name_id, issuer).await
    }

    async fn get_user_mapping_by_user_id(&self, user_id: &str) -> Result<Option<SamlUserMapping>, ApiError> {
        self.get_user_mapping_by_user_id(user_id).await
    }

    async fn delete_user_mapping(&self, name_id: &str, issuer: &str) -> Result<(), ApiError> {
        self.delete_user_mapping(name_id, issuer).await
    }

    async fn list_user_mappings(&self, limit: i64, after: Option<&str>) -> Result<Vec<SamlUserMapping>, ApiError> {
        self.list_user_mappings(limit, after).await
    }

    async fn get_user_mapping_any_issuer(&self, name_id: &str) -> Result<Option<SamlUserMapping>, ApiError> {
        self.get_user_mapping_any_issuer(name_id).await
    }

    async fn update_user_mapping_by_name_id(
        &self,
        name_id: &str,
        new_user_id: Option<&str>,
        attributes: Option<&serde_json::Value>,
    ) -> Result<Option<SamlUserMapping>, ApiError> {
        self.update_user_mapping_by_name_id(name_id, new_user_id, attributes).await
    }

    async fn delete_user_mapping_by_name_id(&self, name_id: &str) -> Result<u64, ApiError> {
        self.delete_user_mapping_by_name_id(name_id).await
    }

    async fn create_identity_provider(
        &self,
        request: CreateSamlIdentityProviderRequest,
    ) -> Result<SamlIdentityProvider, ApiError> {
        self.create_identity_provider(request).await
    }

    async fn get_identity_provider(&self, entity_id: &str) -> Result<Option<SamlIdentityProvider>, ApiError> {
        self.get_identity_provider(entity_id).await
    }

    async fn get_all_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        self.get_all_identity_providers().await
    }

    async fn get_enabled_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        self.get_enabled_identity_providers().await
    }

    async fn update_idp_metadata(
        &self,
        entity_id: &str,
        metadata_xml: &str,
        valid_until: Option<i64>,
    ) -> Result<(), ApiError> {
        self.update_idp_metadata(entity_id, metadata_xml, valid_until).await
    }

    async fn delete_identity_provider(&self, entity_id: &str) -> Result<(), ApiError> {
        self.delete_identity_provider(entity_id).await
    }

    async fn create_auth_event(&self, request: CreateSamlAuthEventRequest) -> Result<SamlAuthEvent, ApiError> {
        self.create_auth_event(request).await
    }

    async fn get_auth_events_by_user(&self, user_id: &str, limit: i64) -> Result<Vec<SamlAuthEvent>, ApiError> {
        self.get_auth_events_by_user(user_id, limit).await
    }

    async fn create_logout_request(
        &self,
        request: CreateSamlLogoutRequestRequest,
    ) -> Result<SamlLogoutRequest, ApiError> {
        self.create_logout_request(request).await
    }

    async fn get_logout_request(&self, request_id: &str) -> Result<Option<SamlLogoutRequest>, ApiError> {
        self.get_logout_request(request_id).await
    }

    async fn process_logout_request(&self, request_id: &str) -> Result<(), ApiError> {
        self.process_logout_request(request_id).await
    }

    async fn cleanup_old_auth_events(&self, days: i64) -> Result<u64, ApiError> {
        self.cleanup_old_auth_events(days).await
    }

    async fn get_all_config_overrides(&self) -> Result<HashMap<String, serde_json::Value>, ApiError> {
        self.get_all_config_overrides().await
    }

    async fn upsert_config_override(&self, key: &str, value: &serde_json::Value) -> Result<(), ApiError> {
        self.upsert_config_override(key, value).await
    }

    async fn delete_config_override(&self, key: &str) -> Result<(), ApiError> {
        self.delete_config_override(key).await
    }

    async fn save_pending_request(&self, relay_state: &str, request_id: &str, expires_at: i64) -> Result<(), ApiError> {
        self.save_pending_request(relay_state, request_id, expires_at).await
    }

    async fn get_and_delete_pending_request(&self, relay_state: &str) -> Result<Option<SamlPendingRequest>, ApiError> {
        self.get_and_delete_pending_request(relay_state).await
    }
}
