use std::collections::HashMap;
use std::sync::Arc;

use synapse_common::error::ApiError;
use synapse_storage::device::DeviceListStoreApi;
use synapse_storage::Device;

#[derive(Debug, Clone)]
pub struct DeviceListEntry {
    pub user_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub last_seen_ts: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DeviceListDeletion {
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceListSnapshot {
    pub changed: Vec<DeviceListEntry>,
    pub left: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceListDelta {
    pub changed: Vec<DeviceListEntry>,
    pub deleted: Vec<DeviceListDeletion>,
    pub left: Vec<String>,
    pub stream_id: i64,
}

#[derive(Clone)]
pub struct AccountDeviceListService {
    device_storage: Arc<dyn DeviceListStoreApi>,
}

impl AccountDeviceListService {
    pub fn new(device_storage: Arc<dyn DeviceListStoreApi>) -> Self {
        Self { device_storage }
    }

    pub async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, ApiError> {
        self.device_storage
            .create_device(device_id, user_id, display_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create device", &e))
    }

    pub async fn delete_device(&self, device_id: &str) -> Result<(), ApiError> {
        self.device_storage
            .delete_device(device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete device", &e))
    }

    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, ApiError> {
        self.device_storage
            .get_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get devices", &e))
    }

    pub async fn get_device(&self, device_id: &str) -> Result<Option<Device>, ApiError> {
        self.device_storage
            .get_device(device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get device", &e))
    }

    pub async fn update_user_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: &str,
    ) -> Result<u64, ApiError> {
        self.device_storage
            .update_user_device_display_name(user_id, device_id, display_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update device", &e))
    }

    pub async fn get_max_stream_id(&self) -> Result<i64, ApiError> {
        self.device_storage.get_max_device_list_stream_id().await.map_err(|e| {
            tracing::error!("Failed to get device list stream position: {e}");
            ApiError::database("Failed to get device list stream position")
        })
    }

    pub async fn get_changed_user_ids(&self, from: i64, to: i64, requester_id: &str) -> Result<Vec<String>, ApiError> {
        self.device_storage.get_device_list_changed_users(from, to, requester_id).await.map_err(|e| {
            tracing::error!("Failed to get key changes: {e}");
            ApiError::database("Failed to get key changes")
        })
    }

    pub async fn get_left_user_ids(&self, from: i64, to: i64, requester_id: &str) -> Result<Vec<String>, ApiError> {
        self.device_storage.get_device_list_left_users(from, to, requester_id).await.map_err(|e| {
            tracing::error!("Failed to get key changes left: {e}");
            ApiError::database("Failed to get key changes left")
        })
    }

    pub async fn get_device_list_snapshot(&self, users: &[String]) -> Result<DeviceListSnapshot, ApiError> {
        let devices_by_user = self.device_storage.get_users_devices_batch(users).await.map_err(|e| {
            tracing::error!("Failed to get devices: {e}");
            ApiError::database("Failed to get devices")
        })?;

        let mut snapshot = DeviceListSnapshot::default();
        for user_id in users {
            if let Some(devices) = devices_by_user.get(user_id) {
                if devices.is_empty() {
                    snapshot.left.push(user_id.clone());
                } else {
                    snapshot.changed.extend(devices.iter().map(|device| DeviceListEntry {
                        user_id: user_id.clone(),
                        device_id: device.device_id.clone(),
                        display_name: device.display_name.clone(),
                        last_seen_ts: device.last_seen_ts,
                    }));
                }
            } else {
                snapshot.left.push(user_id.clone());
            }
        }

        Ok(snapshot)
    }

    pub async fn get_device_list_delta(
        &self,
        since: i64,
        to: Option<i64>,
        users: &[String],
    ) -> Result<DeviceListDelta, ApiError> {
        let stream_id = match to {
            Some(to) if to > 0 => to,
            _ => self.get_max_stream_id().await?,
        };

        let change_rows = self.device_storage.get_device_list_changes(since, stream_id, users).await.map_err(|e| {
            tracing::error!("Failed to get device list changes: {e}");
            ApiError::database("Failed to get device list changes")
        })?;

        let mut latest: HashMap<(String, String), String> = HashMap::new();
        for (user_id, device_id, change_type, _stream_id) in change_rows {
            let Some(device_id) = device_id else {
                continue;
            };
            latest.insert((user_id, device_id), change_type);
        }

        let mut deleted = Vec::new();
        let mut active_pairs = Vec::new();
        for ((user_id, device_id), change_type) in latest {
            if change_type == "deleted" {
                deleted.push(DeviceListDeletion { user_id, device_id });
            } else {
                active_pairs.push((user_id, device_id));
            }
        }

        let mut changed = Vec::new();
        if !active_pairs.is_empty() {
            let user_ids: Vec<&str> = active_pairs.iter().map(|(user_id, _)| user_id.as_str()).collect();
            let device_ids: Vec<&str> = active_pairs.iter().map(|(_, device_id)| device_id.as_str()).collect();

            let device_rows =
                self.device_storage.get_devices_by_user_device_pairs(&user_ids, &device_ids).await.map_err(|e| {
                    tracing::error!("Failed to batch get device data: {e}");
                    ApiError::database("Failed to get device data")
                })?;

            changed.extend(device_rows.into_iter().map(|(user_id, device_id, display_name, last_seen_ts)| {
                DeviceListEntry { user_id, device_id, display_name, last_seen_ts }
            }));
        }

        let existing_users = self.device_storage.filter_existing_users(users).await.map_err(|e| {
            tracing::error!("Failed to resolve left users: {e}");
            ApiError::database("Failed to resolve left users")
        })?;
        let existing: std::collections::HashSet<String> = existing_users.into_iter().collect();

        let mut left = Vec::new();
        for user_id in users {
            if !existing.contains(user_id) {
                left.push(user_id.clone());
            }
        }

        Ok(DeviceListDelta { changed, deleted, left, stream_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::InMemoryDeviceListStore;

    fn test_service() -> AccountDeviceListService {
        AccountDeviceListService::new(Arc::new(InMemoryDeviceListStore::new()))
    }

    #[tokio::test]
    async fn create_device_returns_device_with_correct_fields() {
        let svc = test_service();
        let device = svc.create_device("DEV1", "@alice:example.com", Some("Alice Phone")).await.unwrap();
        assert_eq!(device.device_id, "DEV1");
        assert_eq!(device.user_id, "@alice:example.com");
        assert_eq!(device.display_name.as_deref(), Some("Alice Phone"));
    }

    #[tokio::test]
    async fn create_device_without_display_name_is_ok() {
        let svc = test_service();
        let device = svc.create_device("DEV2", "@bob:example.com", None).await.unwrap();
        assert!(device.display_name.is_none());
    }

    #[tokio::test]
    async fn get_device_finds_created_device() {
        let svc = test_service();
        svc.create_device("DEV3", "@alice:example.com", None).await.unwrap();
        let found = svc.get_device("DEV3").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().device_id, "DEV3");
    }

    #[tokio::test]
    async fn get_device_returns_none_for_unknown() {
        let svc = test_service();
        assert!(svc.get_device("NONEXISTENT").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_device_removes_it() {
        let svc = test_service();
        svc.create_device("DEV4", "@alice:example.com", None).await.unwrap();
        svc.delete_device("DEV4").await.unwrap();
        assert!(svc.get_device("DEV4").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_user_devices_returns_only_that_users_devices() {
        let svc = test_service();
        svc.create_device("D1", "@alice:example.com", None).await.unwrap();
        svc.create_device("D2", "@alice:example.com", None).await.unwrap();
        svc.create_device("D3", "@bob:example.com", None).await.unwrap();

        let alice_devices = svc.get_user_devices("@alice:example.com").await.unwrap();
        assert_eq!(alice_devices.len(), 2);

        let bob_devices = svc.get_user_devices("@bob:example.com").await.unwrap();
        assert_eq!(bob_devices.len(), 1);
    }

    #[tokio::test]
    async fn get_user_devices_returns_empty_for_new_user() {
        let svc = test_service();
        let devices = svc.get_user_devices("@unknown:example.com").await.unwrap();
        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn update_display_name_updates_and_returns_rows_affected() {
        let svc = test_service();
        svc.create_device("DEV5", "@alice:example.com", Some("Old")).await.unwrap();
        let rows = svc.update_user_device_display_name("@alice:example.com", "DEV5", "New").await.unwrap();
        assert_eq!(rows, 1);
        let device = svc.get_device("DEV5").await.unwrap().unwrap();
        assert_eq!(device.display_name.as_deref(), Some("New"));
    }

    #[tokio::test]
    async fn update_display_name_wrong_user_returns_zero() {
        let svc = test_service();
        svc.create_device("DEV6", "@alice:example.com", None).await.unwrap();
        let rows = svc.update_user_device_display_name("@bob:example.com", "DEV6", "Hijack").await.unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn stream_id_increments_after_create_device() {
        let svc = test_service();
        let before = svc.get_max_stream_id().await.unwrap();
        svc.create_device("DEV7", "@alice:example.com", None).await.unwrap();
        let after = svc.get_max_stream_id().await.unwrap();
        assert!(after > before);
    }

    #[tokio::test]
    async fn stream_id_increments_after_delete_device() {
        let svc = test_service();
        svc.create_device("DEV8", "@alice:example.com", None).await.unwrap();
        let before = svc.get_max_stream_id().await.unwrap();
        svc.delete_device("DEV8").await.unwrap();
        let after = svc.get_max_stream_id().await.unwrap();
        assert!(after > before);
    }

    #[tokio::test]
    async fn snapshot_groups_users_with_and_without_devices() {
        let svc = test_service();
        svc.create_device("D1", "@alice:example.com", None).await.unwrap();
        // @bob has no devices
        let snap =
            svc.get_device_list_snapshot(&["@alice:example.com".into(), "@bob:example.com".into()]).await.unwrap();
        assert!(!snap.changed.is_empty());
        assert!(snap.changed.iter().any(|e| e.user_id == "@alice:example.com"));
        assert!(snap.left.contains(&"@bob:example.com".to_string()));
    }

    #[tokio::test]
    async fn snapshot_user_with_zero_devices_is_left() {
        let svc = test_service();
        svc.create_device("D1", "@alice:example.com", None).await.unwrap();
        // A user with no devices at all
        let snap = svc.get_device_list_snapshot(&["@nobody:example.com".into()]).await.unwrap();
        assert!(snap.changed.is_empty());
        assert!(snap.left.contains(&"@nobody:example.com".to_string()));
    }

    #[tokio::test]
    async fn filter_existing_users_returns_only_users_with_devices() {
        let svc = test_service();
        svc.create_device("D1", "@alice:example.com", None).await.unwrap();
        let users: Vec<String> = vec!["@alice:example.com".into(), "@bob:example.com".into()];
        let result = svc.device_storage.filter_existing_users(&users).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "@alice:example.com");
    }
}
