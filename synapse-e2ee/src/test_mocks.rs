//! Pre-positioned Mock adapter for the e2ee layer.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::device_keys::models::DeviceKey;
use crate::device_keys::storage::DeviceKeyStoreApi;
use crate::key_rotation::KeyRotationStorageApi;
use std::collections::HashSet;
use synapse_common::ApiError;

type RotationHistoryEntry = Vec<(Option<String>, Option<i64>)>;

/// In-memory test double for [`KeyRotationStorageApi`].
#[derive(Clone, Default)]
pub struct InMemoryKeyRotationStorage {
    last_rotation_ts: Arc<RwLock<HashMap<String, i64>>>,
    rotation_history: Arc<RwLock<HashMap<String, RotationHistoryEntry>>>,
    config: Arc<RwLock<HashMap<String, String>>>,
    key_rotation_ts: Arc<RwLock<HashMap<String, i64>>>,
    marked_rotations: Arc<RwLock<Vec<(String, String)>>>,
}

impl InMemoryKeyRotationStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn seed_last_rotation_ts(&self, user_id: &str, ts: i64) {
        self.last_rotation_ts.write().await.insert(user_id.to_string(), ts);
    }

    pub async fn seed_device_history(
        &self,
        user_id: &str,
        device_id: &str,
        history: Vec<(Option<String>, Option<i64>)>,
    ) {
        let key = format!("{}:{}", user_id, device_id);
        self.rotation_history.write().await.insert(key, history);
    }

    /// Return the `(room_id, leaving_user_id)` pairs recorded via
    /// [`KeyRotationStorageApi::mark_key_rotation_needed`].
    pub async fn marked_rotations(&self) -> Vec<(String, String)> {
        self.marked_rotations.read().await.clone()
    }
}

#[async_trait::async_trait]
impl KeyRotationStorageApi for InMemoryKeyRotationStorage {
    async fn get_user_last_rotation_ts(&self, user_id: &str) -> Result<Option<i64>, ApiError> {
        Ok(self.last_rotation_ts.read().await.get(user_id).copied())
    }

    async fn get_device_rotation_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<(Option<String>, Option<i64>)>, ApiError> {
        let key = format!("{}:{}", user_id, device_id);
        Ok(self.rotation_history.read().await.get(&key).cloned().unwrap_or_default())
    }

    async fn set_rotation_config(&self, key: &str, value: &str) -> Result<(), ApiError> {
        self.config.write().await.insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn get_rotation_config(&self, key: &str) -> Result<Option<String>, ApiError> {
        Ok(self.config.read().await.get(key).cloned())
    }

    async fn get_last_rotation_for_key(&self, user_id: &str, key_id: &str) -> Result<Option<i64>, ApiError> {
        let lookup = format!("{}:{}", user_id, key_id);
        Ok(self.key_rotation_ts.read().await.get(&lookup).copied())
    }

    async fn get_max_rotation_ts(&self, user_id: &str) -> Result<i64, ApiError> {
        Ok(self.last_rotation_ts.read().await.get(user_id).copied().unwrap_or(0))
    }

    async fn mark_key_rotation_needed(&self, room_id: &str, leaving_user_id: &str) -> Result<(), ApiError> {
        self.marked_rotations.write().await.push((room_id.to_string(), leaving_user_id.to_string()));
        Ok(())
    }
}

// =============================================================================
// InMemoryDeviceKeyStore — mock for DeviceKeyStoreApi
// =============================================================================

/// In-memory test double for [`DeviceKeyStoreApi`].
///
/// Mirrors the semantics of [`crate::device_keys::DeviceKeyStorage`] without
/// touching PostgreSQL. Falls back to empty results for queries that depend
/// on cross-table joins (e.g. `get_key_changes_with_left`'s "left" list).
#[derive(Clone, Default)]
pub struct InMemoryDeviceKeyStore {
    /// `(user_id, device_id, key_id)` → `DeviceKey`
    #[allow(clippy::type_complexity)]
    keys: Arc<RwLock<HashMap<(String, String, String), DeviceKey>>>,
    /// Tracks which `key_id`s are fallback keys (`DeviceKey` has no `is_fallback` field)
    fallback_key_ids: Arc<RwLock<HashSet<String>>>,
    /// device_lists_stream entries (for `get_key_changes_with_left`)
    device_list_stream: Arc<RwLock<Vec<DeviceListStreamEntry>>>,
    /// `(target_user_id, target_key_id, signing_user_id, signing_key_id)` → signature
    #[allow(clippy::type_complexity)]
    signatures: Arc<RwLock<HashMap<(String, String, String, String), String>>>,
}

#[derive(Clone)]
struct DeviceListStreamEntry {
    stream_id: i64,
    user_id: String,
    #[allow(dead_code)]
    device_id: Option<String>,
    #[allow(dead_code)]
    created_ts: i64,
}

impl InMemoryDeviceKeyStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a device key directly into the store (bypasses `create_device_key`).
    pub async fn seed_key(&self, key: DeviceKey) {
        let k = (key.user_id.clone(), key.device_id.clone(), key.key_id.clone());
        self.keys.write().await.insert(k, key);
    }

