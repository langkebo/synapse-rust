use super::models::*;
use super::storage::SecretStorage;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::traits::DehydratedDeviceProvider;
use synapse_common::ApiError;
use x25519_dalek::{PublicKey, StaticSecret};

const SSSS_KEY_LENGTH: usize = 32;
const SSSS_IV_LENGTH: usize = 12;

#[derive(Clone)]
pub struct SecretStorageService {
    storage: SecretStorage,
    dehydrated_device_service: Option<Arc<dyn DehydratedDeviceProvider>>,
}

impl SecretStorageService {
    pub fn new(storage: SecretStorage) -> Self {
        Self { storage, dehydrated_device_service: None }
    }

    pub fn with_dehydrated_device_service(mut self, service: Arc<dyn DehydratedDeviceProvider>) -> Self {
        self.dehydrated_device_service = Some(service);
        self
    }

    pub fn create_key(&self, _user_id: &str, algorithm: &str) -> Result<SecretStorageKeyCreationTerm, ApiError> {
        let key_id = format!("{}", uuid::Uuid::new_v4());

        match algorithm {
            "org.matrix.msc2697.v1.curve25519-aes-sha2" => Self::create_curve25519_key(&key_id),
            "aes-hmac-sha2" => Ok(Self::create_aes_hmac_key(&key_id)),
            _ => Err(ApiError::bad_request(format!("Unsupported secret storage algorithm: {algorithm}"))),
        }
    }

    fn create_curve25519_key(key_id: &str) -> Result<SecretStorageKeyCreationTerm, ApiError> {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);

        let private_key_bytes = secret.as_bytes();
        let public_key_bytes = public.as_bytes();

        let private_key_base64 = BASE64.encode(private_key_bytes);
        let public_key_base64 = BASE64.encode(public_key_bytes);

        let mut iv_bytes = [0u8; SSSS_IV_LENGTH];
        OsRng.fill_bytes(&mut iv_bytes);
        let iv = BASE64.encode(iv_bytes);

        let key_data = format!("{private_key_base64}:{public_key_base64}");

