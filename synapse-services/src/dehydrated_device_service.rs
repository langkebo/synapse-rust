use serde_json::{Map, Value};
use std::sync::Arc;
use synapse_common::traits::DehydratedDeviceProvider;
use synapse_common::ApiError;
use synapse_storage::dehydrated_device::DehydratedDeviceStoreApi;
use synapse_storage::{DehydratedDevice, UpsertDehydratedDeviceParams};

#[derive(Debug)]
struct NormalizedDehydratedDevicePayload {
    device_id: String,
    payload: Map<String, Value>,
    algorithm: String,
    account: Option<Value>,
    expires_at: Option<i64>,
}

#[derive(Clone)]
pub struct DehydratedDeviceService {
    storage: Arc<dyn DehydratedDeviceStoreApi>,
}

impl DehydratedDeviceService {
    pub fn new(storage: Arc<dyn DehydratedDeviceStoreApi>) -> Self {
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
        let existing_device_id = self.existing_device_id(user_id).await;
        let normalized = Self::normalize_put_payload(body, existing_device_id)?;

        self.storage
            .upsert_for_user(UpsertDehydratedDeviceParams {
                user_id: user_id.to_string(),
                device_id: normalized.device_id.clone(),
                device_data: Value::Object(normalized.payload),
                algorithm: normalized.algorithm,
                account: normalized.account,
                expires_at: normalized.expires_at,
            })
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to store dehydrated device", &e))?;

        Ok(normalized.device_id)
    }

    async fn existing_device_id(&self, user_id: &str) -> Option<String> {
        self.storage.get_by_user(user_id).await.ok().flatten().map(|record| record.device_id)
    }

    pub async fn delete_device(&self, user_id: &str) -> Result<Option<String>, ApiError> {
        let record = self
            .storage
            .get_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load dehydrated device before delete", &e))?;

        let Some(record) = record else {
            return Ok(None);
        };

        self.storage
            .delete_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete dehydrated device", &e))?;
        Ok(Some(record.device_id))
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

    fn normalize_put_payload(
        body: Value,
        existing_device_id: Option<String>,
    ) -> Result<NormalizedDehydratedDevicePayload, ApiError> {
        let mut payload =
            body.as_object().cloned().ok_or_else(|| ApiError::bad_request("Expected a JSON object body"))?;

        let mut device_data = payload.get("device_data").and_then(Value::as_object).cloned().unwrap_or_default();

        if !matches!(payload.get("device_keys"), Some(Value::Object(_))) {
            if let Some(nested_device_keys) = device_data.get("device_keys").filter(|value| value.is_object()).cloned()
            {
                payload.insert("device_keys".to_string(), nested_device_keys);
            } else {
                return Err(ApiError::bad_request("Device key(s) not found, these must be provided."));
            }
        }

        if device_data.is_empty() {
            if let Some(algorithm) = payload.get("algorithm").cloned() {
                device_data.insert("algorithm".to_string(), algorithm);
            }
            if let Some(account) = payload.get("account").cloned() {
                device_data.insert("account".to_string(), account);
            }
            if let Some(display_name) = payload.get("initial_device_display_name").cloned() {
                device_data.insert("initial_device_display_name".to_string(), display_name);
            }
        }

        if device_data.is_empty() {
            return Err(ApiError::bad_request("Missing or invalid device_data"));
        }

        let device_id = payload
            .get("device_id")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
            .or(existing_device_id)
            .unwrap_or_else(Self::generate_device_id);
        payload.insert("device_id".to_string(), Value::String(device_id.clone()));
        payload.insert("device_data".to_string(), Value::Object(device_data.clone()));

        let algorithm = device_data
            .get("algorithm")
            .and_then(Value::as_str)
            .or_else(|| payload.get("algorithm").and_then(Value::as_str))
            .unwrap_or("org.matrix.msc3814.v1.olm")
            .to_string();
        let account = device_data.get("account").cloned().or_else(|| payload.get("account").cloned());
        let expires_at = payload.get("expires_at").and_then(Value::as_i64);

        Ok(NormalizedDehydratedDevicePayload { device_id, payload, algorithm, account, expires_at })
    }

    fn record_to_response(record: DehydratedDevice) -> Value {
        let device_data = Self::build_response_device_data(
            record.device_data,
            &record.algorithm,
            record.account.as_ref(),
            record.expires_at,
        );
        serde_json::json!({
            "device_id": record.device_id,
            "device_data": device_data,
        })
    }

    fn build_response_device_data(
        stored_payload: Value,
        algorithm: &str,
        account: Option<&Value>,
        expires_at: Option<i64>,
    ) -> Value {
        let mut device_data = match stored_payload {
            Value::Object(map) => {
                let mut device_data = map.get("device_data").and_then(Value::as_object).cloned().unwrap_or_default();

                if let Some(display_name) = map.get("initial_device_display_name").cloned() {
                    device_data.entry("initial_device_display_name".to_string()).or_insert(display_name);
                }

                device_data
            }
            other => {
                let mut map = Map::new();
                map.insert("raw".to_string(), other);
                map
            }
        };

        device_data.entry("algorithm".to_string()).or_insert_with(|| Value::String(algorithm.to_string()));
        if let Some(account) = account {
            device_data.entry("account".to_string()).or_insert_with(|| account.clone());
        }
        if let Some(expires_at) = expires_at {
            device_data.entry("expires_at".to_string()).or_insert_with(|| Value::from(expires_at));
        }

        Value::Object(device_data)
    }
}

#[async_trait::async_trait]
impl DehydratedDeviceProvider for DehydratedDeviceService {
    async fn get_dehydrated_device(&self, user_id: &str) -> Result<Option<Value>, ApiError> {
        self.get_device(user_id).await
    }

