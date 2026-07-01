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
    pub key_json: Option<serde_json::Value>,
    pub created_ts: DateTime<Utc>,
    pub updated_ts: DateTime<Utc>,
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
    pub created_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureUploadRequest {
    pub user_id: String,
    pub device_id: Option<String>,
    pub key_type: String,
    pub key_id: String,
    pub signatures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureUploadResponse {
    pub fail: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSignature {
    pub user_id: String,
    pub device_id: String,
    pub signing_key_id: String,
    pub target_user_id: String,
    pub target_device_id: String,
    pub target_key_id: String,
    pub signature: String,
    pub created_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningSetupRequest {
    pub master_key: Option<serde_json::Value>,
    pub self_signing_key: Option<serde_json::Value>,
    pub user_signing_key: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningSetupResponse {
    pub master_key: serde_json::Value,
    pub self_signing_key: serde_json::Value,
    pub user_signing_key: serde_json::Value,
    pub master_key_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureVerificationRequest {
    pub user_id: String,
    pub device_id: String,
    pub key_id: String,
    pub signature: String,
    pub signing_key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureVerificationResponse {
    pub valid: bool,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkSignatureUpload {
    pub signatures: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSignatures {
    pub user_id: String,
    pub signatures: Vec<DeviceSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceVerificationStatus {
    pub device_id: String,
    pub is_verified: bool,
    pub verified_by: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserVerificationStatus {
    pub user_id: String,
    pub is_verified: bool,
    pub has_master_key: bool,
    pub has_self_signing_key: bool,
    pub has_user_signing_key: bool,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeyVerificationResult {
    pub user_id: String,
    pub device_id: String,
    pub is_verified: bool,
    pub verified_by_master: bool,
    pub verified_by_self_signing: bool,
    pub verification_method: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedDevicesMap {
    pub user_id: String,
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

    #[test]
    fn test_device_key_info_serde_roundtrip() {
        let info = DeviceKeyInfo {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV001".to_string(),
            key_type: "master".to_string(),
            algorithm: "ed25519".to_string(),
            public_key: "pk_value".to_string(),
            signatures: serde_json::json!({"@alice:example.com": {"ed25519:DEV001": "sig"}}),
            created_ts: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let restored: DeviceKeyInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.user_id, info.user_id);
        assert_eq!(restored.device_id, info.device_id);
        assert_eq!(restored.algorithm, info.algorithm);
    }

    #[test]
    fn test_signature_upload_request_serde_roundtrip() {
        let req = SignatureUploadRequest {
            user_id: "@bob:example.com".to_string(),
            device_id: Some("DEV002".to_string()),
            key_type: "self_signing".to_string(),
            key_id: "ed25519:KEY".to_string(),
            signatures: serde_json::json!({}),
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: SignatureUploadRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.user_id, req.user_id);
        assert_eq!(restored.device_id, req.device_id);
        assert_eq!(restored.key_id, req.key_id);
    }

    #[test]
    fn test_signature_upload_response_serde_roundtrip() {
        let resp = SignatureUploadResponse {
            fail: serde_json::Map::from_iter([("key1".to_string(), serde_json::json!("error"))]),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let restored: SignatureUploadResponse = serde_json::from_str(&json).unwrap();
        assert!(restored.fail.contains_key("key1"));
    }

    #[test]
    fn test_device_signature_serde_roundtrip() {
        let sig = DeviceSignature {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV001".to_string(),
            signing_key_id: "ed25519:KEY1".to_string(),
            target_user_id: "@bob:example.com".to_string(),
            target_device_id: "DEV002".to_string(),
            target_key_id: "ed25519:KEY2".to_string(),
            signature: "sig_hex".to_string(),
            created_ts: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&sig).unwrap();
        let restored: DeviceSignature = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.signing_key_id, sig.signing_key_id);
        assert_eq!(restored.target_device_id, sig.target_device_id);
    }

    #[test]
    fn test_cross_signing_setup_request_with_none_optionals() {
        let req = CrossSigningSetupRequest {
            master_key: None,
            self_signing_key: None,
            user_signing_key: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: CrossSigningSetupRequest = serde_json::from_str(&json).unwrap();
        assert!(restored.master_key.is_none());
        assert!(restored.self_signing_key.is_none());
        assert!(restored.user_signing_key.is_none());
    }

    #[test]
    fn test_cross_signing_setup_response_serde_roundtrip() {
        let resp = CrossSigningSetupResponse {
            master_key: serde_json::json!({"keys": {}}),
            self_signing_key: serde_json::json!({"keys": {}}),
            user_signing_key: serde_json::json!({"keys": {}}),
            master_key_signature: Some("sig".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let restored: CrossSigningSetupResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.master_key_signature, resp.master_key_signature);
    }

    #[test]
    fn test_signature_verification_request_serde_roundtrip() {
        let req = SignatureVerificationRequest {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV001".to_string(),
            key_id: "ed25519:KEY".to_string(),
            signature: "sig_data".to_string(),
            signing_key_id: "ed25519:SIGNER".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: SignatureVerificationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.signature, req.signature);
    }

    #[test]
    fn test_signature_verification_response_serde_roundtrip() {
        let resp = SignatureVerificationResponse {
            valid: true,
            verified_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let restored: SignatureVerificationResponse = serde_json::from_str(&json).unwrap();
        assert!(restored.valid);
    }

    #[test]
    fn test_bulk_signature_upload_serde_roundtrip() {
        let bulk = BulkSignatureUpload {
            signatures: serde_json::Map::from_iter([
                ("@alice:example.com".to_string(), serde_json::json!({"ed25519:KEY": "sig"})),
            ]),
        };
        let json = serde_json::to_string(&bulk).unwrap();
        let restored: BulkSignatureUpload = serde_json::from_str(&json).unwrap();
        assert!(restored.signatures.contains_key("@alice:example.com"));
    }

    #[test]
    fn test_user_signatures_serde_roundtrip() {
        let sig = DeviceSignature {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV001".to_string(),
            signing_key_id: "ed25519:KEY".to_string(),
            target_user_id: "@bob:example.com".to_string(),
            target_device_id: "DEV002".to_string(),
            target_key_id: "ed25519:KEY2".to_string(),
            signature: "sig".to_string(),
            created_ts: chrono::Utc::now(),
        };
        let us = UserSignatures {
            user_id: "@alice:example.com".to_string(),
            signatures: vec![sig],
        };
        let json = serde_json::to_string(&us).unwrap();
        let restored: UserSignatures = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.signatures.len(), 1);
    }

    #[test]
    fn test_device_verification_status_serde_roundtrip() {
        let status = DeviceVerificationStatus {
            device_id: "DEV001".to_string(),
            is_verified: true,
            verified_by: Some("@alice:example.com".to_string()),
            verified_at: Some(chrono::Utc::now()),
        };
        let json = serde_json::to_string(&status).unwrap();
        let restored: DeviceVerificationStatus = serde_json::from_str(&json).unwrap();
        assert!(restored.is_verified);
        assert_eq!(restored.device_id, status.device_id);
    }

    #[test]
    fn test_user_verification_status_serde_roundtrip() {
        let status = UserVerificationStatus {
            user_id: "@alice:example.com".to_string(),
            is_verified: true,
            has_master_key: true,
            has_self_signing_key: true,
            has_user_signing_key: false,
            verified_at: Some(chrono::Utc::now()),
        };
        let json = serde_json::to_string(&status).unwrap();
        let restored: UserVerificationStatus = serde_json::from_str(&json).unwrap();
        assert!(restored.has_master_key);
        assert!(!restored.has_user_signing_key);
    }

    #[test]
    fn test_device_key_verification_result_serde_roundtrip() {
        let result = DeviceKeyVerificationResult {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV001".to_string(),
            is_verified: true,
            verified_by_master: true,
            verified_by_self_signing: false,
            verification_method: Some("sas".to_string()),
            verified_at: Some(chrono::Utc::now()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let restored: DeviceKeyVerificationResult = serde_json::from_str(&json).unwrap();
        assert!(restored.verified_by_master);
        assert!(!restored.verified_by_self_signing);
    }

    #[test]
    fn test_verified_devices_map_serde_roundtrip() {
        let result = DeviceKeyVerificationResult {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV001".to_string(),
            is_verified: false,
            verified_by_master: false,
            verified_by_self_signing: false,
            verification_method: None,
            verified_at: None,
        };
        let map = VerifiedDevicesMap {
            user_id: "@alice:example.com".to_string(),
            verified_devices: vec![result],
        };
        let json = serde_json::to_string(&map).unwrap();
        let restored: VerifiedDevicesMap = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.verified_devices.len(), 1);
    }
}
