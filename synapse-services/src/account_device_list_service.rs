use std::collections::HashMap;

use synapse_common::error::ApiError;
use synapse_storage::{Device, DeviceStorage};

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
    device_storage: DeviceStorage,
}

impl AccountDeviceListService {
    pub fn new(device_storage: DeviceStorage) -> Self {
        Self { device_storage }
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
