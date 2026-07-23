use std::collections::HashMap;

use async_trait::async_trait;
use synapse_common::error::ApiError;

use super::models::*;
use super::repository::SamlStorage;

#[async_trait]
pub trait SamlStoreApi: Send + Sync {
    async fn create_session(&self, request: CreateSamlSessionRequest) -> Result<SamlSession, ApiError>;
    async fn get_session(&self, session_id: &str) -> Result<Option<SamlSession>, ApiError>;
    async fn get_session_by_user(&self, user_id: &str) -> Result<Option<SamlSession>, ApiError>;
    async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError>;
    async fn invalidate_session(&self, session_id: &str) -> Result<(), ApiError>;
    async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError>;
    async fn create_user_mapping(&self, request: CreateSamlUserMappingRequest) -> Result<SamlUserMapping, ApiError>;
    async fn get_user_mapping_by_name_id(
        &self,
        name_id: &str,
        issuer: &str,
    ) -> Result<Option<SamlUserMapping>, ApiError>;
    async fn get_user_mapping_by_user_id(&self, user_id: &str) -> Result<Option<SamlUserMapping>, ApiError>;
    async fn delete_user_mapping(&self, name_id: &str, issuer: &str) -> Result<(), ApiError>;
    async fn list_user_mappings(&self, limit: i64, after: Option<&str>) -> Result<Vec<SamlUserMapping>, ApiError>;
    async fn get_user_mapping_any_issuer(&self, name_id: &str) -> Result<Option<SamlUserMapping>, ApiError>;
    async fn update_user_mapping_by_name_id(
        &self,
        name_id: &str,
        new_user_id: Option<&str>,
        attributes: Option<&serde_json::Value>,
    ) -> Result<Option<SamlUserMapping>, ApiError>;
    async fn delete_user_mapping_by_name_id(&self, name_id: &str) -> Result<u64, ApiError>;
    async fn create_identity_provider(
        &self,
        request: CreateSamlIdentityProviderRequest,
    ) -> Result<SamlIdentityProvider, ApiError>;
    async fn get_identity_provider(&self, entity_id: &str) -> Result<Option<SamlIdentityProvider>, ApiError>;
    async fn get_all_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError>;
    async fn get_enabled_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError>;
    async fn update_idp_metadata(
        &self,
        entity_id: &str,
        metadata_xml: &str,
        valid_until: Option<i64>,
    ) -> Result<(), ApiError>;
    async fn delete_identity_provider(&self, entity_id: &str) -> Result<(), ApiError>;
    async fn create_auth_event(&self, request: CreateSamlAuthEventRequest) -> Result<SamlAuthEvent, ApiError>;
    async fn get_auth_events_by_user(&self, user_id: &str, limit: i64) -> Result<Vec<SamlAuthEvent>, ApiError>;
    async fn create_logout_request(
        &self,
        request: CreateSamlLogoutRequestRequest,
    ) -> Result<SamlLogoutRequest, ApiError>;
    async fn get_logout_request(&self, request_id: &str) -> Result<Option<SamlLogoutRequest>, ApiError>;
    async fn process_logout_request(&self, request_id: &str) -> Result<(), ApiError>;
    async fn cleanup_old_auth_events(&self, days: i64) -> Result<u64, ApiError>;
    async fn get_all_config_overrides(&self) -> Result<HashMap<String, serde_json::Value>, ApiError>;
    async fn upsert_config_override(&self, key: &str, value: &serde_json::Value) -> Result<(), ApiError>;
    async fn delete_config_override(&self, key: &str) -> Result<(), ApiError>;
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
}
