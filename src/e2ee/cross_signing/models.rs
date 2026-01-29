use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningKey {
    pub id: Uuid,
    pub user_id: String,
    pub key_type: String,
    pub public_key: String,
    pub usage: Vec<String>,
    pub signatures: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningKeys {
    pub user_id: String,
    pub master_key: String,
    pub self_signing_key: String,
    pub user_signing_key: String,
    pub self_signing_signature: String,
    pub user_signing_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningUpload {
    pub master_key: serde_json::Value,
    pub self_signing_key: serde_json::Value,
    pub user_signing_key: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeyInfo {
    pub user_id: String,
    pub device_id: String,
    pub key_type: String,
    pub algorithm: String,
    pub public_key: String,
    pub signatures: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_signing_key_creation() {
        let key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "master".to_string(),
            public_key: "public_key_value".to_string(),
            usage: vec!["master_key".to_string()],
            signatures: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert_eq!(key.user_id, "@test:example.com");
        assert_eq!(key.key_type, "master");
        assert_eq!(key.usage, vec!["master_key"]);
    }

    #[test]
    fn test_cross_signing_keys_creation() {
        let keys = CrossSigningKeys {
            user_id: "@test:example.com".to_string(),
            master_key: "master_public_key".to_string(),
            self_signing_key: "self_signing_public_key".to_string(),
            user_signing_key: "user_signing_public_key".to_string(),
            self_signing_signature: "signature1".to_string(),
            user_signing_signature: "signature2".to_string(),
        };

        assert_eq!(keys.user_id, "@test:example.com");
        assert!(keys.master_key.starts_with("master"));
        assert!(keys.self_signing_key.starts_with("self"));
    }

    #[test]
    fn test_cross_signing_upload() {
        let upload = CrossSigningUpload {
            master_key: serde_json::json!({
                "user_id": "@test:example.com",
                "usage": ["master_key"],
                "keys": {"ed25519:KEY": "public_key"}
            }),
            self_signing_key: serde_json::json!({
                "user_id": "@test:example.com",
                "usage": ["self_signing_key"],
                "keys": {"ed25519:KEY": "public_key"}
            }),
            user_signing_key: serde_json::json!({
                "user_id": "@test:example.com",
                "usage": ["user_signing_key"],
                "keys": {"ed25519:KEY": "public_key"}
            }),
        };

        assert!(upload.master_key.is_object());
        assert!(upload.self_signing_key.is_object());
        assert!(upload.user_signing_key.is_object());
    }

    #[test]
    fn test_cross_signing_key_types() {
        let master = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "master".to_string(),
            public_key: "pk1".to_string(),
            usage: vec!["master_key".to_string()],
            signatures: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let self_signing = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "self_signing".to_string(),
            public_key: "pk2".to_string(),
            usage: vec!["self_signing_key".to_string()],
            signatures: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let user_signing = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "user_signing".to_string(),
            public_key: "pk3".to_string(),
            usage: vec!["user_signing_key".to_string()],
            signatures: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert_eq!(master.key_type, "master");
        assert_eq!(self_signing.key_type, "self_signing");
        assert_eq!(user_signing.key_type, "user_signing");
    }

    #[test]
    fn test_cross_signing_key_serialization() {
        let key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "master".to_string(),
            public_key: "public_key".to_string(),
            usage: vec!["master_key".to_string()],
            signatures: serde_json::json!({
                "@test:example.com": {"ed25519:DEVICE": "signature"}
            }),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&key).unwrap();
        let deserialized: CrossSigningKey = serde_json::from_str(&json).unwrap();

        assert_eq!(key.user_id, deserialized.user_id);
        assert_eq!(key.key_type, deserialized.key_type);
        assert_eq!(key.public_key, deserialized.public_key);
    }
}
