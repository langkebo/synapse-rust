use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

fn default_json_object() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKey {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub algorithm: String,
    pub key_id: String,
    pub public_key: String,
    pub signatures: serde_json::Value,
    pub created_ts: DateTime<Utc>,
    pub updated_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeys {
    pub user_id: String,
    pub device_id: String,
    pub algorithms: Vec<String>,
    pub keys: serde_json::Value,
    pub signatures: serde_json::Value,
    pub unsigned: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyQueryRequest {
    pub timeout: Option<u64>,
    #[serde(default = "default_json_object")]
    pub device_keys: serde_json::Value,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyQueryResponse {
    pub device_keys: serde_json::Value,
    pub master_keys: serde_json::Value,
    pub self_signing_keys: serde_json::Value,
    pub user_signing_keys: serde_json::Value,
    pub failures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadRequest {
    pub device_keys: Option<DeviceKeys>,
    pub one_time_keys: Option<serde_json::Value>,
    #[serde(default)]
    pub fallback_keys: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadResponse {
    pub one_time_key_counts: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyClaimRequest {
    pub timeout: Option<u64>,
    #[serde(default = "default_json_object")]
    pub one_time_keys: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyClaimResponse {
    pub one_time_keys: serde_json::Value,
    pub failures: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_key_creation() {
        let key = DeviceKey {
            id: 0, // 数据库自动生成
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            display_name: Some("My Device".to_string()),
            algorithm: "curve25519".to_string(),
            key_id: "KEY123".to_string(),
            public_key: "public_key_value".to_string(),
            signatures: serde_json::json!({}),
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        assert_eq!(key.user_id, "@test:example.com");
        assert_eq!(key.device_id, "DEVICE123");
        assert_eq!(key.algorithm, "curve25519");
    }

    #[test]
    fn test_device_keys_creation() {
        let keys = DeviceKeys {
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            algorithms: vec!["curve25519".to_string(), "ed25519".to_string()],
            keys: serde_json::json!({
                "curve25519:KEY1": "public_key_1",
                "ed25519:KEY1": "public_key_2"
            }),
            signatures: serde_json::json!({}),
            unsigned: None,
        };

        assert_eq!(keys.user_id, "@test:example.com");
        assert_eq!(keys.device_id, "DEVICE123");
        assert_eq!(keys.algorithms.len(), 2);
    }

    #[test]
    fn test_key_query_request() {
        let request = KeyQueryRequest {
            timeout: Some(10000),
            device_keys: serde_json::json!({
                "@test:example.com": ["DEVICE123", "DEVICE456"]
            }),
            token: Some("token123".to_string()),
        };

        assert!(request.timeout.is_some());
        assert!(request.device_keys.is_object());
        assert!(request.token.is_some());
    }

    #[test]
    fn test_key_query_response() {
        let response = KeyQueryResponse {
            device_keys: serde_json::json!({
                "@test:example.com": {
                    "DEVICE123": {}
                }
            }),
            master_keys: serde_json::json!({}),
            self_signing_keys: serde_json::json!({}),
            user_signing_keys: serde_json::json!({}),
            failures: serde_json::json!({}),
        };

        assert!(response.device_keys.is_object());
        assert!(response.failures.is_object());
    }

    #[test]
    fn test_key_upload_request() {
        let request = KeyUploadRequest {
            device_keys: Some(DeviceKeys {
                user_id: "@test:example.com".to_string(),
                device_id: "DEVICE123".to_string(),
                algorithms: vec!["curve25519".to_string()],
                keys: serde_json::json!({
                    "curve25519:KEY1": "public_key"
                }),
                signatures: serde_json::json!({}),
                unsigned: None,
            }),
            one_time_keys: Some(serde_json::json!({
                "curve25519:KEY2": "one_time_key"
            })),
            fallback_keys: None,
        };

        assert!(request.device_keys.is_some());
        assert!(request.one_time_keys.is_some());
    }

    #[test]
    fn test_key_upload_response() {
        let response = KeyUploadResponse {
            one_time_key_counts: serde_json::json!({
                "curve25519": 10
            }),
        };

        assert!(response.one_time_key_counts.is_object());
    }

    #[test]
    fn test_key_claim_request() {
        let request = KeyClaimRequest {
            timeout: Some(5000),
            one_time_keys: serde_json::json!({
                "@test:example.com": {
                    "DEVICE123": ["curve25519:KEY1"]
                }
            }),
        };

        assert!(request.timeout.is_some());
        assert!(request.one_time_keys.is_object());
    }

    #[test]
    fn test_key_claim_response() {
        let response = KeyClaimResponse {
            one_time_keys: serde_json::json!({
                "@test:example.com": {
                    "DEVICE123": {
                        "curve25519:KEY1": "encrypted_key"
                    }
                }
            }),
            failures: serde_json::json!({}),
        };

        assert!(response.one_time_keys.is_object());
        assert!(response.failures.is_object());
    }

    #[test]
    fn test_device_keys_serialization() {
        let keys = DeviceKeys {
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            algorithms: vec!["curve25519".to_string()],
            keys: serde_json::json!({
                "curve25519:KEY1": "public_key"
            }),
            signatures: serde_json::json!({
                "@test:example.com": {
                    "ed25519:DEVICE123": "signature"
                }
            }),
            unsigned: Some(serde_json::json!({
                "device_display_name": "My Device"
            })),
        };

        let json = serde_json::to_string(&keys).unwrap();
        let deserialized: DeviceKeys = serde_json::from_str(&json).unwrap();

        assert_eq!(keys.user_id, deserialized.user_id);
        assert_eq!(keys.device_id, deserialized.device_id);
        assert_eq!(keys.algorithms, deserialized.algorithms);
    }

    // ================================================================
    // ISSUE-02: fallback key claim tests
    // Verifies the two-phase claim: regular OTK first, then fallback
    // when OTK stock is exhausted. Fallback keys must NOT be consumed.
    // ================================================================

    use crate::device_keys::storage::DeviceKeyStoreApi;
    use crate::test_mocks::InMemoryDeviceKeyStore;

    fn make_test_key(user_id: &str, device_id: &str, algorithm: &str, key_id: &str) -> DeviceKey {
        DeviceKey {
            id: 0,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            display_name: None,
            algorithm: algorithm.to_string(),
            key_id: key_id.to_string(),
            public_key: format!("pk_{key_id}"),
            signatures: serde_json::json!({}),
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_claim_returns_regular_otk_when_available() {
        let store = InMemoryDeviceKeyStore::new();
        let user = "@alice:example.com";
        let device = "DEV001";
        let algo = "signed_curve25519";

        // Seed a regular OTK
        let otk = make_test_key(user, device, algo, "otk:1");
        store.create_device_key(&otk).await.unwrap();

        // Seed a fallback key (should NOT be returned while OTKs exist)
        let fbk = make_test_key(user, device, algo, "fallback:1");
        store.create_fallback_key(&fbk).await.unwrap();

        let claimed = store.claim_one_time_key(user, device, algo).await.unwrap();
        assert!(claimed.is_some(), "should return a key");
        let key = claimed.unwrap();
        assert_eq!(key.key_id, "otk:1", "should return the regular OTK, not the fallback");

        // Verify the OTK was consumed (deleted)
        let count = store.get_one_time_keys_count(user, device).await.unwrap();
        assert_eq!(count, 0, "regular OTK should be consumed after claim");
    }

    #[tokio::test]
    async fn test_claim_returns_fallback_when_otk_exhausted() {
        let store = InMemoryDeviceKeyStore::new();
        let user = "@bob:example.com";
        let device = "DEV002";
        let algo = "signed_curve25519";

        // Seed only a fallback key (no regular OTKs)
        let fbk = make_test_key(user, device, algo, "fallback:1");
        store.create_fallback_key(&fbk).await.unwrap();

        // First claim — should return the fallback key
        let claimed = store.claim_one_time_key(user, device, algo).await.unwrap();
        assert!(claimed.is_some(), "should return fallback key when OTKs exhausted");
        let key = claimed.unwrap();
        assert_eq!(key.key_id, "fallback:1", "should return the fallback key");
        assert_eq!(key.algorithm, algo);
    }

    #[tokio::test]
    async fn test_fallback_key_not_consumed_on_claim() {
        let store = InMemoryDeviceKeyStore::new();
        let user = "@carol:example.com";
        let device = "DEV003";
        let algo = "signed_curve25519";

        // Seed only a fallback key
        let fbk = make_test_key(user, device, algo, "fallback:1");
        store.create_fallback_key(&fbk).await.unwrap();

        // Claim multiple times — fallback should still be available
        for i in 0..3 {
            let claimed = store.claim_one_time_key(user, device, algo).await.unwrap();
            assert!(claimed.is_some(), "claim #{i}: fallback key should still be available");
            assert_eq!(claimed.unwrap().key_id, "fallback:1");
        }

        // Verify the fallback key is still in the store
        let unused = store.get_unused_fallback_key_types(user, device).await.unwrap();
        assert!(!unused.is_empty(), "fallback key should not be consumed");
    }

    #[tokio::test]
    async fn test_claim_returns_none_when_no_keys() {
        let store = InMemoryDeviceKeyStore::new();
        let user = "@dave:example.com";
        let device = "DEV004";
        let algo = "signed_curve25519";

        // No keys at all
        let claimed = store.claim_one_time_key(user, device, algo).await.unwrap();
        assert!(claimed.is_none(), "should return None when no OTK or fallback exists");
    }

    #[tokio::test]
    async fn test_claim_does_not_cross_devices() {
        let store = InMemoryDeviceKeyStore::new();
        let user = "@eve:example.com";
        let algo = "signed_curve25519";

        // Seed OTK for device A
        let otk_a = make_test_key(user, "DEVICE_A", algo, "otk:a");
        store.create_device_key(&otk_a).await.unwrap();

        // Claim for device B — should not get device A's key
        let claimed = store.claim_one_time_key(user, "DEVICE_B", algo).await.unwrap();
        assert!(claimed.is_none(), "should not return keys from a different device");
    }

    #[tokio::test]
    async fn test_claim_otk_priority_over_fallback() {
        let store = InMemoryDeviceKeyStore::new();
        let user = "@frank:example.com";
        let device = "DEV005";
        let algo = "signed_curve25519";

        // Seed multiple OTKs and one fallback
        for i in 0..3 {
            let otk = make_test_key(user, device, algo, &format!("otk:{i}"));
            store.create_device_key(&otk).await.unwrap();
        }
        let fbk = make_test_key(user, device, algo, "fallback:1");
        store.create_fallback_key(&fbk).await.unwrap();

        // Claim all 3 OTKs
        for i in 0..3 {
            let claimed = store.claim_one_time_key(user, device, algo).await.unwrap();
            assert!(claimed.is_some(), "claim #{i} should succeed");
            assert!(
                claimed.unwrap().key_id.starts_with("otk:"),
                "should return regular OTK, not fallback, while OTKs available"
            );
        }

        // 4th claim — OTKs exhausted, should get fallback
        let claimed = store.claim_one_time_key(user, device, algo).await.unwrap();
        assert!(claimed.is_some(), "claim #4 should return fallback");
        assert_eq!(claimed.unwrap().key_id, "fallback:1");

        // 5th claim — fallback still available (not consumed)
        let claimed = store.claim_one_time_key(user, device, algo).await.unwrap();
        assert!(claimed.is_some(), "claim #5 should still return fallback");
        assert_eq!(claimed.unwrap().key_id, "fallback:1");
    }
}
