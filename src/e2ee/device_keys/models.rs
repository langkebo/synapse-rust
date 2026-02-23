use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    pub device_keys: serde_json::Value,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyQueryResponse {
    pub device_keys: serde_json::Value,
    pub failures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadRequest {
    pub device_keys: Option<DeviceKeys>,
    pub one_time_keys: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadResponse {
    pub one_time_key_counts: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyClaimRequest {
    pub timeout: Option<u64>,
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
}
