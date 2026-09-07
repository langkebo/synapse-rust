use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageKey` type.
pub struct SecretStorageKey {
    /// The `key_id` field.
    /// The `user_id` field.
    /// The `algorithm` field.
    /// The `encrypted_key` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub key_id: String,
    /// The `user_id` field.
    /// The `algorithm` field.
    /// The `encrypted_key` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub user_id: String,
    /// The `algorithm` field.
    /// The `encrypted_key` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub algorithm: String,
    /// The `encrypted_key` field.
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub encrypted_key: String,
    /// The `public_key` field.
    /// The `signatures` field.
    /// The `created_ts` field.
    pub public_key: Option<String>,
    /// The `signatures` field.
    /// The `created_ts` field.
    pub signatures: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageKeyCreationTerm` type.
pub struct SecretStorageKeyCreationTerm {
    /// The `key_id` field.
    /// The `algorithm` field.
    /// The `key` field.
    /// The `iv` field.
    /// The `mac` field.
    pub key_id: String,
    /// The `algorithm` field.
    /// The `key` field.
    /// The `iv` field.
    /// The `mac` field.
    pub algorithm: String,
    /// The `key` field.
    /// The `iv` field.
    /// The `mac` field.
    pub key: SecretStorageKeyCreationKey,
    /// The `iv` field.
    /// The `mac` field.
    pub iv: Option<String>,
    /// The `mac` field.
    pub mac: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "algorithm")]
/// The `SecretStorageKeyCreationKey` enum.
pub enum SecretStorageKeyCreationKey {
    /// Curve25519-AES-SHA2 (MSC2697 v1) secret storage key.
    #[serde(rename = "org.matrix.msc2697.v1.curve25519-aes-sha2")]
    Curve25519AesSha2(Curve25519Key),
    /// AES-HMAC-SHA2 secret storage key.
    #[serde(rename = "aes-hmac-sha2")]
    AesHmacSha2(AesHmacSha2Key),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `Curve25519Key` type.
pub struct Curve25519Key {
    /// The `key` field.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `AesHmacSha2Key` type.
pub struct AesHmacSha2Key {
    /// The `key` field.
    /// The `iv` field.
    /// The `mac` field.
    pub key: String,
    /// The `iv` field.
    /// The `mac` field.
    pub iv: String,
    /// The `mac` field.
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageKeyInUpload` type.
pub struct SecretStorageKeyInUpload {
    /// The `key_id` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    pub key_id: String,
    /// The `algorithm` field.
    /// The `auth_data` field.
    pub algorithm: String,
    /// The `auth_data` field.
    pub auth_data: SecretStorageKeyAuthData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageKeyAuthData` type.
pub struct SecretStorageKeyAuthData {
    /// The `key` field.
    /// The `iv` field.
    /// The `mac` field.
    /// The `signatures` field.
    pub key: String,
    /// The `iv` field.
    /// The `mac` field.
    /// The `signatures` field.
    pub iv: String,
    /// The `mac` field.
    /// The `signatures` field.
    pub mac: String,
    /// The `signatures` field.
    pub signatures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `StoredSecret` type.
pub struct StoredSecret {
    /// The `secret_name` field.
    /// The `encrypted_secret` field.
    /// The `key_id` field.
    pub secret_name: String,
    /// The `encrypted_secret` field.
    /// The `key_id` field.
    pub encrypted_secret: String,
    /// The `key_id` field.
    pub key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageGetRequest` type.
pub struct SecretStorageGetRequest {
    /// The `secrets` field.
    /// The `keys` field.
    pub secrets: Vec<String>,
    /// The `keys` field.
    pub keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageGetResponse` type.
pub struct SecretStorageGetResponse {
    /// The `secrets` field.
    pub secrets: std::collections::HashMap<String, Option<SecretResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretResult` type.
pub struct SecretResult {
    /// The `encrypted` field.
    #[serde(rename = "encrypted_secret")]
    pub encrypted: String,
    /// The `key` field.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageSetRequest` type.
pub struct SecretStorageSetRequest {
    /// The `secret` field.
    /// The `encrypted_secret` field.
    /// The `key` field.
    pub secret: String,
    /// The `encrypted_secret` field.
    /// The `key` field.
    pub encrypted_secret: String,
    /// The `key` field.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageDeleteRequest` type.
pub struct SecretStorageDeleteRequest {
    /// The `secrets` field.
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageKeyInfo` type.
pub struct SecretStorageKeyInfo {
    /// The `key_id` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `tracks` field.
    pub key_id: String,
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `tracks` field.
    pub algorithm: String,
    /// The `auth_data` field.
    /// The `tracks` field.
    pub auth_data: SecretStorageKeyAuthData,
    /// The `tracks` field.
    pub tracks: Option<SecretStorageKeyTracks>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageKeyTracks` type.
pub struct SecretStorageKeyTracks {
    /// The `self_signing` field.
    #[serde(rename = "m.cross-signing.self-signing")]
    pub self_signing: Option<bool>,
    #[serde(rename = "m.cross-signing.user-signing")]
    /// The `user_signing` field.
    pub user_signing: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecretStorageAlgorithm` type.
pub struct SecretStorageAlgorithm {
    /// The `algorithm` field.
    /// The `config` field.
    pub algorithm: String,
    /// The `config` field.
    pub config: serde_json::Value,
}

/// (see code)
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
/// The `SecretStorageEncryptionInfo` type.
pub struct SecretStorageEncryptionInfo {
    /// The `algorithm` field.
    /// The `master_key_id` field.
    /// The `key_count` field.
    pub algorithm: String,
    /// The `master_key_id` field.
    /// The `key_count` field.
    pub master_key_id: Option<String>,
    /// The `key_count` field.
    pub key_count: std::collections::HashMap<String, u32>,
}

/// (see code)
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
/// The `SecretStorageSessionKey` type.
pub struct SecretStorageSessionKey {
    /// The `key` field.
    /// The `iv` field.
    /// The `mac` field.
    pub key: String,
    /// The `iv` field.
    /// The `mac` field.
    pub iv: String,
    /// The `mac` field.
    pub mac: String,
}

/// (see code)
impl SecretStorageSessionKey {
    /// See [`from_key_parts`].
    pub fn from_key_parts(key: &str, iv: &str, mac: &str) -> Self {
        Self { key: key.to_string(), iv: iv.to_string(), mac: mac.to_string() }
    }
}
