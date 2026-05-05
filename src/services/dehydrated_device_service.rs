use crate::common::ApiError;
use crate::storage::{DehydratedDevice, DehydratedDeviceStorage, UpsertDehydratedDeviceParams};
use serde_json::{Map, Value};

#[derive(Clone)]
pub struct DehydratedDeviceService {
    storage: DehydratedDeviceStorage,
}

impl DehydratedDeviceService {
    pub fn new(storage: DehydratedDeviceStorage) -> Self {
        Self { storage }
    }

    pub async fn get_device(&self, user_id: &str) -> Result<Option<Value>, ApiError> {
        let record =
            self.storage.get_by_user(user_id).await.map_err(|e| {
                ApiError::internal(format!("Failed to load dehydrated device: {}", e))
            })?;

        Ok(record.map(Self::record_to_response))
    }

    pub async fn put_device(&self, user_id: &str, body: Value) -> Result<String, ApiError> {
        let mut payload = body
            .as_object()
            .cloned()
            .ok_or_else(|| ApiError::bad_request("Expected a JSON object body"))?;
        let existing_device_id = self.existing_device_id(user_id).await;

        let device_id = payload
            .get("device_id")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
            .or(existing_device_id)
            .unwrap_or_else(Self::generate_device_id);

        payload.insert("device_id".to_string(), Value::String(device_id.clone()));

        let algorithm = payload
            .get("algorithm")
            .and_then(|value| value.as_str())
            .unwrap_or("org.matrix.msc3814.v1.olm")
            .to_string();
        let account = payload.get("account").cloned();
        let expires_at = payload.get("expires_at").and_then(Value::as_i64);

        self.storage
            .upsert_for_user(UpsertDehydratedDeviceParams {
                user_id: user_id.to_string(),
                device_id: device_id.clone(),
                device_data: Value::Object(payload),
                algorithm,
                account,
                expires_at,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Failed to store dehydrated device: {}", e)))?;

        Ok(device_id)
    }

    async fn existing_device_id(&self, user_id: &str) -> Option<String> {
        self.storage
            .get_by_user(user_id)
            .await
            .ok()
            .flatten()
            .map(|record| record.device_id)
    }

    pub async fn delete_device(&self, user_id: &str) -> Result<bool, ApiError> {
        let rows = self.storage.delete_by_user(user_id).await.map_err(|e| {
            ApiError::internal(format!("Failed to delete dehydrated device: {}", e))
        })?;
        Ok(rows > 0)
    }

    /// Background sweep: deletes expired dehydrated devices (and their pending
    /// to-device messages). Returns the number of devices removed.
    pub async fn sweep_expired(&self) -> Result<u64, ApiError> {
        self.storage.sweep_expired().await.map_err(|e| {
            ApiError::internal(format!("Failed to sweep expired dehydrated devices: {}", e))
        })
    }

    /// Claim a batch of to-device events addressed to a dehydrated device.
    ///
    /// Validates that the dehydrated device exists for `user_id` and matches
    /// `device_id`, then returns the next page of pending to-device events
    /// after `next_batch` (interpreted as a `stream_id` cursor; empty/missing
    /// strings start from the beginning).
    pub async fn claim_events(
        &self,
        user_id: &str,
        device_id: &str,
        next_batch: Option<&str>,
        limit: i64,
    ) -> Result<Value, ApiError> {
        let record = self
            .storage
            .get_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load dehydrated device: {}", e)))?
            .ok_or_else(|| ApiError::not_found("No dehydrated device for this user"))?;

        if record.device_id != device_id {
            return Err(ApiError::not_found(
                "Dehydrated device id does not match the stored device",
            ));
        }

        let since: i64 = next_batch
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| ApiError::bad_request("Invalid next_batch cursor"))?
            .unwrap_or(0);

        let limit = limit.clamp(1, 100);

        let (events, max_stream_id) = self
            .storage
            .claim_to_device_events(user_id, device_id, since, limit)
            .await
            .map_err(|e| {
                ApiError::internal(format!(
                    "Failed to fetch to-device events for dehydrated device: {}",
                    e
                ))
            })?;

        Ok(serde_json::json!({
            "events": events,
            "next_batch": max_stream_id.to_string(),
        }))
    }

    fn generate_device_id() -> String {
        format!(
            "DEHYDRATED{}",
            uuid::Uuid::new_v4().simple().to_string()[..10].to_ascii_uppercase()
        )
    }

    fn record_to_response(record: DehydratedDevice) -> Value {
        let device_id = record.device_id.clone();
        let algorithm = record.algorithm.clone();
        let account = record.account.clone();
        let expires_at = record.expires_at;

        match record.device_data {
            Value::Object(mut map) => {
                Self::ensure_response_fields(
                    &mut map,
                    &device_id,
                    &algorithm,
                    account.as_ref(),
                    expires_at,
                );
                Value::Object(map)
            }
            other => {
                let mut map = Map::new();
                map.insert("device_data".to_string(), other);
                Self::ensure_response_fields(
                    &mut map,
                    &device_id,
                    &algorithm,
                    account.as_ref(),
                    expires_at,
                );
                Value::Object(map)
            }
        }
    }

    fn ensure_response_fields(
        map: &mut Map<String, Value>,
        device_id: &str,
        algorithm: &str,
        account: Option<&Value>,
        expires_at: Option<i64>,
    ) {
        map.insert(
            "device_id".to_string(),
            Value::String(device_id.to_string()),
        );
        map.entry("algorithm".to_string())
            .or_insert_with(|| Value::String(algorithm.to_string()));
        if let Some(account) = account {
            map.entry("account".to_string())
                .or_insert_with(|| account.clone());
        }
        if let Some(expires_at) = expires_at {
            map.entry("expires_at".to_string())
                .or_insert_with(|| Value::from(expires_at));
        }
    }
}
