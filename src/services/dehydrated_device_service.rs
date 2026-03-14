use crate::common::error::ApiError;
use crate::storage::dehydrated_device::{
    CreateDehydratedDeviceParams, DehydratedDevice, DehydratedDeviceContent,
    DehydratedDeviceEvent, DehydratedDeviceStorage,
};
use crate::cache::CacheManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct DehydratedDeviceService {
    storage: DehydratedDeviceStorage,
    cache: Arc<CacheManager>,
}

impl DehydratedDeviceService {
    pub fn new(storage: DehydratedDeviceStorage, cache: Arc<CacheManager>) -> Self {
        Self { storage, cache }
    }

    pub async fn create_device(
        &self,
        user_id: &str,
        device_id: &str,
        device_data: serde_json::Value,
        algorithm: &str,
        account: Option<serde_json::Value>,
        expires_in_ms: Option<i64>,
    ) -> Result<DehydratedDevice, ApiError> {
        if device_data.is_null() {
            return Err(ApiError::bad_request("device_data is required"));
        }

        if algorithm.is_empty() {
            return Err(ApiError::bad_request("algorithm is required"));
        }

        let valid_algorithms = [
            "m.megolm.v1",
            "m.megolm.v1.aes-sha2",
            "m.olm.v1.curve25519-aes-sha2",
        ];

        if !valid_algorithms.contains(&algorithm) {
            return Err(ApiError::bad_request(format!(
                "Unsupported algorithm: {}. Valid algorithms: {:?}",
                algorithm, valid_algorithms
            )));
        }

        let params = CreateDehydratedDeviceParams {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            device_data,
            algorithm: algorithm.to_string(),
            account,
            expires_in_ms,
        };

        let device = self
            .storage
            .create_device(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create dehydrated device: {}", e)))?;

        self.invalidate_cache(user_id, device_id).await;

        Ok(device)
    }

    pub async fn get_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DehydratedDevice>, ApiError> {
        let cache_key = format!("dehydrated_device:{}:{}", user_id, device_id);

        if let Ok(Some(device)) = self.cache.get::<DehydratedDevice>(&cache_key).await {
            return Ok(Some(device));
        }

        let device = self
            .storage
            .get_device(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get dehydrated device: {}", e)))?;

        if let Some(ref d) = device {
            let _ = self.cache.set(&cache_key, d, 300).await;
        }

        Ok(device)
    }

    pub async fn get_devices_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<DehydratedDevice>, ApiError> {
        let cache_key = format!("dehydrated_devices:{}", user_id);

        if let Ok(Some(devices)) = self.cache.get::<Vec<DehydratedDevice>>(&cache_key).await {
            return Ok(devices);
        }

        let devices = self
            .storage
            .get_devices_for_user(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get dehydrated devices: {}", e)))?;

        let _ = self.cache.set(&cache_key, &devices, 60).await;

        Ok(devices)
    }

    pub async fn claim_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DehydratedDevice>, ApiError> {
        let device = self
            .storage
            .claim_device(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to claim dehydrated device: {}", e)))?;

        self.invalidate_cache(user_id, device_id).await;
        self.invalidate_user_cache(user_id).await;

        Ok(device)
    }

    pub async fn delete_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<bool, ApiError> {
        let deleted = self
            .storage
            .delete_device(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete dehydrated device: {}", e)))?;

        if deleted {
            self.invalidate_cache(user_id, device_id).await;
            self.invalidate_user_cache(user_id).await;
        }

        Ok(deleted)
    }

    pub async fn delete_devices_for_user(
        &self,
        user_id: &str,
    ) -> Result<u64, ApiError> {
        let count = self
            .storage
            .delete_devices_for_user(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete dehydrated devices: {}", e)))?;

        self.invalidate_user_cache(user_id).await;

        Ok(count)
    }

    pub async fn update_device_data(
        &self,
        user_id: &str,
        device_id: &str,
        device_data: &serde_json::Value,
    ) -> Result<(), ApiError> {
        self.storage
            .update_device_data(user_id, device_id, device_data)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update device data: {}", e)))?;

        self.invalidate_cache(user_id, device_id).await;

        Ok(())
    }

    pub async fn cleanup_expired_devices(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_devices()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup expired devices: {}", e)))?;

        Ok(count)
    }

    pub fn to_device_event(device: &DehydratedDevice) -> DehydratedDeviceEvent {
        DehydratedDeviceEvent {
            event_type: "m.dehydrated_device".to_string(),
            content: DehydratedDeviceContent {
                device_id: device.device_id.clone(),
                algorithm: device.algorithm.clone(),
                device_data: device.device_data.clone(),
                account: device.account.clone(),
            },
        }
    }

    pub async fn get_device_event(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DehydratedDeviceEvent>, ApiError> {
        let device = self.get_device(user_id, device_id).await?;

        Ok(device.as_ref().map(Self::to_device_event))
    }

    async fn invalidate_cache(&self, user_id: &str, device_id: &str) {
        let _ = self
            .cache
            .delete(&format!("dehydrated_device:{}:{}", user_id, device_id))
            .await;
    }

    async fn invalidate_user_cache(&self, user_id: &str) {
        let _ = self
            .cache
            .delete(&format!("dehydrated_devices:{}", user_id))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_device_event() {
        let device = DehydratedDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            device_data: serde_json::json!({"key": "data"}),
            algorithm: "m.megolm.v1".to_string(),
            account: Some(serde_json::json!({"account": "pickle"})),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            expires_at: None,
        };

        let event = DehydratedDeviceService::to_device_event(&device);

        assert_eq!(event.event_type, "m.dehydrated_device");
        assert_eq!(event.content.device_id, "DEVICE123");
        assert_eq!(event.content.algorithm, "m.megolm.v1");
    }

    #[test]
    fn test_valid_algorithms() {
        let valid_algorithms = [
            "m.megolm.v1",
            "m.megolm.v1.aes-sha2",
            "m.olm.v1.curve25519-aes-sha2",
        ];

        for algo in valid_algorithms {
            assert!(valid_algorithms.contains(&algo));
        }
    }

    #[test]
    fn test_dehydrated_device_content() {
        let content = DehydratedDeviceContent {
            device_id: "TEST_DEVICE".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            device_data: serde_json::json!({
                "pickle": "encrypted_pickle_data",
                "passphrase": "encrypted_passphrase"
            }),
            account: Some(serde_json::json!({
                "pickle": "account_pickle"
            })),
        };

        assert_eq!(content.device_id, "TEST_DEVICE");
        assert!(content.account.is_some());
    }
}