    async fn put_dehydrated_device(&self, user_id: &str, data: Value) -> Result<String, ApiError> {
        self.put_device(user_id, data).await
    }

    async fn delete_dehydrated_device(&self, user_id: &str, _device_id: &str) -> Result<(), ApiError> {
        self.delete_all_for_user(user_id).await
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use synapse_storage::test_mocks::dehydrated_device::InMemoryDehydratedDeviceStore;

    fn make_valid_device_body() -> Value {
        json!({
            "device_keys": {
                "user_id": "@alice:localhost",
                "device_id": "DEV123",
                "algorithms": ["m.olm.v1.curve25519-aes-sha2"]
            },
            "device_data": {
                "algorithm": "org.matrix.msc3814.v1.olm"
            }
        })
    }

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

    // ========== normalize_put_payload tests ==========

    #[test]
    fn test_normalize_put_payload_promotes_legacy_nested_device_keys() {
        let normalized = DehydratedDeviceService::normalize_put_payload(
            json!({
                "algorithm": "org.matrix.msc3814.v1.olm",
                "account": { "pickle": "opaque-account" },
                "device_data": {
                    "device_keys": {
                        "user_id": "@alice:example.com",
                        "device_id": "DEV1",
                        "algorithms": ["m.olm.v1.curve25519-aes-sha2"]
                    },
                    "initial_device_display_name": "Legacy Device"
                }
            }),
            None,
        )
        .expect("payload should normalize");

        assert!(normalized.device_id.starts_with("DEHYDRATED"));
        assert_eq!(normalized.algorithm, "org.matrix.msc3814.v1.olm");
        assert_eq!(normalized.account, Some(json!({ "pickle": "opaque-account" })));
        assert!(normalized.payload["device_keys"].is_object());
        assert_eq!(normalized.payload["device_data"]["initial_device_display_name"], json!("Legacy Device"));
    }

    #[test]
    fn test_normalize_put_payload_rejects_missing_device_keys() {
        let error = DehydratedDeviceService::normalize_put_payload(
            json!({
                "device_id": "DEV1",
                "device_data": {
                    "algorithm": "org.matrix.msc3814.v1.olm"
                }
            }),
            None,
        )
        .expect_err("missing device_keys should be rejected");

        assert!(format!("{error:?}").contains("Device key(s) not found"));
    }

    #[test]
    fn test_build_response_device_data_returns_synapse_shape() {
        let response = DehydratedDeviceService::build_response_device_data(
            json!({
                "device_id": "DEV1",
                "device_keys": {
                    "user_id": "@alice:example.com",
                    "device_id": "DEV1"
                },
                "device_data": {
                    "initial_device_display_name": "Synapse-style device"
                }
            }),
            "org.matrix.msc3814.v1.olm",
            Some(&json!({ "pickle": "opaque-account" })),
            Some(1700000000000_i64),
        );

        assert_eq!(response["initial_device_display_name"], json!("Synapse-style device"));
        assert_eq!(response["algorithm"], json!("org.matrix.msc3814.v1.olm"));
        assert_eq!(response["account"], json!({ "pickle": "opaque-account" }));
        assert_eq!(response["expires_at"], json!(1700000000000_i64));
        assert!(response.get("device_keys").is_none());
    }

    // ========== record_to_response tests ==========

    #[test]
    fn test_record_to_response_basic() {
        let record = DehydratedDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV123".to_string(),
            device_data: json!({
                "device_keys": {
                    "user_id": "@alice:example.com",
                    "device_id": "DEV123"
                },
                "device_data": {
                    "initial_device_display_name": "Stored device"
                }
            }),
            algorithm: "org.matrix.msc3814.v1.olm".to_string(),
            account: None,
            created_ts: 1700000000000,
            updated_ts: 1700000001000,
            expires_at: None,
        };
        let result = DehydratedDeviceService::record_to_response(record);
        assert_eq!(result["device_id"], json!("DEV123"));
        assert_eq!(result["device_data"]["initial_device_display_name"], json!("Stored device"));
        assert_eq!(result["device_data"]["algorithm"], json!("org.matrix.msc3814.v1.olm"));
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
        assert_eq!(result["device_data"]["algorithm"], json!("org.matrix.msc3814.v1.olm"));
        assert_eq!(result["device_data"]["account"], json!({"algorithms": ["m.olm.v1.curve25519-aes-sha2"]}));
        assert_eq!(result["device_data"]["expires_at"], json!(1700086400000_i64));
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
        assert_eq!(result["device_data"]["raw"], json!("non_object_data"));
        assert_eq!(result["device_data"]["algorithm"], json!("m.olm.v1.curve25519-aes-sha2"));
    }

