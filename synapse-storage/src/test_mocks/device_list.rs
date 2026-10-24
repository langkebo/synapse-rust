use super::*;
use crate::device::{Device, DeviceListStoreApi, LazyLoadedMemberRow};
use synapse_common::current_timestamp_millis;

/// In-memory lazy-loaded members:
/// `(user_id, device_id, room_id) → (member_user_id → last_sent_position)`.
///
/// The value mirrors the real table's `last_sent_position` column, so the sync
/// layer's position-based pruning is exercised by the in-memory store too.
type LazyLoadedMembersMap = HashMap<(String, String, String), HashMap<String, Option<i64>>>;

/// In-memory device list store mirroring [`crate::device::DeviceStorage`].
///
/// Stores devices in a `HashMap<device_id, Device>` and tracks stream
/// position with a monotonically increasing counter.
#[derive(Clone, Default)]
pub struct InMemoryDeviceListStore {
    devices: Arc<tokio::sync::RwLock<HashMap<String, Device>>>,
    stream_id: Arc<tokio::sync::RwLock<i64>>,
    /// (user_id, device_id, room_id) → member_user_id → last_sent_position.
    lazy_loaded_members: Arc<tokio::sync::RwLock<LazyLoadedMembersMap>>,
}

impl InMemoryDeviceListStore {
    /// See [`new`].
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
        let now = current_timestamp_millis();
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
        // ⚠️ SIMPLIFIED STUB — NOT behaviourally equivalent to storage.
        //
        // The real implementation (device/mod.rs) selects users whose device-list
        // changed inside a stream window, excluding the requester, ordered and
        // capped:
        //     SELECT DISTINCT user_id … WHERE stream_id > $1 AND stream_id <= $2
        //       AND user_id <> $3 ORDER BY user_id LIMIT 100
        //
        // This mock ignores `from`/`to`/`requester_id` and returns EVERY user that
        // has a device, unordered and uncapped. It therefore cannot be used to test
        // stream-window semantics (which changes a `/sync` result reports as
        // changed), the requester exclusion, the ordering, or the LIMIT 100 cap —
        // and it will never surface a drift in any of them.
        //
        // Why it is not fixed here: the store keeps a single global `stream_id`
        // counter and no per-device stream position, so honouring the window needs a
        // data-model change (record the stream position at device write time). That
        // is a deliberate, larger task. See docs/audit/P5_engineering_2026-09-11.md.
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
    ) -> Result<Vec<LazyLoadedMemberRow>, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), room_id.to_string());
        let stored = self.lazy_loaded_members.read().await.get(&key).cloned().unwrap_or_default();
        Ok(stored
            .into_iter()
            .map(|(member_user_id, last_sent_position)| LazyLoadedMemberRow { member_user_id, last_sent_position })
            .collect())
    }

    async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        members: &[LazyLoadedMemberRow],
    ) -> Result<u64, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), room_id.to_string());
        let mut store = self.lazy_loaded_members.write().await;
        let entry = store.entry(key).or_default();
        let mut affected = 0u64;
        for member in members {
            // Same rule as the real store: a member without a position cannot
            // prove anything, so it is not stored at all.
            let Some(position) = member.last_sent_position else {
                continue;
            };
            // Mirrors the real `ON CONFLICT … DO UPDATE`: an existing row is
            // rewritten (new position) and does not count as a new row.
            if entry.insert(member.member_user_id.clone(), Some(position)).is_none() {
                affected += 1;
            }
        }
        Ok(affected)
    }

    async fn record_device_list_change(
        &self,
        _user_id: &str,
        _device_id: Option<&str>,
        _change_type: &str,
    ) -> Result<i64, sqlx::Error> {
        // 与真实实现同形：分配一个单调递增的**本地**序号并返回。
        // 不返回 0，也不用 `current_timestamp_millis()` —— 后者正是 NEW-07 的原始缺陷
        // （对端毫秒时间戳混进本地序列），mock 照抄会让用例替它背书。
        let mut sid = self.stream_id.write().await;
        *sid += 1;
        Ok(*sid)
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

    async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let devices = self.devices.read().await;
        Ok(devices.values().filter(|device| device.user_id == user_id).count() as i64)
    }
}