    /// Mark a `key_id` as a fallback key.
    pub async fn seed_fallback(&self, key_id: &str) {
        self.fallback_key_ids.write().await.insert(key_id.to_string());
    }
}

#[async_trait::async_trait]
impl DeviceKeyStoreApi for InMemoryDeviceKeyStore {
    async fn record_device_list_change_best_effort(&self, user_id: &str, device_id: Option<&str>, change_type: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        let mut stream = self.device_list_stream.write().await;
        let stream_id = (stream.len() + 1) as i64;
        stream.push(DeviceListStreamEntry {
            stream_id,
            user_id: user_id.to_string(),
            device_id: device_id.map(String::from),
            created_ts: now,
        });
        let _ = change_type;
    }

    async fn create_device_key(&self, key: &DeviceKey) -> Result<(), ApiError> {
        let k = (key.user_id.clone(), key.device_id.clone(), key.key_id.clone());
        let mut key_copy = key.clone();
        key_copy.updated_ts = chrono::Utc::now();
        self.keys.write().await.insert(k, key_copy);
        Ok(())
    }

    async fn create_fallback_key(&self, key: &DeviceKey) -> Result<(), ApiError> {
        let k = (key.user_id.clone(), key.device_id.clone(), key.key_id.clone());
        let mut key_copy = key.clone();
        key_copy.updated_ts = chrono::Utc::now();
        self.keys.write().await.insert(k, key_copy);
        self.fallback_key_ids.write().await.insert(key.key_id.clone());
        Ok(())
    }

