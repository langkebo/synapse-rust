//! Secure Secret Storage (SSSS) — `m.secret_storage.v1.aes-hmac-sha2`.
//!
//! ## Algorithm (normative)
//!
//! Key derivation — HKDF-SHA256 with **salt = 32 zero bytes**:
//! * key-validation check (`iv`/`mac` on `m.secret_storage.key.<id>`): `info` is
//!   the **empty string**;
//! * secret encryption: `info` is the **secret name** (the account-data event
//!   type, e.g. `m.cross_signing.master`).
//!
//! HKDF output is 64 bytes: the first 32 are the AES key, the last 32 are the
//! MAC key.
//!
//! Encryption is **AES-256-CTR** (full 16-byte counter block, i.e. WebCrypto's
//! `AES-CTR` with a 128-bit counter) with a random 16-byte IV whose bit 63 is
//! cleared, followed by **encrypt-then-MAC**: HMAC-SHA-256 over the raw
//! ciphertext, keyed with the derived MAC key.  `iv`/`ciphertext`/`mac` are
//! encoded as unpadded base64.
//!
//! ## What this module deliberately does NOT do
//!
//! The former MSC2697 `curve25519-aes-sha2` path is gone.  It derived the AES
//! key from the ciphertext itself (`derive_ssss_key(&ciphertext_bytes)`), so a
//! conforming client could never decrypt anything it produced, and its
//! "public key" was the first 32 bytes of that ciphertext.  A curve25519-bound
//! SSSS key must be established client-side; the server has no key material to
//! derive it from, so generating or encrypting with it now fails closed with a
//! `400` instead of emitting an undecryptable blob.

use super::models::*;
use super::storage::SecretStorage;
use aes::cipher::{generic_array::GenericArray, KeyIvInit, StreamCipher};
use base64::engine::general_purpose::{STANDARD as BASE64, STANDARD_NO_PAD as BASE64_NO_PAD};
use base64::Engine;
use rand::RngCore;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::traits::DehydratedDeviceProvider;
use synapse_common::ApiError;
#[cfg(test)]
use synapse_common::ApiErrorKind;
use zeroize::Zeroizing;

const SSSS_KEY_LENGTH: usize = 32;
/// AES-CTR uses a full 16-byte counter block, not the 12-byte nonce AES-GCM
/// used.  The matrix-spec parameters are "16 random bytes, set bit 63 to 0".
const SSSS_IV_LENGTH: usize = 16;

/// The algorithm identifier this module implements, in both the short form the
/// internal table has always used and the spec's fully-qualified form (which is
/// what clients put in `m.secret_storage.key.<id>.algorithm`).
const AES_HMAC_SHA2_SHORT: &str = "aes-hmac-sha2";
const AES_HMAC_SHA2_SPEC: &str = "m.secret_storage.v1.aes-hmac-sha2";

/// AES-256 in counter mode with a 128-bit big-endian counter — the variant
/// WebCrypto and libolm implement.
type Aes256Ctr = ctr::Ctr128BE<aes::Aes256>;

/// True when `algorithm` names `m.secret_storage.v1.aes-hmac-sha2`.
pub fn is_aes_hmac_sha2(algorithm: &str) -> bool {
    algorithm == AES_HMAC_SHA2_SHORT || algorithm == AES_HMAC_SHA2_SPEC
}

#[derive(Clone)]
/// The `SecretStorageService` type.
pub struct SecretStorageService {
    storage: SecretStorage,
    dehydrated_device_service: Option<Arc<dyn DehydratedDeviceProvider>>,
}

/// Implementation of [`SecretStorageService`] methods.
impl SecretStorageService {
    /// See [`new`].
    pub fn new(storage: SecretStorage) -> Self {
        Self { storage, dehydrated_device_service: None }
    }

    /// See [`with_dehydrated_device_service`].
    pub fn with_dehydrated_device_service(mut self, service: Arc<dyn DehydratedDeviceProvider>) -> Self {
        self.dehydrated_device_service = Some(service);
        self
    }

