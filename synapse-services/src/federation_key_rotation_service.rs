use std::sync::Arc;

use synapse_common::error::ApiError;
use synapse_e2ee::key_rotation::KeyRotationStorageApi;
use synapse_federation::KeyRotationManagerApi;

#[derive(Clone)]
pub struct FederationKeyRotationService {
    manager: Arc<dyn KeyRotationManagerApi>,
    storage: Arc<dyn KeyRotationStorageApi>,
}

impl FederationKeyRotationService {
    pub fn new(manager: Arc<dyn KeyRotationManagerApi>, storage: Arc<dyn KeyRotationStorageApi>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_e2ee::test_mocks::InMemoryKeyRotationStorage;
    use synapse_federation::test_mocks::InMemoryKeyRotationManager;

    fn test_service() -> FederationKeyRotationService {
        FederationKeyRotationService::new(
            Arc::new(InMemoryKeyRotationManager::new()),
            Arc::new(InMemoryKeyRotationStorage::new()),
        )
    }

    fn test_service_with(
        manager: InMemoryKeyRotationManager,
        storage: InMemoryKeyRotationStorage,
    ) -> FederationKeyRotationService {
        FederationKeyRotationService::new(Arc::new(manager), Arc::new(storage))
    }

    // ── get_rotation_status ───────────────────────────────────────────

    #[tokio::test]
    async fn get_rotation_status_aggregates_manager_and_storage() {
        let storage = InMemoryKeyRotationStorage::new();
        storage.seed_last_rotation_ts("@alice:example.com", 1_700_000_000_000).await;
        let svc = test_service_with(InMemoryKeyRotationManager::new(), storage);

        let (status, last_rotation) = svc.get_rotation_status("@alice:example.com").await.unwrap();
        assert_eq!(last_rotation, Some(1_700_000_000_000));
        assert_eq!(status["rotation_enabled"], true);
    }

    #[tokio::test]
    async fn get_rotation_status_returns_none_when_no_rotation() {
        let svc = test_service();
        let (_status, last_rotation) = svc.get_rotation_status("@alice:example.com").await.unwrap();
        assert_eq!(last_rotation, None);
    }

    // ── rotate_keys ────────────────────────────────────────────────────

    #[tokio::test]
    async fn rotate_keys_returns_true_after_rotation() {
        let svc = test_service();
        let has_key = svc.rotate_keys(None).await.unwrap();
        assert!(has_key);
    }

    // ── get_rotation_history ──────────────────────────────────────────

    #[tokio::test]
    async fn get_rotation_history_returns_seeded_data() {
        let storage = InMemoryKeyRotationStorage::new();
        let history = vec![(Some("key1".to_string()), Some(1_700_000_000_000_i64))];
        storage.seed_device_history("@alice:example.com", "DEV1", history.clone()).await;
        let svc = test_service_with(InMemoryKeyRotationManager::new(), storage);

        let result = svc.get_rotation_history("@alice:example.com", "DEV1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.as_deref(), Some("key1"));
    }

    #[tokio::test]
    async fn get_rotation_history_returns_empty_for_unknown_device() {
        let svc = test_service();
        let result = svc.get_rotation_history("@alice:example.com", "UNKNOWN").await.unwrap();
        assert!(result.is_empty());
    }

    // ── revoke_key ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn revoke_key_returns_rows_affected() {
        let svc = test_service();
        let revoked = svc.revoke_key("ed25519:old", Some("compromised")).await.unwrap();
        assert_eq!(revoked, 1);
    }

    // ── set_rotation_enabled ──────────────────────────────────────────

    #[tokio::test]
    async fn set_rotation_enabled_updates_state() {
        let svc = test_service();
        svc.set_rotation_enabled(false).await;
        // No error = success; state is internal to the manager
    }

    // ── set/get interval_ms round trip ────────────────────────────────

    #[tokio::test]
    async fn set_and_get_interval_ms_round_trip() {
        let svc = test_service();
        assert!(svc.get_interval_ms().await.unwrap().is_none());

        svc.set_rotation_interval_ms(3600000).await.unwrap();
        assert_eq!(svc.get_interval_ms().await.unwrap(), Some(3600000));
    }

    // ── get_last_rotation_for_key ─────────────────────────────────────

    #[tokio::test]
    async fn get_last_rotation_for_key_returns_none_for_unknown() {
        let svc = test_service();
        let result = svc.get_last_rotation_for_key("@alice:example.com", "unknown_key").await.unwrap();
        assert_eq!(result, None);
    }

    // ── get_max_rotation_ts ───────────────────────────────────────────

    #[tokio::test]
    async fn get_max_rotation_ts_returns_zero_when_no_data() {
        let svc = test_service();
        let ts = svc.get_max_rotation_ts("@alice:example.com").await.unwrap();
        assert_eq!(ts, 0);
    }

    #[tokio::test]
    async fn get_max_rotation_ts_returns_seeded_value() {
        let storage = InMemoryKeyRotationStorage::new();
        storage.seed_last_rotation_ts("@alice:example.com", 1_700_000_000_000).await;
        let svc = test_service_with(InMemoryKeyRotationManager::new(), storage);

        let ts = svc.get_max_rotation_ts("@alice:example.com").await.unwrap();
        assert_eq!(ts, 1_700_000_000_000);
    }
}
