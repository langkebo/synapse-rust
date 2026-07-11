//! Pre-positioned Mock adapter for the e2ee layer.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::key_rotation::KeyRotationStorageApi;
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