    /// See [`create_key`].
    ///
    /// Only `m.secret_storage.v1.aes-hmac-sha2` can be generated server-side.
    /// The MSC2697 `curve25519-aes-sha2` algorithm binds the key to a client
    /// key pair via ECDH, so the server has no input from which to derive it —
    /// requesting it is a `400`, and clients must upload such a key instead.
    pub fn create_key(&self, _user_id: &str, algorithm: &str) -> Result<SecretStorageKeyCreationTerm, ApiError> {
        let key_id = format!("{}", uuid::Uuid::new_v4());

        if is_aes_hmac_sha2(algorithm) {
            return Self::create_aes_hmac_key(&key_id);
        }

        if algorithm == "org.matrix.msc2697.v1.curve25519-aes-sha2" {
            return Err(ApiError::bad_request(
                "curve25519-aes-sha2 key generation requires client key material (ECDH); \
                 the server cannot generate it"
                    .to_string(),
            ));
        }

        Err(ApiError::bad_request(format!("Unsupported secret storage algorithm: {algorithm}")))
    }

    /// Generate a spec-compliant `m.secret_storage.v1.aes-hmac-sha2` key.
    ///
    /// Returns the raw key plus the key-validation `iv`/`mac` pair that belongs
    /// in `m.secret_storage.key.<id>` (encrypt 32 zero bytes under the derived
    /// AES key with `info = ""`, then HMAC the ciphertext under the derived MAC
    /// key).
    fn create_aes_hmac_key(key_id: &str) -> Result<SecretStorageKeyCreationTerm, ApiError> {
        let mut key_bytes = Zeroizing::new([0u8; SSSS_KEY_LENGTH]);
        rand::rng().fill_bytes(&mut *key_bytes);
        let key_base64 = BASE64_NO_PAD.encode(*key_bytes);

        let iv = generate_iv();
        let (iv_base64, mac_base64) = key_validation_mac(&key_bytes, &iv)?;

        Ok(SecretStorageKeyCreationTerm {
            key_id: key_id.to_string(),
            algorithm: AES_HMAC_SHA2_SHORT.to_string(),
            key: SecretStorageKeyCreationKey::AesHmacSha2(AesHmacSha2Key {
                key: key_base64,
                iv: iv_base64.clone(),
                mac: mac_base64.clone(),
            }),
            iv: Some(iv_base64),
            mac: Some(mac_base64),
        })
    }

    /// See [`store_key`].
    pub async fn store_key(&self, user_id: &str, key: &SecretStorageKeyCreationTerm) -> Result<(), ApiError> {
        let encrypted_key = match &key.key {
            SecretStorageKeyCreationKey::AesHmacSha2(ak) => ak.key.clone(),
        };

        let storage_key = SecretStorageKey {
            key_id: key.key_id.clone(),
            user_id: user_id.to_string(),
            algorithm: key.algorithm.clone(),
            encrypted_key,
            public_key: None,
            signatures: serde_json::json!({}),
            created_ts: current_timestamp_millis(),
        };

        self.storage.create_key(&storage_key).await
    }

    /// See [`store_account_data_key`].
    pub async fn store_account_data_key(&self, user_id: &str, key_id: &str, content: &Value) -> Result<(), ApiError> {
        let algorithm = content
            .get("algorithm")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("m.secret_storage.key event is missing algorithm".to_string()))?;

        let auth_data = content
            .get("auth_data")
            .and_then(Value::as_object)
            .ok_or_else(|| ApiError::bad_request("m.secret_storage.key event is missing auth_data".to_string()))?;

        // The internal SSSS table cannot yet preserve the full auth_data
        // payload (notably iv/mac). Keep the standard account_data event as
        // the source of truth and mirror just the fields the legacy internal
        // consumers need for existence checks and minimal metadata reads.
        let encrypted_key = auth_data.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
        let public_key = content.get("public_key").and_then(Value::as_str).map(ToOwned::to_owned);
        let signatures = auth_data.get("signatures").cloned().unwrap_or_else(|| serde_json::json!({}));

        let storage_key = SecretStorageKey {
            key_id: key_id.to_string(),
            user_id: user_id.to_string(),
            algorithm: algorithm.to_string(),
            encrypted_key,
            public_key,
            signatures,
            created_ts: current_timestamp_millis(),
        };

