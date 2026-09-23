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
///
/// Only `m.secret_storage.v1.aes-hmac-sha2` is representable: the former
/// MSC2697 `curve25519-aes-sha2` variant was removed together with its broken
/// server-side crypto (it derived the AES key from the ciphertext itself, so no
/// conforming client could ever decrypt a secret). A curve25519-bound key must
/// be generated client-side; the server only stores its public description.
pub enum SecretStorageKeyCreationKey {
    /// AES-HMAC-SHA2 secret storage key.
    #[serde(rename = "aes-hmac-sha2")]
    AesHmacSha2(AesHmacSha2Key),
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

/// One encrypted secret, in the shape the spec requires for the
/// `m.secret_storage.v1.aes-hmac-sha2` algorithm (`iv` / `ciphertext` / `mac`,
/// all unpadded base64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AesHmacSha2EncryptedData {
    /// The 16-byte AES-CTR initialization vector, unpadded base64.
    pub iv: String,
    /// The AES-256-CTR ciphertext, unpadded base64.
    pub ciphertext: String,
    /// HMAC-SHA-256 over the raw ciphertext, unpadded base64.
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

/// Default implementation for [`SecretStorageAlgorithm`].
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

/// Default implementation for [`SecretStorageEncryptionInfo`].
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

/// Implementation of [`SecretStorageSessionKey`] methods.
impl SecretStorageSessionKey {
    /// See [`from_key_parts`].
    pub fn from_key_parts(key: &str, iv: &str, mac: &str) -> Self {
        Self { key: key.to_string(), iv: iv.to_string(), mac: mac.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_storage_session_key_from_key_parts() {
        let sk = SecretStorageSessionKey::from_key_parts("k", "iv", "mac");
        assert_eq!(sk.key, "k");
        assert_eq!(sk.iv, "iv");
        assert_eq!(sk.mac, "mac");
    }

    #[test]
    fn default_algorithm_is_curve25519_aes_sha2() {
        let algo = SecretStorageAlgorithm::default();
        assert_eq!(algo.algorithm, "org.matrix.msc2697.v1.curve25519-aes-sha2");
        assert!(algo.config.get("rotation_period_ms").is_some());
        assert!(algo.config.get("rotation_period_steps").is_some());
    }

    #[test]
    fn default_encryption_info_is_aes_hmac_sha2() {
        let info = SecretStorageEncryptionInfo::default();
        assert_eq!(info.algorithm, "m.secret_storage.v1.aes-hmac-sha2");
        assert!(info.master_key_id.is_none());
        assert!(info.key_count.is_empty());
    }

    #[test]
    fn serialization_roundtrip_secret_storage_key() {
        let key = SecretStorageKey {
            key_id: "key_id".to_string(),
            user_id: "@user:example.org".to_string(),
            algorithm: "m.secret_storage.v1.aes-hmac-sha2".to_string(),
            encrypted_key: "enc".to_string(),
            public_key: Some("pub".to_string()),
            signatures: serde_json::json!({}),
            created_ts: 1000,
        };
        let json = serde_json::to_string(&key).unwrap();
        let rt: SecretStorageKey = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.key_id, "key_id");
        assert_eq!(rt.algorithm, "m.secret_storage.v1.aes-hmac-sha2");
        assert_eq!(rt.encrypted_key, "enc");
        assert_eq!(rt.public_key, Some("pub".to_string()));
        assert_eq!(rt.created_ts, 1000);
    }

    #[test]
    fn creation_key_enum_tagged_variants() {
        let aes = SecretStorageKeyCreationKey::AesHmacSha2(AesHmacSha2Key {
            key: "ak".to_string(),
            iv: "iv".to_string(),
            mac: "mac".to_string(),
        });
        let json_aes = serde_json::to_string(&aes).unwrap();
        assert!(json_aes.contains("aes-hmac-sha2"));

        let rt: SecretStorageKeyCreationKey = serde_json::from_str(&json_aes).unwrap();
        match rt {
            SecretStorageKeyCreationKey::AesHmacSha2(k) => assert_eq!(k.key, "ak"),
        }
    }

    #[test]
    fn encrypted_data_roundtrips_spec_fields() {
        let data =
            AesHmacSha2EncryptedData { iv: "aXY".to_string(), ciphertext: "Y3Q".to_string(), mac: "bWFj".to_string() };
        let json = serde_json::to_string(&data).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v.get("iv").and_then(|x| x.as_str()), Some("aXY"));
        assert_eq!(v.get("ciphertext").and_then(|x| x.as_str()), Some("Y3Q"));
        assert_eq!(v.get("mac").and_then(|x| x.as_str()), Some("bWFj"));
    }

