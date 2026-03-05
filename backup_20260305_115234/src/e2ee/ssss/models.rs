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
        Self {
            key: key.to_string(),
            iv: iv.to_string(),
            mac: mac.to_string(),
        }
    }
}