        self.storage.create_key(&storage_key).await
    }

    /// See [`get_key`].
    pub async fn get_key(&self, user_id: &str, key_id: &str) -> Result<Option<SecretStorageKey>, ApiError> {
        self.storage.get_key(user_id, key_id).await
    }

    /// See [`get_all_keys`].
    pub async fn get_all_keys(&self, user_id: &str) -> Result<Vec<SecretStorageKey>, ApiError> {
        self.storage.get_all_keys(user_id).await
    }

    /// See [`delete_key`].
    pub async fn delete_key(&self, user_id: &str, key_id: &str) -> Result<(), ApiError> {
        self.storage.delete_key(user_id, key_id).await?;
        if let Some(dehydrated_device_service) = &self.dehydrated_device_service {
            dehydrated_device_service.delete_dehydrated_device(user_id, "").await?;
        }
        Ok(())
    }

    /// See [`encrypt_secret`].
    ///
    /// `secret_name` is the account-data event type the secret will be stored
    /// under; it is the HKDF `info` parameter, so the same key encrypts
    /// different secrets under independent AES/MAC keys.
    pub fn encrypt_secret(
        &self,
        secret: &str,
        secret_name: &str,
        key_data: &SecretStorageKey,
    ) -> Result<AesHmacSha2EncryptedData, ApiError> {
        if !is_aes_hmac_sha2(&key_data.algorithm) {
            return Err(ApiError::bad_request(format!("Unsupported secret storage algorithm: {}", key_data.algorithm)));
        }

        Self::encrypt_secret_aes_hmac(secret, secret_name, key_data)
    }

    /// Encrypt one secret as `{iv, ciphertext, mac}` (all unpadded base64).
    fn encrypt_secret_aes_hmac(
        secret: &str,
        secret_name: &str,
        key_data: &SecretStorageKey,
    ) -> Result<AesHmacSha2EncryptedData, ApiError> {
        let key_bytes = parse_raw_key(key_data)?;
        let iv = generate_iv();
        let (aes_key, mac_key) = derive_keys(&key_bytes, secret_name)?;

        let ciphertext = aes_ctr_apply(&aes_key, &iv, secret.as_bytes());
        let mac = synapse_common::crypto::hmac_sha256(&*mac_key, &ciphertext);

        Ok(AesHmacSha2EncryptedData {
            iv: BASE64_NO_PAD.encode(iv),
            ciphertext: BASE64_NO_PAD.encode(&ciphertext),
            mac: BASE64_NO_PAD.encode(mac),
        })
    }

    /// Decrypt one `m.secret_storage.v1.aes-hmac-sha2` secret, verifying the MAC
    /// **before** decrypting (encrypt-then-MAC).
    ///
    /// A MAC mismatch is a `403`: the ciphertext must not be used.
    pub fn decrypt_secret(
        &self,
        encrypted: &AesHmacSha2EncryptedData,
        secret_name: &str,
        key_data: &SecretStorageKey,
    ) -> Result<String, ApiError> {
        if !is_aes_hmac_sha2(&key_data.algorithm) {
            return Err(ApiError::bad_request(format!("Unsupported secret storage algorithm: {}", key_data.algorithm)));
        }

        let key_bytes = parse_raw_key(key_data)?;
        let iv = decode_iv(&encrypted.iv)?;
        let ciphertext = BASE64_NO_PAD
            .decode(&encrypted.ciphertext)
            .map_err(|e| ApiError::bad_request(format!("Invalid ciphertext base64: {e}")))?;
        let mac = BASE64_NO_PAD
            .decode(&encrypted.mac)
            .map_err(|e| ApiError::bad_request(format!("Invalid mac base64: {e}")))?;

        let (aes_key, mac_key) = derive_keys(&key_bytes, secret_name)?;
        let expected = synapse_common::crypto::hmac_sha256(&*mac_key, &ciphertext);
        if !synapse_common::crypto::secure_compare_bytes(&expected, &mac) {
            return Err(ApiError::forbidden("Secret storage MAC verification failed".to_string()));
        }

        let plaintext = aes_ctr_apply(&aes_key, &iv, &ciphertext);
        String::from_utf8(plaintext).map_err(|e| ApiError::bad_request(format!("Decrypted secret is not UTF-8: {e}")))
    }

    /// See [`store_secret`].
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

    /// See [`get_secret`].
    pub async fn get_secret(&self, user_id: &str, secret_name: &str) -> Result<Option<StoredSecret>, ApiError> {
        self.storage.get_secret(user_id, secret_name).await
    }

    /// See [`get_secrets`].
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

    /// See [`delete_secret`].
    pub async fn delete_secret(&self, user_id: &str, secret_name: &str) -> Result<(), ApiError> {
        self.storage.delete_secret(user_id, secret_name).await
    }

    /// See [`delete_secrets`].
    pub async fn delete_secrets(&self, user_id: &str, secret_names: &[String]) -> Result<(), ApiError> {
        self.storage.delete_secrets(user_id, secret_names).await
    }

    /// See [`has_secrets`].
    pub async fn has_secrets(&self, user_id: &str) -> Result<bool, ApiError> {
        self.storage.has_secrets(user_id).await
    }

    /// See [`get_encryption_info`].
    pub fn get_encryption_info(&self, _user_id: &str) -> Result<SecretStorageEncryptionInfo, ApiError> {
        Ok(SecretStorageEncryptionInfo::default())
    }
}

