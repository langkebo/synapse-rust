use super::*;

use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::Value;

use crate::dehydrated_device::{DehydratedDevice, DehydratedDeviceStoreApi, UpsertDehydratedDeviceParams};

/// In-memory [`DehydratedDeviceStoreApi`] backed by a `HashMap` keyed on
/// `user_id` (at most one dehydrated device per user, matching the production
/// delete-then-insert upsert). The to-device / one-time-key claim methods are
/// not modelled here and return empty results.
#[derive(Clone, Debug, Default)]
pub struct InMemoryDehydratedDeviceStore {
    devices: Arc<RwLock<HashMap<String, DehydratedDevice>>>,
    next_id: Arc<AtomicI64>,
}

impl InMemoryDehydratedDeviceStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl DehydratedDeviceStoreApi for InMemoryDehydratedDeviceStore {
    async fn get_by_user(&self, user_id: &str) -> Result<Option<DehydratedDevice>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        Ok(self
            .devices
            .read()
            .await
            .get(user_id)
            .filter(|device| device.expires_at.is_none_or(|expires| expires > now))
            .cloned())
    }

    async fn upsert_for_user(&self, params: UpsertDehydratedDeviceParams) -> Result<DehydratedDevice, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let record = DehydratedDevice {
            id,
            user_id: params.user_id.clone(),
            device_id: params.device_id,
            device_data: params.device_data,
            algorithm: params.algorithm,
            account: params.account,
            created_ts: now,
            updated_ts: now,
            expires_at: params.expires_at,
        };
        // Production deletes any existing rows for the user before inserting.
        self.devices.write().await.insert(params.user_id, record.clone());
        Ok(record)
    }

    async fn delete_by_user(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        Ok(if self.devices.write().await.remove(user_id).is_some() { 1 } else { 0 })
    }

    async fn sweep_expired(&self) -> Result<u64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let mut devices = self.devices.write().await;
        let before = devices.len();
        devices.retain(|_, device| device.expires_at.is_none_or(|expires| expires > now));
        Ok((before - devices.len()) as u64)
    }

    async fn claim_to_device_events(
        &self,
        _user_id: &str,
        _device_id: &str,
        since_stream_id: i64,
        _limit: i64,
    ) -> Result<(Vec<Value>, i64), sqlx::Error> {
        // `to_device_messages` is not modelled in this mock; report no new events.
        Ok((Vec::new(), since_stream_id))
    }

    async fn claim_one_time_key(
        &self,
        _user_id: &str,
        _device_id: &str,
        _algorithm: &str,
    ) -> Result<Option<(String, Value)>, sqlx::Error> {
        // One-time keys are not modelled in this mock.
        Ok(None)
    }
}