    #[test]
    fn secret_result_uses_encrypted_secret_alias() {
        let result = SecretResult { encrypted: "enc_secret".to_string(), key: "key1".to_string() };
        let json = serde_json::to_string(&result).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v.get("encrypted_secret").and_then(|x| x.as_str()), Some("enc_secret"));
        assert_eq!(v.get("key").and_then(|x| x.as_str()), Some("key1"));

        let rt: SecretResult = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.encrypted, "enc_secret");
    }

    #[test]
    fn secret_storage_key_tracks_rename() {
        let tracks = SecretStorageKeyTracks { self_signing: Some(true), user_signing: Some(false) };
        let json = serde_json::to_string(&tracks).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v.get("m.cross-signing.self-signing").and_then(|x| x.as_bool()), Some(true));
        assert_eq!(v.get("m.cross-signing.user-signing").and_then(|x| x.as_bool()), Some(false));
    }

    #[test]
    fn stored_secret_roundtrip() {
        let secret = StoredSecret {
            secret_name: "m.cross_signing.master".to_string(),
            encrypted_secret: "enc_data".to_string(),
            key_id: "key1".to_string(),
        };
        let json = serde_json::to_string(&secret).unwrap();
        let rt: StoredSecret = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.secret_name, "m.cross_signing.master");
        assert_eq!(rt.encrypted_secret, "enc_data");
        assert_eq!(rt.key_id, "key1");
    }

    #[test]
    fn secret_storage_get_request_optional_keys() {
        let req_with_keys = SecretStorageGetRequest {
            secrets: vec!["m.cross_signing.master".to_string()],
            keys: Some(vec!["key1".to_string()]),
        };
        let json = serde_json::to_string(&req_with_keys).unwrap();
        let rt: SecretStorageGetRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.keys, Some(vec!["key1".to_string()]));

        let req_without_keys =
            SecretStorageGetRequest { secrets: vec!["m.cross_signing.master".to_string()], keys: None };
        let json2 = serde_json::to_string(&req_without_keys).unwrap();
        let rt2: SecretStorageGetRequest = serde_json::from_str(&json2).unwrap();
        assert!(rt2.keys.is_none());
    }

    #[test]
    fn secret_storage_get_response_with_none_value() {
        use std::collections::HashMap;
        let mut secrets = HashMap::new();
        secrets.insert(
            "secret1".to_string(),
            Some(SecretResult { encrypted: "enc1".to_string(), key: "key1".to_string() }),
        );
        secrets.insert("missing".to_string(), None);

        let resp = SecretStorageGetResponse { secrets };
        let json = serde_json::to_string(&resp).unwrap();
        let rt: SecretStorageGetResponse = serde_json::from_str(&json).unwrap();
        assert!(rt.secrets.contains_key("missing"));
        assert!(rt.secrets.get("missing").unwrap().is_none());
        assert!(rt.secrets.get("secret1").unwrap().is_some());
    }

    #[test]
    fn secret_storage_key_info_serialization() {
        let info = SecretStorageKeyInfo {
            key_id: "key1".to_string(),
            algorithm: "m.secret_storage.v1.aes-hmac-sha2".to_string(),
            auth_data: SecretStorageKeyAuthData {
                key: "k".to_string(),
                iv: "iv".to_string(),
                mac: "mac".to_string(),
                signatures: serde_json::json!({}),
            },
            tracks: Some(SecretStorageKeyTracks { self_signing: Some(true), user_signing: None }),
        };
        let json = serde_json::to_string(&info).unwrap();
        let rt: SecretStorageKeyInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.key_id, "key1");
        assert!(rt.tracks.is_some());
        assert_eq!(rt.tracks.unwrap().self_signing, Some(true));
    }
}
