use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageKey {
    pub key_id: String,
    pub user_id: String,
    pub algorithm: String,
    pub encrypted_key: String,
    pub public_key: Option<String>,
    pub signatures: serde_json::Value,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageKeyCreationTerm {
    pub key_id: String,
    pub algorithm: String,
    pub key: SecretStorageKeyCreationKey,
    pub iv: Option<String>,
    pub mac: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "algorithm")]
pub enum SecretStorageKeyCreationKey {
    #[serde(rename = "org.matrix.msc2697.v1.curve25519-aes-sha2")]
    Curve25519AesSha2(Curve25519Key),
    #[serde(rename = "aes-hmac-sha2")]
    AesHmacSha2(AesHmacSha2Key),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curve25519Key {
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AesHmacSha2Key {
    pub key: String,
    pub iv: String,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageKeyInUpload {
    pub key_id: String,
    pub algorithm: String,
    pub auth_data: SecretStorageKeyAuthData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageKeyAuthData {
    pub key: String,
    pub iv: String,
    pub mac: String,
    pub signatures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSecret {
    pub secret_name: String,
    pub encrypted_secret: String,
    pub key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageGetRequest {
    pub secrets: Vec<String>,
    pub keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageGetResponse {
    pub secrets: std::collections::HashMap<String, Option<SecretResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretResult {
    #[serde(rename = "encrypted_secret")]
    pub encrypted: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageSetRequest {
    pub secret: String,
    pub encrypted_secret: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageDeleteRequest {
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageKeyInfo {
    pub key_id: String,
    pub algorithm: String,
    pub auth_data: SecretStorageKeyAuthData,
    pub tracks: Option<SecretStorageKeyTracks>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageKeyTracks {
    #[serde(rename = "m.cross-signing.self-signing")]
    pub self_signing: Option<bool>,
    #[serde(rename = "m.cross-signing.user-signing")]
    pub user_signing: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageAlgorithm {
    pub algorithm: String,
    pub config: serde_json::Value,
}

impl Default for SecretStorageAlgorithm {
    fn default() -> Self {
        Self {
            algorithm: "org.matrix.msc2697.v1.curve25519-aes-sha2".to_string(),
            config: serde_json::json!({
                "rotation_period_ms": 604800000,
                "rotation_period_steps": 1
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageEncryptionInfo {
    pub algorithm: String,
    pub master_key_id: Option<String>,
    pub key_count: std::collections::HashMap<String, u32>,
}

impl Default for SecretStorageEncryptionInfo {
    fn default() -> Self {
        Self {
            algorithm: "m.secret_storage.v1.aes-hmac-sha2".to_string(),
            master_key_id: None,
            key_count: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStorageSessionKey {
    pub key: String,
    pub iv: String,
    pub mac: String,
}

impl SecretStorageSessionKey {
    pub fn from_key_parts(key: &str, iv: &str, mac: &str) -> Self {
        Self { key: key.to_string(), iv: iv.to_string(), mac: mac.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_storage_key() {
        let key = SecretStorageKey {
            key_id: "key_1".to_string(),
            user_id: "@alice:example.com".to_string(),
            algorithm: "org.matrix.msc2697.v1.curve25519-aes-sha2".to_string(),
            encrypted_key: "encrypted_key_data".to_string(),
            public_key: Some("public_key_data".to_string()),
            signatures: serde_json::json!({"@alice:example.com": {"ed25519:DEVICE": "sig"}}),
            created_ts: 1700000000000,
        };
        assert_eq!(key.key_id, "key_1");
        assert_eq!(key.user_id, "@alice:example.com");
        assert!(key.public_key.is_some());
    }

    #[test]
    fn test_secret_storage_algorithm_default() {
        let algo = SecretStorageAlgorithm::default();
        assert_eq!(algo.algorithm, "org.matrix.msc2697.v1.curve25519-aes-sha2");
        assert!(algo.config.is_object());
    }

    #[test]
    fn test_secret_storage_encryption_info_default() {
        let info = SecretStorageEncryptionInfo::default();
        assert_eq!(info.algorithm, "m.secret_storage.v1.aes-hmac-sha2");
        assert!(info.master_key_id.is_none());
        assert!(info.key_count.is_empty());
    }

    #[test]
    fn test_secret_storage_session_key_from_parts() {
        let key = SecretStorageSessionKey::from_key_parts("enc_key", "iv_value", "mac_value");
        assert_eq!(key.key, "enc_key");
        assert_eq!(key.iv, "iv_value");
        assert_eq!(key.mac, "mac_value");
    }

    #[test]
    fn test_curve25519_key() {
        let key = Curve25519Key { key: "curve25519_key_data".to_string() };
        assert_eq!(key.key, "curve25519_key_data");
    }

    #[test]
    fn test_aes_hmac_sha2_key() {
        let key = AesHmacSha2Key {
            key: "aes_key".to_string(),
            iv: "aes_iv".to_string(),
            mac: "aes_mac".to_string(),
        };
        assert_eq!(key.key, "aes_key");
        assert_eq!(key.iv, "aes_iv");
        assert_eq!(key.mac, "aes_mac");
    }

    #[test]
    fn test_stored_secret() {
        let secret = StoredSecret {
            secret_name: "m.cross_signing.master".to_string(),
            encrypted_secret: "encrypted_data".to_string(),
            key_id: "key_1".to_string(),
        };
        assert_eq!(secret.secret_name, "m.cross_signing.master");
    }

    #[test]
    fn test_secret_storage_get_request() {
        let request = SecretStorageGetRequest {
            secrets: vec!["m.cross_signing.master".to_string()],
            keys: Some(vec!["key_1".to_string()]),
        };
        assert_eq!(request.secrets.len(), 1);
        assert!(request.keys.is_some());
    }

    #[test]
    fn test_secret_storage_set_request() {
        let request = SecretStorageSetRequest {
            secret: "m.cross_signing.master".to_string(),
            encrypted_secret: "encrypted".to_string(),
            key: "key_1".to_string(),
        };
        assert_eq!(request.secret, "m.cross_signing.master");
    }

    #[test]
    fn test_secret_storage_key_info() {
        let info = SecretStorageKeyInfo {
            key_id: "key_1".to_string(),
            algorithm: "aes-hmac-sha2".to_string(),
            auth_data: SecretStorageKeyAuthData {
                key: "key_data".to_string(),
                iv: "iv_data".to_string(),
                mac: "mac_data".to_string(),
                signatures: serde_json::json!({}),
            },
            tracks: Some(SecretStorageKeyTracks {
                self_signing: Some(true),
                user_signing: Some(false),
            }),
        };
        assert_eq!(info.key_id, "key_1");
        assert!(info.tracks.is_some());
    }
}