/// Decode the raw 32-byte SSSS key from `encrypted_key`.
///
/// `encrypted_key` is the spec's `auth_data.key`: unpadded base64 of the raw
/// key.  A legacy `<key>:<iv>` suffix (written by the old implementation) is
/// tolerated by taking everything before the first `:`.
fn parse_raw_key(key_data: &SecretStorageKey) -> Result<Zeroizing<[u8; SSSS_KEY_LENGTH]>, ApiError> {
    let key_base64 = key_data.encrypted_key.split(':').next().unwrap_or_default();
    let key_bytes = BASE64_NO_PAD
        .decode(key_base64)
        .or_else(|_| BASE64.decode(key_base64))
        .map_err(|e| ApiError::bad_request(format!("Invalid key base64: {e}")))?;

    if key_bytes.len() != SSSS_KEY_LENGTH {
        return Err(ApiError::bad_request(format!(
            "Secret storage key must be exactly {SSSS_KEY_LENGTH} bytes, got {}",
            key_bytes.len()
        )));
    }

    let mut out = Zeroizing::new([0u8; SSSS_KEY_LENGTH]);
    out.copy_from_slice(&key_bytes);
    Ok(out)
}

/// Decode a base64 `iv` field that must be exactly 16 bytes, accepting both
/// padded and unpadded input (the spec requires clients to accept both).
fn decode_iv(value: &str) -> Result<[u8; SSSS_IV_LENGTH], ApiError> {
    let bytes = BASE64_NO_PAD
        .decode(value)
        .or_else(|_| BASE64.decode(value))
        .map_err(|e| ApiError::bad_request(format!("Invalid iv base64: {e}")))?;
    if bytes.len() != SSSS_IV_LENGTH {
        return Err(ApiError::bad_request(format!("iv must be {SSSS_IV_LENGTH} bytes, got {}", bytes.len())));
    }
    let mut out = [0u8; SSSS_IV_LENGTH];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Generate a 16-byte IV with bit 63 cleared, as the spec mandates (to work
/// around differing AES-CTR counter-overflow behaviour).
fn generate_iv() -> [u8; SSSS_IV_LENGTH] {
    let mut iv = [0u8; SSSS_IV_LENGTH];
    rand::rng().fill_bytes(&mut iv);
    // Bit 63 is the most significant bit of byte 8 in the big-endian block.
    iv[8] &= 0x7F;
    iv
}

/// HKDF-SHA256 with a 32-byte zero salt, expanded to 64 bytes and split into
/// the AES key (first 32) and the MAC key (last 32).
fn derive_keys(key: &[u8], info: &str) -> Result<(Zeroizing<[u8; 32]>, Zeroizing<[u8; 32]>), ApiError> {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&[0u8; 32]), key);
    let mut okm = Zeroizing::new([0u8; 64]);
    hk.expand(info.as_bytes(), &mut *okm).map_err(|e| {
        tracing::error!("HKDF key derivation failed: {e}");
        ApiError::internal("HKDF key derivation failed")
    })?;

    let mut aes_key = Zeroizing::new([0u8; 32]);
    aes_key.copy_from_slice(&okm[..32]);
    let mut mac_key = Zeroizing::new([0u8; 32]);
    mac_key.copy_from_slice(&okm[32..]);
    Ok((aes_key, mac_key))
}