    // ========== DehydratedDeviceService mock-backed behavioural tests ==========

    #[tokio::test]
    async fn get_device_returns_none_when_no_device_stored() {
        let store = Arc::new(InMemoryDehydratedDeviceStore::new());
        let service = DehydratedDeviceService::new(store);

        let result = service.get_device("@alice:localhost").await.unwrap();
        assert!(result.is_none(), "expected None when no device exists");
    }

    #[tokio::test]
    async fn get_status_shows_not_found_when_no_device_stored() {
        let store = Arc::new(InMemoryDehydratedDeviceStore::new());
        let service = DehydratedDeviceService::new(store);

        let status = service.get_status("@alice:localhost").await.unwrap();
        assert_eq!(status["exists"], json!(false));
    }

    #[tokio::test]
    async fn put_then_get_device_roundtrip() {
        let store = Arc::new(InMemoryDehydratedDeviceStore::new());
        let service = DehydratedDeviceService::new(store);

        let device_id = service.put_device("@alice:localhost", make_valid_device_body()).await.unwrap();
        assert!(device_id.starts_with("DEHYDRATED"), "expected generated device_id, got: {device_id}");

        let result = service.get_device("@alice:localhost").await.unwrap();
        let device = result.expect("expected Some after put_device");
        assert_eq!(device["device_id"], json!(device_id));
        assert_eq!(device["device_data"]["algorithm"], json!("org.matrix.msc3814.v1.olm"));
    }

    #[tokio::test]
    async fn put_then_get_status_shows_details() {
        let store = Arc::new(InMemoryDehydratedDeviceStore::new());
        let service = DehydratedDeviceService::new(store);

        let device_id = service.put_device("@bob:localhost", make_valid_device_body()).await.unwrap();

        let status = service.get_status("@bob:localhost").await.unwrap();
        assert_eq!(status["exists"], json!(true));
        assert_eq!(status["device_id"], json!(device_id));
        assert_eq!(status["algorithm"], json!("org.matrix.msc3814.v1.olm"));
    }

    #[tokio::test]
    async fn delete_returns_none_when_no_device_stored() {
        let store = Arc::new(InMemoryDehydratedDeviceStore::new());
        let service = DehydratedDeviceService::new(store);

        let result = service.delete_device("@nobody:localhost").await.unwrap();
        assert!(result.is_none(), "expected None when deleting non-existent device");
    }

    #[tokio::test]
    async fn put_delete_then_get_returns_none() {
        let store = Arc::new(InMemoryDehydratedDeviceStore::new());
        let service = DehydratedDeviceService::new(store);

        let device_id = service.put_device("@carol:localhost", make_valid_device_body()).await.unwrap();

        let deleted = service.delete_device("@carol:localhost").await.unwrap();
        assert_eq!(deleted, Some(device_id.clone()), "delete should return the removed device_id");

        let after = service.get_device("@carol:localhost").await.unwrap();
        assert!(after.is_none(), "expected None after delete");
    }

    #[tokio::test]
    async fn put_device_preserves_explicit_device_id() {
        let store = Arc::new(InMemoryDehydratedDeviceStore::new());
        let service = DehydratedDeviceService::new(store);

        let mut body = make_valid_device_body();
        body["device_id"] = json!("CUSTOM-DEV-42");
        let returned_id = service.put_device("@dave:localhost", body).await.unwrap();
        assert_eq!(returned_id, "CUSTOM-DEV-42");

        let result = service.get_device("@dave:localhost").await.unwrap();
        assert_eq!(result.unwrap()["device_id"], json!("CUSTOM-DEV-42"));
    }
}