        // Generate random AES-256-GCM session key
        let mut session_key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut session_key_bytes);
        let cipher = Aes256Gcm::new_from_slice(&session_key_bytes).map_err(|e| {
            tracing::error!("Cipher init failed: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, key_data.as_bytes()).map_err(|e| {
            tracing::error!("Encryption failed: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let encrypted_key = format!("{}.{}", BASE64.encode(nonce_bytes), BASE64.encode(&ciphertext));

        Ok(SecretStorageKeyCreationTerm {
            key_id: key_id.to_string(),
            algorithm: "org.matrix.msc2697.v1.curve25519-aes-sha2".to_string(),
            key: SecretStorageKeyCreationKey::Curve25519AesSha2(Curve25519Key { key: encrypted_key }),
            iv: Some(iv),
            mac: None,
        })
    }

    fn create_aes_hmac_key(key_id: &str) -> SecretStorageKeyCreationTerm {
        let mut key_bytes = [0u8; SSSS_KEY_LENGTH];
        OsRng.fill_bytes(&mut key_bytes);
        let key_base64 = BASE64.encode(key_bytes);

        let mut iv_bytes = [0u8; SSSS_IV_LENGTH];
        OsRng.fill_bytes(&mut iv_bytes);
        let iv_base64 = BASE64.encode(iv_bytes);

        let key_data = format!("{key_base64}:{iv_base64}");
        let mac = compute_hmac(&key_data, b"secure_storage_key");

        SecretStorageKeyCreationTerm {
            key_id: key_id.to_string(),
            algorithm: "aes-hmac-sha2".to_string(),
            key: SecretStorageKeyCreationKey::AesHmacSha2(AesHmacSha2Key {
                key: key_base64,
                iv: iv_base64.clone(),
                mac: BASE64.encode(&mac[..8]),
            }),
            iv: Some(iv_base64),
            mac: Some(BASE64.encode(&mac[..8])),
        }
    }

    pub async fn store_key(&self, user_id: &str, key: &SecretStorageKeyCreationTerm) -> Result<(), ApiError> {
        let encrypted_key = match &key.key {
            SecretStorageKeyCreationKey::Curve25519AesSha2(ck) => ck.key.clone(),
            SecretStorageKeyCreationKey::AesHmacSha2(ak) => ak.key.clone(),
        };

        let public_key = match &key.key {
            SecretStorageKeyCreationKey::Curve25519AesSha2(ck) => {
                if let Some(pos) = ck.key.find('.') {
                    let encrypted = &ck.key[pos + 1..];
                    if let Ok(decoded) = BASE64.decode(encrypted) {
                        if decoded.len() >= 32 {
                            Some(BASE64.encode(&decoded[..32]))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            SecretStorageKeyCreationKey::AesHmacSha2(_) => None,
        };

        let storage_key = SecretStorageKey {
            key_id: key.key_id.clone(),
            user_id: user_id.to_string(),
            algorithm: key.algorithm.clone(),
            encrypted_key,
            public_key,
            signatures: serde_json::json!({}),
            created_ts: chrono::Utc::now().timestamp_millis(),
        };

        self.storage.create_key(&storage_key).await
    }

    pub async fn get_key(&self, user_id: &str, key_id: &str) -> Result<Option<SecretStorageKey>, ApiError> {
        self.storage.get_key(user_id, key_id).await
    }

    pub async fn get_all_keys(&self, user_id: &str) -> Result<Vec<SecretStorageKey>, ApiError> {
        self.storage.get_all_keys(user_id).await
    }

    pub async fn delete_key(&self, user_id: &str, key_id: &str) -> Result<(), ApiError> {
        self.storage.delete_key(user_id, key_id).await?;
        if let Some(dehydrated_device_service) = &self.dehydrated_device_service {
            dehydrated_device_service.delete_dehydrated_device(user_id, "").await?;
        }
        Ok(())
    }

    pub fn encrypt_secret(&self, secret: &str, _key_id: &str, key_data: &SecretStorageKey) -> Result<String, ApiError> {
        match key_data.algorithm.as_str() {
            "org.matrix.msc2697.v1.curve25519-aes-sha2" => Self::encrypt_secret_curve25519(secret, key_data),
            "aes-hmac-sha2" => Self::encrypt_secret_aes_hmac(secret, key_data),
            _ => Err(ApiError::bad_request(format!("Unsupported algorithm: {}", key_data.algorithm))),
        }
    }

    fn encrypt_secret_curve25519(secret: &str, key_data: &SecretStorageKey) -> Result<String, ApiError> {
        let parts: Vec<&str> = key_data.encrypted_key.split('.').collect();
        if parts.len() != 2 {
            return Err(ApiError::bad_request("Invalid encrypted key format".to_string()));
        }

        let nonce_bytes =
            BASE64.decode(parts[0]).map_err(|e| ApiError::bad_request(format!("Invalid nonce base64: {e}")))?;
        let ciphertext_bytes =
            BASE64.decode(parts[1]).map_err(|e| ApiError::bad_request(format!("Invalid ciphertext base64: {e}")))?;

        if nonce_bytes.len() < 12 {
            return Err(ApiError::bad_request("Nonce too short".to_string()));
        }

        let nonce = Nonce::from_slice(&nonce_bytes[..12]);

        if ciphertext_bytes.len() < 32 {
            return Err(ApiError::bad_request("Ciphertext too short to extract key".to_string()));
        }

        let cipher = Aes256Gcm::new_from_slice(&ciphertext_bytes[..32]).map_err(|e| {
            tracing::error!("Cipher init failed: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let encrypted = cipher.encrypt(nonce, secret.as_bytes()).map_err(|e| {
            tracing::error!("Encryption failed: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(BASE64.encode(&encrypted))
    }

    fn encrypt_secret_aes_hmac(secret: &str, key_data: &SecretStorageKey) -> Result<String, ApiError> {
        let parts: Vec<&str> = key_data.encrypted_key.split(':').collect();
        if parts.len() != 2 {
            return Err(ApiError::bad_request("Invalid key format".to_string()));
        }

        let key_bytes =
            BASE64.decode(parts[0]).map_err(|e| ApiError::bad_request(format!("Invalid key base64: {e}")))?;

        let mut key_arr = [0u8; 32];
        if key_bytes.len() >= 32 {
            key_arr.copy_from_slice(&key_bytes[..32]);
        } else {
            key_arr[..key_bytes.len()].copy_from_slice(&key_bytes);
        }

        let cipher = Aes256Gcm::new_from_slice(&key_arr).map_err(|e| {
            tracing::error!("Cipher init failed: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted = cipher.encrypt(nonce, secret.as_bytes()).map_err(|e| {
            tracing::error!("Encryption failed: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        // Prepend nonce to ciphertext for storage
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&encrypted);
        Ok(BASE64.encode(&result))
    }

    pub async fn store_secret(
        &self,
        user_id: &str,
        secret_name: &str,
        encrypted_secret: &str,
        key_id: &str,
    ) -> Result<(), ApiError> {
        let secret = StoredSecret {
            secret_name: secret_name.to_string(),
            encrypted_secret: encrypted_secret.to_string(),
            key_id: key_id.to_string(),
        };

        self.storage.store_secret(user_id, &secret).await
    }

    pub async fn get_secret(&self, user_id: &str, secret_name: &str) -> Result<Option<StoredSecret>, ApiError> {
        self.storage.get_secret(user_id, secret_name).await
    }

    pub async fn get_secrets(
        &self,
        user_id: &str,
        secret_names: &[String],
    ) -> Result<HashMap<String, Option<String>>, ApiError> {
        let secrets = self.storage.get_secrets(user_id, secret_names).await?;

        let mut result = HashMap::new();
        for name in secret_names {
            let encrypted = secrets.iter().find(|s| s.secret_name == *name).map(|s| s.encrypted_secret.clone());
            result.insert(name.clone(), encrypted);
        }

        Ok(result)
    }

    pub async fn delete_secret(&self, user_id: &str, secret_name: &str) -> Result<(), ApiError> {
        self.storage.delete_secret(user_id, secret_name).await
    }

    pub async fn delete_secrets(&self, user_id: &str, secret_names: &[String]) -> Result<(), ApiError> {
        self.storage.delete_secrets(user_id, secret_names).await
    }

    pub async fn has_secrets(&self, user_id: &str) -> Result<bool, ApiError> {
        self.storage.has_secrets(user_id).await
    }

    pub fn get_encryption_info(&self, _user_id: &str) -> Result<SecretStorageEncryptionInfo, ApiError> {
        Ok(SecretStorageEncryptionInfo::default())
    }
}

fn compute_hmac(data: &str, key: &[u8]) -> [u8; 32] {
    let digest = synapse_common::crypto::hmac_sha256(key, data.as_bytes());
    let mut result = [0u8; 32];
    result.copy_from_slice(&digest[..32]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_create_key() {
        let pool = sqlx::PgPool::connect(
            &std::env::var("TEST_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()),
        )
        .await
        .expect("Failed to connect to test database");
        let storage = SecretStorage::new(&pool);
        let service = SecretStorageService::new(storage);

        let result = service.create_key("@test:example.com", "aes-hmac-sha2");
        assert!(result.is_ok());
    }
}
