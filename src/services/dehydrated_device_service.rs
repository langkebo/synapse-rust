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
        let record = self
            .storage
            .get_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load dehydrated device", &e))?;

        Ok(record.map(Self::record_to_response))
    }

    pub async fn get_status(&self, user_id: &str) -> Result<Value, ApiError> {
        let record = self
            .storage
            .get_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load dehydrated device status", &e))?;

        if let Some(record) = record {
            Ok(serde_json::json!({
                "exists": true,
                "device_id": record.device_id,
                "algorithm": record.algorithm,
                "created_ts": record.created_ts,
                "updated_ts": record.updated_ts,
                "expires_at": record.expires_at,
            }))
        } else {
            Ok(serde_json::json!({
                "exists": false
            }))
        }
    }

    pub async fn put_device(&self, user_id: &str, body: Value) -> Result<String, ApiError> {
        let mut payload =
            body.as_object().cloned().ok_or_else(|| ApiError::bad_request("Expected a JSON object body"))?;
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
            .map_err(|e| ApiError::internal_with_log("Failed to store dehydrated device", &e))?;

        Ok(device_id)
    }

    async fn existing_device_id(&self, user_id: &str) -> Option<String> {
        self.storage.get_by_user(user_id).await.ok().flatten().map(|record| record.device_id)
    }

    pub async fn delete_device(&self, user_id: &str) -> Result<bool, ApiError> {
        let rows = self
            .storage
            .delete_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete dehydrated device", &e))?;
        Ok(rows > 0)
    }

    pub async fn delete_all_for_user(&self, user_id: &str) -> Result<(), ApiError> {
        self.storage
            .delete_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete all dehydrated devices for user", &e))?;
        Ok(())
    }

    /// Background sweep: deletes expired dehydrated devices (and their pending
    /// to-device messages). Returns the number of devices removed.
    pub async fn sweep_expired(&self) -> Result<u64, ApiError> {
        self.storage
            .sweep_expired()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to sweep expired dehydrated devices", &e))
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
            .map_err(|e| ApiError::internal_with_log("Failed to load dehydrated device", &e))?
            .ok_or_else(|| ApiError::not_found("No dehydrated device for this user"))?;

        if record.device_id != device_id {
            return Err(ApiError::not_found("Dehydrated device id does not match the stored device"));
        }

        let since: i64 = next_batch
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| ApiError::bad_request("Invalid next_batch cursor"))?
            .unwrap_or(0);

        let limit = limit.clamp(1, 100);

        let (events, max_stream_id) =
            self.storage.claim_to_device_events(user_id, device_id, since, limit).await.map_err(|e| {
                ApiError::internal_with_log("Failed to fetch to-device events for dehydrated device", &e)
            })?;

        Ok(serde_json::json!({
            "events": events,
            "next_batch": max_stream_id.to_string(),
        }))
    }

    fn generate_device_id() -> String {
        format!("DEHYDRATED{}", uuid::Uuid::new_v4().simple().to_string()[..10].to_ascii_uppercase())
    }

    fn record_to_response(record: DehydratedDevice) -> Value {
        let device_id = record.device_id.clone();
        let algorithm = record.algorithm.clone();
        let account = record.account.clone();
        let expires_at = record.expires_at;

        match record.device_data {
            Value::Object(mut map) => {
                Self::ensure_response_fields(&mut map, &device_id, &algorithm, account.as_ref(), expires_at);
                Value::Object(map)
            }
            other => {
                let mut map = Map::new();
                map.insert("device_data".to_string(), other);
                Self::ensure_response_fields(&mut map, &device_id, &algorithm, account.as_ref(), expires_at);
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
        map.insert("device_id".to_string(), Value::String(device_id.to_string()));
        map.entry("algorithm".to_string()).or_insert_with(|| Value::String(algorithm.to_string()));
        if let Some(account) = account {
            map.entry("account".to_string()).or_insert_with(|| account.clone());
        }
        if let Some(expires_at) = expires_at {
            map.entry("expires_at".to_string()).or_insert_with(|| Value::from(expires_at));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========== generate_device_id tests ==========

    #[test]
    fn test_generate_device_id_format() {
        let id = DehydratedDeviceService::generate_device_id();
        assert!(id.starts_with("DEHYDRATED"), "Device ID should start with DEHYDRATED, got: {}", id);
        assert_eq!(id.len(), 20, "Device ID should be 20 chars (DEHYDRATED + 10 hex), got: {} (len={})", id, id.len());
        // The hex part should be uppercase
        let hex_part = &id[10..];
        assert!(hex_part.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_device_id_uniqueness() {
        let id1 = DehydratedDeviceService::generate_device_id();
        let id2 = DehydratedDeviceService::generate_device_id();
        assert_ne!(id1, id2, "Two generated device IDs should be different");
    }

    // ========== ensure_response_fields tests ==========

    #[test]
    fn test_ensure_response_fields_basic() {
        let mut map = Map::new();
        DehydratedDeviceService::ensure_response_fields(&mut map, "DEV1", "m.olm.v1.curve25519-aes-sha2", None, None);
        assert_eq!(map["device_id"], json!("DEV1"));
        assert_eq!(map["algorithm"], json!("m.olm.v1.curve25519-aes-sha2"));
        assert!(!map.contains_key("account"));
        assert!(!map.contains_key("expires_at"));
    }

    #[test]
    fn test_ensure_response_fields_with_account() {
        let mut map = Map::new();
        let account = json!({"algorithms": ["m.olm.v1.curve25519-aes-sha2"]});
        DehydratedDeviceService::ensure_response_fields(
            &mut map,
            "DEV1",
            "m.olm.v1.curve25519-aes-sha2",
            Some(&account),
            None,
        );
        assert_eq!(map["account"], account);
    }

    #[test]
    fn test_ensure_response_fields_with_expires_at() {
        let mut map = Map::new();
        DehydratedDeviceService::ensure_response_fields(
            &mut map,
            "DEV1",
            "m.olm.v1.curve25519-aes-sha2",
            None,
            Some(1700000000000),
        );
        assert_eq!(map["expires_at"], json!(1700000000000_i64));
    }

    #[test]
    fn test_ensure_response_fields_does_not_overwrite_existing() {
        let mut map = Map::new();
        map.insert("algorithm".to_string(), json!("custom_algo"));
        map.insert("account".to_string(), json!("existing_account"));
        map.insert("expires_at".to_string(), json!(1600000000000_i64));

        DehydratedDeviceService::ensure_response_fields(
            &mut map,
            "DEV1",
            "m.olm.v1.curve25519-aes-sha2",
            Some(&json!("new_account")),
            Some(1700000000000),
        );
        // device_id is always overwritten
        assert_eq!(map["device_id"], json!("DEV1"));
        // algorithm should NOT be overwritten (or_insert_with)
        assert_eq!(map["algorithm"], json!("custom_algo"));
        // account should NOT be overwritten (or_insert_with)
        assert_eq!(map["account"], json!("existing_account"));
        // expires_at should NOT be overwritten (or_insert_with)
        assert_eq!(map["expires_at"], json!(1600000000000_i64));
    }

    // ========== record_to_response tests ==========

    #[test]
    fn test_record_to_response_basic() {
        let record = DehydratedDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV123".to_string(),
            device_data: json!({"key": "value"}),
            algorithm: "m.olm.v1.curve25519-aes-sha2".to_string(),
            account: None,
            created_ts: 1700000000000,
            updated_ts: 1700000001000,
            expires_at: None,
        };
        let result = DehydratedDeviceService::record_to_response(record);
        assert_eq!(result["device_id"], json!("DEV123"));
        assert_eq!(result["key"], json!("value"));
        assert_eq!(result["algorithm"], json!("m.olm.v1.curve25519-aes-sha2"));
    }

    #[test]
    fn test_record_to_response_with_account() {
        let record = DehydratedDevice {
            id: 2,
            user_id: "@bob:example.com".to_string(),
            device_id: "DEV456".to_string(),
            device_data: Value::Object(Map::new()),
            algorithm: "org.matrix.msc3814.v1.olm".to_string(),
            account: Some(json!({"algorithms": ["m.olm.v1.curve25519-aes-sha2"]})),
            created_ts: 1700000000000,
            updated_ts: 1700000001000,
            expires_at: Some(1700086400000),
        };
        let result = DehydratedDeviceService::record_to_response(record);
        assert_eq!(result["device_id"], json!("DEV456"));
        assert_eq!(result["algorithm"], json!("org.matrix.msc3814.v1.olm"));
        assert_eq!(result["account"], json!({"algorithms": ["m.olm.v1.curve25519-aes-sha2"]}));
        assert_eq!(result["expires_at"], json!(1700086400000_i64));
    }

    #[test]
    fn test_record_to_response_non_object_data() {
        let record = DehydratedDevice {
            id: 3,
            user_id: "@carol:example.com".to_string(),
            device_id: "DEV789".to_string(),
            device_data: json!("non_object_data"),
            algorithm: "m.olm.v1.curve25519-aes-sha2".to_string(),
            account: None,
            created_ts: 1700000000000,
            updated_ts: 1700000001000,
            expires_at: None,
        };
        let result = DehydratedDeviceService::record_to_response(record);
        assert_eq!(result["device_id"], json!("DEV789"));
        assert_eq!(result["device_data"], json!("non_object_data"));
        assert_eq!(result["algorithm"], json!("m.olm.v1.curve25519-aes-sha2"));
    }
}