    async fn delete_fallback_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        let mut keys = self.keys.write().await;
        let mut fallback_ids = self.fallback_key_ids.write().await;
        let to_remove: Vec<(String, String, String)> = keys
            .keys()
            .filter(|(uid, did, kid)| uid == user_id && did == device_id && fallback_ids.contains(kid))
            .cloned()
            .collect();
        for k in to_remove {
            keys.remove(&k);
            fallback_ids.remove(&k.2);
        }
        Ok(())
    }

    async fn get_unused_fallback_key_types(&self, user_id: &str, device_id: &str) -> Result<Vec<String>, ApiError> {
        let keys = self.keys.read().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        let mut algos: Vec<String> = keys
            .iter()
            .filter(|((uid, did, kid), _)| uid == user_id && did == device_id && fallback_ids.contains(kid))
            .map(|(_, key)| {
                if key.algorithm.starts_with("signed_curve25519") {
                    "signed_curve25519".to_string()
                } else {
                    key.algorithm.clone()
                }
            })
            .collect();
        algos.sort();
        algos.dedup();
        Ok(algos)
    }

    async fn get_device_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<DeviceKey>, ApiError> {
        let keys = self.keys.read().await;
        Ok(keys
            .iter()
            .find(|((uid, did, _), key)| uid == user_id && did == device_id && key.algorithm == algorithm)
            .map(|(_, key)| key.clone()))
    }

    async fn get_device_keys(&self, user_id: &str, device_ids: &[String]) -> Result<Vec<DeviceKey>, ApiError> {
        let keys = self.keys.read().await;
        Ok(keys
            .iter()
            .filter(|((uid, did, _), _)| uid == user_id && device_ids.contains(did))
            .map(|(_, key)| key.clone())
            .collect())
    }

    async fn get_all_device_keys(&self, user_id: &str) -> Result<Vec<DeviceKey>, ApiError> {
        let keys = self.keys.read().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        Ok(keys
            .iter()
            .filter(|((uid, _, kid), key)| {
                uid == user_id
                    && !fallback_ids.contains(kid)
                    && (key.algorithm == "ed25519" || key.algorithm == "curve25519")
            })
            .map(|(_, key)| key.clone())
            .collect())
    }

    async fn get_all_device_keys_batch(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, Vec<DeviceKey>>, ApiError> {
        let keys = self.keys.read().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        let mut result: HashMap<String, Vec<DeviceKey>> = HashMap::new();
        for ((uid, _, kid), key) in keys.iter() {
            if user_ids.contains(uid)
                && !fallback_ids.contains(kid)
                && (key.algorithm == "ed25519" || key.algorithm == "curve25519")
            {
                result.entry(uid.clone()).or_default().push(key.clone());
            }
        }
        Ok(result)
    }

    async fn delete_device_key(&self, user_id: &str, device_id: &str, algorithm: &str) -> Result<(), ApiError> {
        let mut keys = self.keys.write().await;
        let to_remove: Vec<(String, String, String)> = keys
            .iter()
            .filter(|((uid, did, _), key)| uid == user_id && did == device_id && key.algorithm == algorithm)
            .map(|(k, _)| k.clone())
            .collect();
        for k in to_remove {
            keys.remove(&k);
        }
        Ok(())
    }

    async fn get_device_count(&self, user_id: &str) -> Result<i64, ApiError> {
        let keys = self.keys.read().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        let count = keys
            .iter()
            .filter(|((uid, _, kid), _)| uid == user_id && !fallback_ids.contains(kid))
            .map(|((_, did, _), _)| did.clone())
            .collect::<HashSet<_>>()
            .len();
        Ok(count as i64)
    }

    async fn get_device_counts_batch(&self, user_ids: &[String]) -> Result<HashMap<String, i64>, sqlx::Error> {
        let keys = self.keys.read().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        let mut counts: HashMap<String, i64> = HashMap::new();
        for uid in user_ids {
            let count = keys
                .iter()
                .filter(|((u, _, kid), _)| u == uid && !fallback_ids.contains(kid))
                .map(|((_, did, _), _)| did.clone())
                .collect::<HashSet<_>>()
                .len();
            counts.insert(uid.clone(), count as i64);
        }
        Ok(counts)
    }

    async fn delete_device_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        let mut keys = self.keys.write().await;
        let to_remove: Vec<(String, String, String)> =
            keys.keys().filter(|(uid, did, _)| uid == user_id && did == device_id).cloned().collect();
        for k in to_remove {
            keys.remove(&k);
        }
        Ok(())
    }

    async fn get_one_time_keys_count(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let keys = self.keys.read().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        let count = keys
            .iter()
            .filter(|((uid, did, _), key)| {
                uid == user_id
                    && did == device_id
                    && !fallback_ids.contains(&key.key_id)
                    && key.algorithm.starts_with("signed_curve25519")
            })
            .count();
        Ok(count as i64)
    }

    async fn get_one_time_keys_count_by_algorithm(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<std::collections::HashMap<String, i64>, ApiError> {
        let keys = self.keys.read().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        let mut counts = std::collections::HashMap::new();
        for ((uid, did, _), key) in keys.iter() {
            if uid == user_id && did == device_id && !fallback_ids.contains(&key.key_id) {
                if key.algorithm == "ed25519" || key.algorithm == "curve25519" {
                    continue;
                }
                let algo_name = if key.algorithm.starts_with("signed_curve25519") {
                    "signed_curve25519".to_string()
                } else if key.algorithm.starts_with("curve25519") {
                    "curve25519".to_string()
                } else {
                    key.algorithm.clone()
                };
                *counts.entry(algo_name).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    async fn claim_one_time_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<DeviceKey>, ApiError> {
        let mut keys = self.keys.write().await;
        let fallback_ids = self.fallback_key_ids.read().await;
        // Find a non-fallback OTK matching the algorithm
        let otk_key = keys
            .iter()
            .find(|((uid, did, kid), key)| {
                uid == user_id && did == device_id && !fallback_ids.contains(kid) && key.algorithm == algorithm
            })
            .map(|(k, _)| k.clone());
        if let Some(k) = otk_key {
            if let Some(key) = keys.remove(&k) {
                return Ok(Some(key));
            }
        }
        // Fall back to fallback key (not consumed, matching Postgres behavior)
        let fb_key = keys
            .iter()
            .find(|((uid, did, kid), key)| {
                uid == user_id && did == device_id && fallback_ids.contains(kid) && key.algorithm == algorithm
            })
            .map(|(k, _)| k.clone());
        if let Some(k) = fb_key {
            if let Some(key) = keys.get(&k) {
                return Ok(Some(key.clone()));
            }
        }
        Ok(None)
    }

    async fn get_key_changes(&self, from_ts: i64, to_ts: i64) -> Result<Vec<String>, ApiError> {
        let keys = self.keys.read().await;
        let mut changed: Vec<String> = keys
            .iter()
            .filter(|(_, key)| {
                let ts = key.updated_ts.timestamp_millis();
                ts > from_ts && ts <= to_ts
            })
            .map(|((uid, _, _), _)| uid.clone())
            .collect();
        changed.sort();
        changed.dedup();
        Ok(changed)
    }

    async fn get_key_changes_with_left(
        &self,
        from_ts: i64,
        to_ts: i64,
        current_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), ApiError> {
        let stream = self.device_list_stream.read().await;
        let changed: Vec<String> = stream
            .iter()
            .filter(|e| e.stream_id > from_ts && e.stream_id <= to_ts && e.user_id != current_user_id)
            .map(|e| e.user_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        // "left" requires room_memberships cross-table join; mock returns empty.
        let left: Vec<String> = Vec::new();
        Ok((changed, left))
    }

    async fn store_signature(
        &self,
        target_user_id: &str,
        target_key_id: &str,
        signing_user_id: &str,
        signing_key_id: &str,
        signature: &str,
    ) -> Result<(), ApiError> {
        let key = (
            target_user_id.to_string(),
            target_key_id.to_string(),
            signing_user_id.to_string(),
            signing_key_id.to_string(),
        );
        self.signatures.write().await.insert(key, signature.to_string());
        Ok(())
    }
}
