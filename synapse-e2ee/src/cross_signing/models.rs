use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `CrossSigningKey` type.
pub struct CrossSigningKey {
    /// The `id` field.
    /// The `user_id` field.
    /// The `key_type` field.
    /// The `public_key` field.
    /// The `usage` field.
    /// The `signatures` field.
    /// The `key_json` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub id: Uuid,
    /// The `user_id` field.
    /// The `key_type` field.
    /// The `public_key` field.
    /// The `usage` field.
    /// The `signatures` field.
    /// The `key_json` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub user_id: String,
    /// The `key_type` field.
    /// The `public_key` field.
    /// The `usage` field.
    /// The `signatures` field.
    /// The `key_json` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub key_type: String,
    /// The `public_key` field.
    /// The `usage` field.
    /// The `signatures` field.
    /// The `key_json` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub public_key: String,
    /// The `usage` field.
    /// The `signatures` field.
    /// The `key_json` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub usage: Vec<String>,
    /// The `signatures` field.
    /// The `key_json` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub signatures: serde_json::Value,
    /// The `key_json` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub key_json: Option<serde_json::Value>,
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub created_ts: DateTime<Utc>,
    /// The `updated_ts` field.
    pub updated_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `CrossSigningKeys` type.
pub struct CrossSigningKeys {
    /// The `user_id` field.
    /// The `master_key` field.
    /// The `self_signing_key` field.
    /// The `user_signing_key` field.
    /// The `self_signing_signature` field.
    /// The `user_signing_signature` field.
    pub user_id: String,
    /// The `master_key` field.
    /// The `self_signing_key` field.
    /// The `user_signing_key` field.
    /// The `self_signing_signature` field.
    /// The `user_signing_signature` field.
    pub master_key: String,
    /// The `self_signing_key` field.
    /// The `user_signing_key` field.
    /// The `self_signing_signature` field.
    /// The `user_signing_signature` field.
    pub self_signing_key: String,
    /// The `user_signing_key` field.
    /// The `self_signing_signature` field.
    /// The `user_signing_signature` field.
    pub user_signing_key: String,
    /// The `self_signing_signature` field.
    /// The `user_signing_signature` field.
    pub self_signing_signature: String,
    /// The `user_signing_signature` field.
    pub user_signing_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DeviceKeyInfo` type.
pub struct DeviceKeyInfo {
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `key_type` field.
    /// The `algorithm` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `key_type` field.
    /// The `algorithm` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub device_id: String,
    /// The `key_type` field.
    /// The `algorithm` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub key_type: String,
    /// The `algorithm` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub algorithm: String,
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub public_key: String,
    /// The `signatures` field.
    /// The `created_ts` field.
    pub signatures: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SignatureUploadRequest` type.
pub struct SignatureUploadRequest {
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `key_type` field.
    /// The `key_id` field.
    /// The `signatures` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `key_type` field.
    /// The `key_id` field.
    /// The `signatures` field.
    pub device_id: Option<String>,
    /// The `key_type` field.
    /// The `key_id` field.
    /// The `signatures` field.
    pub key_type: String,
    /// The `key_id` field.
    /// The `signatures` field.
    pub key_id: String,
    /// The `signatures` field.
    pub signatures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SignatureUploadResponse` type.
pub struct SignatureUploadResponse {
    /// The `fail` field.
    pub fail: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DeviceSignature` type.
pub struct DeviceSignature {
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `signing_key_id` field.
    /// The `target_user_id` field.
    /// The `target_device_id` field.
    /// The `target_key_id` field.
    /// The `signature` field.
    /// The `created_ts` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `signing_key_id` field.
    /// The `target_user_id` field.
    /// The `target_device_id` field.
    /// The `target_key_id` field.
    /// The `signature` field.
    /// The `created_ts` field.
    pub device_id: String,
    /// The `signing_key_id` field.
    /// The `target_user_id` field.
    /// The `target_device_id` field.
    /// The `target_key_id` field.
    /// The `signature` field.
    /// The `created_ts` field.
    pub signing_key_id: String,
    /// The `target_user_id` field.
    /// The `target_device_id` field.
    /// The `target_key_id` field.
    /// The `signature` field.
    /// The `created_ts` field.
    pub target_user_id: String,
    /// The `target_device_id` field.
    /// The `target_key_id` field.
    /// The `signature` field.
    /// The `created_ts` field.
    pub target_device_id: String,
    /// The `target_key_id` field.
    /// The `signature` field.
    /// The `created_ts` field.
    pub target_key_id: String,
    /// The `signature` field.
    /// The `created_ts` field.
    pub signature: String,
    /// The `created_ts` field.
    pub created_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SignatureVerificationRequest` type.
pub struct SignatureVerificationRequest {
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `key_id` field.
    /// The `signature` field.
    /// The `signing_key_id` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `key_id` field.
    /// The `signature` field.
    /// The `signing_key_id` field.
    pub device_id: String,
    /// The `key_id` field.
    /// The `signature` field.
    /// The `signing_key_id` field.
    pub key_id: String,
    /// The `signature` field.
    /// The `signing_key_id` field.
    pub signature: String,
    /// The `signing_key_id` field.
    pub signing_key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SignatureVerificationResponse` type.
pub struct SignatureVerificationResponse {
    /// The `valid` field.
    /// The `verified_at` field.
    pub valid: bool,
    /// The `verified_at` field.
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BulkSignatureUpload` type.
pub struct BulkSignatureUpload {
    /// The `signatures` field.
    pub signatures: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `UserSignatures` type.
pub struct UserSignatures {
    /// The `user_id` field.
    /// The `signatures` field.
    pub user_id: String,
    /// The `signatures` field.
    pub signatures: Vec<DeviceSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DeviceVerificationStatus` type.
pub struct DeviceVerificationStatus {
    /// The `device_id` field.
    /// The `is_verified` field.
    /// The `verified_by` field.
    /// The `verified_at` field.
    pub device_id: String,
    /// The `is_verified` field.
    /// The `verified_by` field.
    /// The `verified_at` field.
    pub is_verified: bool,
    /// The `verified_by` field.
    /// The `verified_at` field.
    pub verified_by: Option<String>,
    /// The `verified_at` field.
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `UserVerificationStatus` type.
pub struct UserVerificationStatus {
    /// The `user_id` field.
    /// The `is_verified` field.
    /// The `has_master_key` field.
    /// The `has_self_signing_key` field.
    /// The `has_user_signing_key` field.
    /// The `verified_at` field.
    pub user_id: String,
    /// The `is_verified` field.
    /// The `has_master_key` field.
    /// The `has_self_signing_key` field.
    /// The `has_user_signing_key` field.
    /// The `verified_at` field.
    pub is_verified: bool,
    /// The `has_master_key` field.
    /// The `has_self_signing_key` field.
    /// The `has_user_signing_key` field.
    /// The `verified_at` field.
    pub has_master_key: bool,
    /// The `has_self_signing_key` field.
    /// The `has_user_signing_key` field.
    /// The `verified_at` field.
    pub has_self_signing_key: bool,
    /// The `has_user_signing_key` field.
    /// The `verified_at` field.
    pub has_user_signing_key: bool,
    /// The `verified_at` field.
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DeviceKeyVerificationResult` type.
pub struct DeviceKeyVerificationResult {
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `is_verified` field.
    /// The `verified_by_master` field.
    /// The `verified_by_self_signing` field.
    /// The `verification_method` field.
    /// The `verified_at` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `is_verified` field.
    /// The `verified_by_master` field.
    /// The `verified_by_self_signing` field.
    /// The `verification_method` field.
    /// The `verified_at` field.
    pub device_id: String,
    /// The `is_verified` field.
    /// The `verified_by_master` field.
    /// The `verified_by_self_signing` field.
    /// The `verification_method` field.
    /// The `verified_at` field.
    pub is_verified: bool,
    /// The `verified_by_master` field.
    /// The `verified_by_self_signing` field.
    /// The `verification_method` field.
    /// The `verified_at` field.
    pub verified_by_master: bool,
    /// The `verified_by_self_signing` field.
    /// The `verification_method` field.
    /// The `verified_at` field.
    pub verified_by_self_signing: bool,
    /// The `verification_method` field.
    /// The `verified_at` field.
    pub verification_method: Option<String>,
    /// The `verified_at` field.
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `VerifiedDevicesMap` type.
pub struct VerifiedDevicesMap {
    /// The `user_id` field.
    /// The `verified_devices` field.
    pub user_id: String,
    /// The `verified_devices` field.
    pub verified_devices: Vec<DeviceKeyVerificationResult>,
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
            key_json: None,
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
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
    fn test_cross_signing_key_types() {
        let master = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "master".to_string(),
            public_key: "pk1".to_string(),
            usage: vec!["master_key".to_string()],
            signatures: serde_json::json!({}),
            key_json: None,
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        let self_signing = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "self_signing".to_string(),
            public_key: "pk2".to_string(),
            usage: vec!["self_signing_key".to_string()],
            signatures: serde_json::json!({}),
            key_json: None,
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        let user_signing = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            key_type: "user_signing".to_string(),
            public_key: "pk3".to_string(),
            usage: vec!["user_signing_key".to_string()],
            signatures: serde_json::json!({}),
            key_json: None,
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
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
            key_json: None,
            created_ts: chrono::Utc::now(),
            updated_ts: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&key).unwrap();
        let deserialized: CrossSigningKey = serde_json::from_str(&json).unwrap();

        assert_eq!(key.user_id, deserialized.user_id);
        assert_eq!(key.key_type, deserialized.key_type);
        assert_eq!(key.public_key, deserialized.public_key);
    }
}