/// Apply AES-256-CTR (128-bit big-endian counter) to `data`.
fn aes_ctr_apply(key: &[u8; 32], iv: &[u8; SSSS_IV_LENGTH], data: &[u8]) -> Vec<u8> {
    let mut cipher = Aes256Ctr::new(GenericArray::from_slice(key), GenericArray::from_slice(iv));
    let mut buffer = data.to_vec();
    cipher.apply_keystream(&mut buffer);
    buffer
}

/// The key-validation `(iv, mac)` pair: encrypt 32 zero bytes with
/// `info = ""`, then HMAC the raw ciphertext under the derived MAC key.
fn key_validation_mac(key: &[u8], iv: &[u8; SSSS_IV_LENGTH]) -> Result<(String, String), ApiError> {
    let (aes_key, mac_key) = derive_keys(key, "")?;
    let ciphertext = aes_ctr_apply(&aes_key, iv, &[0u8; 32]);
    let mac = synapse_common::crypto::hmac_sha256(&*mac_key, &ciphertext);
    Ok((BASE64_NO_PAD.encode(iv), BASE64_NO_PAD.encode(mac)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::crypto::hmac_sha256;

    fn lazy_service() -> SecretStorageService {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
        let pool = sqlx::PgPool::connect_lazy(&database_url).expect("connect_lazy should not perform I/O");
        SecretStorageService::new(SecretStorage::new(&pool))
    }

    fn key_description(raw_key: &[u8; 32]) -> SecretStorageKey {
        SecretStorageKey {
            key_id: "test-key".to_string(),
            user_id: "@test:example.com".to_string(),
            algorithm: AES_HMAC_SHA2_SPEC.to_string(),
            encrypted_key: BASE64_NO_PAD.encode(raw_key),
            public_key: None,
            signatures: serde_json::json!({}),
            created_ts: 0,
        }
    }

    // ── AES-256-CTR primitive ────────────────────────────────────────
    //
    // NIST SP 800-38A, F.5.5 CTR-AES256.Encrypt, block 1.  This pins the mode
    // *and* the counter orientation: a little-endian counter implementation
    // produces a different first block and fails.
    #[test]
    fn aes_256_ctr_matches_nist_sp800_38a_vector() {
        let key: [u8; 32] = [
            0x60, 0x3d, 0xeb, 0x10, 0x15, 0xca, 0x71, 0xbe, 0x2b, 0x73, 0xae, 0xf0, 0x85, 0x7d, 0x77, 0x81, 0x1f, 0x35,
            0x2c, 0x07, 0x3b, 0x61, 0x08, 0xd7, 0x2d, 0x98, 0x10, 0xa3, 0x09, 0x14, 0xdf, 0xf4,
        ];
        let iv: [u8; 16] =
            [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff];
        let plaintext: [u8; 16] =
            [0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93, 0x17, 0x2a];
        let expected: [u8; 16] =
            [0x60, 0x1e, 0xc3, 0x13, 0x77, 0x57, 0x89, 0xa5, 0xb7, 0xa7, 0xf5, 0x04, 0xbb, 0xf3, 0xd2, 0x28];

        assert_eq!(aes_ctr_apply(&key, &iv, &plaintext), expected.to_vec());
    }

    // ── key generation / validation ──────────────────────────────────

    #[test]
    fn create_aes_hmac_key_is_unpadded_and_has_validation_mac() {
        let key = SecretStorageService::create_aes_hmac_key("k1").expect("key generation must succeed");
        assert_eq!(key.algorithm, AES_HMAC_SHA2_SHORT);

        let SecretStorageKeyCreationKey::AesHmacSha2(inner) = &key.key;
        // Unpadded base64: 32 bytes -> 43 chars, 16 bytes -> 22 chars.
        assert_eq!(inner.key.len(), 43, "raw key must be unpadded base64 of 32 bytes");
        assert_eq!(inner.iv.len(), 22, "iv must be unpadded base64 of 16 bytes");
        assert!(!inner.key.contains('=') && !inner.iv.contains('=') && !inner.mac.contains('='));
        assert_eq!(key.iv.as_deref(), Some(inner.iv.as_str()));
        assert_eq!(key.mac.as_deref(), Some(inner.mac.as_str()));
    }

    #[test]
    fn create_key_rejects_curve25519_algorithm() {
        let service = lazy_service();
        let err = service
            .create_key("@test:example.com", "org.matrix.msc2697.v1.curve25519-aes-sha2")
            .expect_err("curve25519 key generation must fail closed");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert!(err.message.contains("curve25519"), "unexpected message: {}", err.message);
    }

    #[test]
    fn create_key_rejects_unknown_algorithm() {
        let service = lazy_service();
        let err =
            service.create_key("@test:example.com", "unknown-algorithm").expect_err("unknown algorithm must fail");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
    }

    #[test]
    fn create_key_accepts_both_aes_hmac_sha2_spellings() {
        let service = lazy_service();
        for algorithm in [AES_HMAC_SHA2_SHORT, AES_HMAC_SHA2_SPEC] {
            let key = service.create_key("@test:example.com", algorithm).expect("must generate");
            assert_eq!(key.algorithm, AES_HMAC_SHA2_SHORT);
        }
    }

    // ── IV rule ──────────────────────────────────────────────────────

    #[test]
    fn generated_iv_has_bit_63_cleared() {
        for _ in 0..64 {
            let iv = generate_iv();
            assert_eq!(iv.len(), SSSS_IV_LENGTH);
            assert_eq!(iv[8] & 0x80, 0, "bit 63 of the counter block must be 0");
        }
    }

    // ── HKDF split ───────────────────────────────────────────────────

    #[test]
    fn derive_keys_splits_hkdf_output_and_is_info_separated() {
        let raw = [0x42u8; 32];
        let (aes_a, mac_a) = derive_keys(&raw, "m.cross_signing.master").expect("derive");
        let (aes_b, mac_b) = derive_keys(&raw, "m.cross_signing.self_signing").expect("derive");

        // Different `info` => independent key material (the secret name is the
        // domain separator, so one leaked secret key cannot decrypt another).
        assert_ne!(&aes_a[..], &aes_b[..]);
        assert_ne!(&mac_a[..], &mac_b[..]);

        // The AES and MAC halves are distinct, and neither equals the input.
        assert_ne!(&aes_a[..], &mac_a[..]);
        assert_ne!(&aes_a[..], &raw[..]);
        assert_ne!(&mac_a[..], &raw[..]);

        // Deterministic.
        let (aes_c, mac_c) = derive_keys(&raw, "m.cross_signing.master").expect("derive");
        assert_eq!(&aes_a[..], &aes_c[..]);
        assert_eq!(&mac_a[..], &mac_c[..]);
    }

    // ── encrypt / decrypt round trip ─────────────────────────────────

    #[test]
    fn encrypt_decrypt_roundtrip_verifies_mac() {
        let service = lazy_service();
        let raw = [0x11u8; 32];
        let key_data = key_description(&raw);

        let encrypted =
            service.encrypt_secret("top secret value", "m.cross_signing.master", &key_data).expect("encrypt");
        assert!(!encrypted.iv.contains('='));
        assert!(!encrypted.ciphertext.contains('='));
        assert!(!encrypted.mac.contains('='));

        let plaintext = service.decrypt_secret(&encrypted, "m.cross_signing.master", &key_data).expect("decrypt");
        assert_eq!(plaintext, "top secret value");
    }

    #[test]
    fn decrypt_rejects_tampered_ciphertext() {
        let service = lazy_service();
        let raw = [0x22u8; 32];
        let key_data = key_description(&raw);

        let mut encrypted =
            service.encrypt_secret("top secret value", "m.cross_signing.master", &key_data).expect("encrypt");

        // Flip one ciphertext byte: the MAC must no longer verify, and the
        // failure must be fail-closed (403) rather than returning garbage.
        let mut bytes = BASE64_NO_PAD.decode(&encrypted.ciphertext).expect("decode");
        bytes[0] ^= 0x01;
        encrypted.ciphertext = BASE64_NO_PAD.encode(&bytes);

        let err = service
            .decrypt_secret(&encrypted, "m.cross_signing.master", &key_data)
            .expect_err("tampered ciphertext must be rejected");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
    }

    #[test]
    fn decrypt_rejects_wrong_secret_name() {
        let service = lazy_service();
        let raw = [0x33u8; 32];
        let key_data = key_description(&raw);

        let encrypted = service.encrypt_secret("v", "m.cross_signing.master", &key_data).expect("encrypt");
        // The secret name is the HKDF info, so a wrong name derives a wrong MAC
        // key and must be rejected.
        let err = service
            .decrypt_secret(&encrypted, "m.cross_signing.self_signing", &key_data)
            .expect_err("wrong secret name must fail MAC verification");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
    }

    #[test]
    fn mac_is_keyed_by_derived_mac_key_not_raw_key() {
        let service = lazy_service();
        let raw = [0x44u8; 32];
        let key_data = key_description(&raw);
        let encrypted = service.encrypt_secret("v", "name", &key_data).expect("encrypt");

        let ciphertext = BASE64_NO_PAD.decode(&encrypted.ciphertext).expect("decode");
        let (_, mac_key) = derive_keys(&raw, "name").expect("derive");
        let derived = hmac_sha256(&*mac_key, &ciphertext);
        let raw_keyed = hmac_sha256(&raw, &ciphertext);

        assert_eq!(BASE64_NO_PAD.encode(&derived), encrypted.mac);
        assert_ne!(BASE64_NO_PAD.encode(&raw_keyed), encrypted.mac, "MAC must not be keyed by the raw secret");
    }

    #[test]
    fn encrypt_rejects_non_aes_hmac_algorithm() {
        let service = lazy_service();
        let mut key_data = key_description(&[0x55u8; 32]);
        key_data.algorithm = "org.matrix.msc2697.v1.curve25519-aes-sha2".to_string();
        let err = service.encrypt_secret("v", "name", &key_data).expect_err("curve25519 encryption must fail closed");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
    }

    #[test]
    fn encrypt_rejects_short_and_long_keys() {
        let service = lazy_service();
        for length in [16usize, 31, 33, 64] {
            let mut raw = vec![0u8; length];
            raw.fill(0x66);
            let mut key_data = key_description(&[0u8; 32]);
            key_data.encrypted_key = BASE64_NO_PAD.encode(&raw);
            let err = service
                .encrypt_secret("v", "name", &key_data)
                .expect_err("non-32-byte key must be rejected, never padded");
            assert_eq!(err.kind, ApiErrorKind::BadRequest);
        }
    }

    #[test]
    fn encrypt_accepts_padded_key_input() {
        let service = lazy_service();
        let mut key_data = key_description(&[0x77u8; 32]);
        key_data.encrypted_key = BASE64.encode([0x77u8; 32]);
        let encrypted = service.encrypt_secret("v", "name", &key_data).expect("padded base64 must be accepted");
        let plaintext = service.decrypt_secret(&encrypted, "name", &key_data).expect("decrypt");
        assert_eq!(plaintext, "v");
    }

    // ── key-validation MAC ───────────────────────────────────────────

    #[test]
    fn key_validation_mac_is_reproducible_and_key_bound() {
        let raw = [0x88u8; 32];
        let iv = generate_iv();
        let (iv_b64, mac_b64) = key_validation_mac(&raw, &iv).expect("key mac");
        let (iv_b64_again, mac_b64_again) = key_validation_mac(&raw, &iv).expect("key mac");
        assert_eq!(iv_b64, iv_b64_again);
        assert_eq!(mac_b64, mac_b64_again);

        let other = [0x99u8; 32];
        let (_, other_mac) = key_validation_mac(&other, &iv).expect("key mac");
        assert_ne!(mac_b64, other_mac, "validation MAC must be bound to the key");
    }
}
