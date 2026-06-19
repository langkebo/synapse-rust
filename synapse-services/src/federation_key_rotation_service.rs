use std::sync::Arc;

use synapse_common::error::ApiError;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_federation::KeyRotationManager;

#[derive(Clone)]
pub struct FederationKeyRotationService {
    manager: Arc<KeyRotationManager>,
    storage: Arc<KeyRotationStorage>,
}

impl FederationKeyRotationService {
    pub fn new(manager: Arc<KeyRotationManager>, storage: Arc<KeyRotationStorage>) -> Self {
        Self { manager, storage }
    }

    pub async fn get_rotation_status(&self, user_id: &str) -> Result<(serde_json::Value, Option<i64>), ApiError> {
        let status = self.manager.get_rotation_status().await;
        let last_rotation = self.storage.get_user_last_rotation_ts(user_id).await?;
        Ok((status, last_rotation))
    }

    pub async fn rotate_keys(&self, requested_key_id: Option<String>) -> Result<bool, ApiError> {
        self.manager.rotate_keys(requested_key_id).await?;
        Ok(self.manager.get_current_key().await?.is_some())
    }

    pub async fn get_rotation_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<(Option<String>, Option<i64>)>, ApiError> {
        self.storage.get_device_rotation_history(user_id, device_id).await
    }

    pub async fn revoke_key(&self, key_id: &str, reason: Option<&str>) -> Result<u64, ApiError> {
        self.manager
            .revoke_key(key_id, reason)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to revoke key: {e}")))
    }

    pub async fn set_rotation_enabled(&self, enabled: bool) {
        self.manager.set_rotation_enabled(enabled).await;
    }

    pub async fn set_rotation_interval_ms(&self, interval_ms: i64) -> Result<(), ApiError> {
        self.storage.set_rotation_config("interval_ms", &interval_ms.to_string()).await
    }

    pub async fn set_manager_config_value(&self, key: &str, value: i64) -> Result<(), ApiError> {
        self.manager.set_rotation_config_value(key, &value.to_string()).await
    }

    pub async fn set_storage_config_value(&self, key: &str, value: i64) -> Result<(), ApiError> {
        self.storage.set_rotation_config(key, &value.to_string()).await
    }

    pub async fn get_interval_ms(&self) -> Result<Option<i64>, ApiError> {
        Ok(self.storage.get_rotation_config("interval_ms").await?.and_then(|v| v.parse().ok()))
    }

    pub async fn get_last_rotation_for_key(&self, user_id: &str, key_id: &str) -> Result<Option<i64>, ApiError> {
        self.storage.get_last_rotation_for_key(user_id, key_id).await
    }

    pub async fn get_max_rotation_ts(&self, user_id: &str) -> Result<i64, ApiError> {
        self.storage.get_max_rotation_ts(user_id).await
    }
}
