use super::*;
use crate::device::{Device, DeviceListStoreApi};

/// In-memory lazy-loaded members: `(user_id, device_id, room_id) → member set`.
type LazyLoadedMembersMap = HashMap<(String, String, String), std::collections::HashSet<String>>;

/// In-memory device list store mirroring [`crate::device::DeviceStorage`].
///
/// Stores devices in a `HashMap<device_id, Device>` and tracks stream
/// position with a monotonically increasing counter.
#[derive(Clone, Default)]
pub struct InMemoryDeviceListStore {
    devices: Arc<tokio::sync::RwLock<HashMap<String, Device>>>,
    stream_id: Arc<tokio::sync::RwLock<i64>>,
    /// (user_id, device_id, room_id) → set of member user_ids.
    lazy_loaded_members: Arc<tokio::sync::RwLock<LazyLoadedMembersMap>>,
}

impl InMemoryDeviceListStore {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            stream_id: Arc::new(tokio::sync::RwLock::new(0)),
            lazy_loaded_members: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl DeviceListStoreApi for InMemoryDeviceListStore {
    async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let device = Device {
            device_id: device_id.to_string(),
            user_id: user_id.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            device_key: None,
            last_seen_ts: Some(now),
            last_seen_ip: None,
            created_ts: now,
            first_seen_ts: now,
            user_agent: None,
            appservice_id: None,
            ignored_user_list: None,
        };
        self.devices.write().await.insert(device_id.to_string(), device.clone());
        let mut sid = self.stream_id.write().await;
        *sid += 1;
        Ok(device)
    }

    async fn delete_device(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let removed = self.devices.write().await.remove(device_id);
        if removed.is_some() {
            let mut sid = self.stream_id.write().await;
            *sid += 1;
        }
        Ok(())
    }

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        let devices: Vec<Device> =
            self.devices.read().await.values().filter(|d| d.user_id == user_id).cloned().collect();
        Ok(devices)
    }

    async fn get_device(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        Ok(self.devices.read().await.get(device_id).cloned())
    }

    async fn update_user_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: &str,
    ) -> Result<u64, sqlx::Error> {
        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get_mut(device_id) {
            if device.user_id == user_id {
                device.display_name = Some(display_name.to_string());
                return Ok(1);
            }
        }
        Ok(0)
    }

    async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        Ok(*self.stream_id.read().await)
    }

    async fn get_device_list_changed_users(
        &self,
        _from: i64,
        _to: i64,
        _requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        // Simplified: return all users that have devices
        let user_ids: Vec<String> = self.devices.read().await.values().map(|d| d.user_id.clone()).collect();
        Ok(user_ids)
    }

    async fn get_device_list_left_users(
        &self,
        _from: i64,
        _to: i64,
        _requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_users_devices_batch(
        &self,
        users: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Device>>, sqlx::Error> {
        let devices = self.devices.read().await;
        let mut result: std::collections::HashMap<String, Vec<Device>> =
            users.iter().map(|id| (id.clone(), Vec::new())).collect();
        for device in devices.values() {
            if let Some(user_devices) = result.get_mut(&device.user_id) {
                user_devices.push(device.clone());
            }
        }
        Ok(result)
    }

    async fn get_device_list_changes(
        &self,
        _since: i64,
        _to: i64,
        _users: &[String],
    ) -> Result<Vec<(String, Option<String>, String, i64)>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_devices_by_user_device_pairs(
        &self,
        user_ids: &[&str],
        device_ids: &[&str],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        let devices = self.devices.read().await;
        let mut result = Vec::new();
        for (&user_id, &device_id) in user_ids.iter().zip(device_ids.iter()) {
            if let Some(device) = devices.get(device_id) {
                if device.user_id == user_id {
                    result.push((
                        user_id.to_string(),
                        device_id.to_string(),
                        device.display_name.clone(),
                        device.last_seen_ts,
                    ));
                }
            }
        }
        Ok(result)
    }

    async fn filter_existing_users(&self, users: &[String]) -> Result<Vec<String>, sqlx::Error> {
        let devices = self.devices.read().await;
        let device_user_ids: HashSet<String> = devices.values().map(|d| d.user_id.clone()).collect();
        Ok(users.iter().filter(|u| device_user_ids.contains(u.as_str())).cloned().collect())
    }

    // ── incremental device-list polling ──────────────────────────────────

    async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error> {
        Ok(*self.stream_id.read().await > since_stream_id)
    }

    async fn get_device_lists_since_with_shared_rooms(
        &self,
        _since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
        // Simplified: changed = all users with devices except exclude_user_id;
        // left = empty (no departures tracked in-memory).
        let changed: Vec<String> = self
            .devices
            .read()
            .await
            .values()
            .map(|d| d.user_id.clone())
            .filter(|u| u != exclude_user_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        Ok((changed, Vec::new()))
    }

    // ── lazy-loaded members ──────────────────────────────────────────────

    async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<HashSet<String>, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), room_id.to_string());
        Ok(self.lazy_loaded_members.read().await.get(&key).cloned().unwrap_or_default())
    }

    async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        member_user_ids: &HashSet<String>,
    ) -> Result<u64, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), room_id.to_string());
        let mut store = self.lazy_loaded_members.write().await;
        let entry = store.entry(key).or_default();
        let before = entry.len() as u64;
        for member in member_user_ids {
            entry.insert(member.clone());
        }
        Ok((entry.len() as u64).saturating_sub(before))
    }

    async fn insert_device_list_change(
        &self,
        _user_id: &str,
        _device_id: Option<&str>,
        _change_type: &str,
        _stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_max_device_list_stream_id_for_user(&self, _user_id: &str) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    async fn delete_user_devices_batch(&self, user_id: &str, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        let mut devices = self.devices.write().await;
        let mut count = 0u64;
        for device_id in device_ids {
            if let Some(device) = devices.get(device_id) {
                if device.user_id == user_id {
                    devices.remove(device_id);
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        Ok(self.devices.read().await.get(device_id).cloned())
    }

    async fn delete_device_returning_count(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get(device_id) {
            if device.user_id == user_id {
                devices.remove(device_id);
                return Ok(1);
            }
        }
        Ok(0)
    }

    async fn delete_all_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let mut devices = self.devices.write().await;
        let to_remove: Vec<String> =
            devices.iter().filter(|(_, device)| device.user_id == user_id).map(|(id, _)| id.clone()).collect();
        for device_id in to_remove {
            devices.remove(&device_id);
        }
        Ok(())
    }
}
