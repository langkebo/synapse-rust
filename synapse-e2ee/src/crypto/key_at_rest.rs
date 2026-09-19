//! Key-at-rest encryption for Megolm session keys.
//!
//! This module provides `KeyAtRest` — a thin wrapper that encrypts
//! 32-byte Megolm session keys before persisting them to Redis / Postgres.
//!
//! The serialized format is:
//!   `v1:<unpadded_base64(nonce ‖ ciphertext ‖ tag)>`
//!
//! where `nonce` is 12 bytes, `ciphertext` is the AES-256-GCM
//! encrypted key bytes, and `tag` is the 16-byte authentication tag
//! (all folded into the single `ciphertext` output of `Aes256GcmCipher`).

use crate::crypto::{Aes256GcmCipher, Aes256GcmKey};
use base64::Engine;
use rand::RngCore;
use std::path::PathBuf;
use synapse_common::ApiError;

/// The version prefix for the at-rest format.
const AT_REST_VERSION_PREFIX: &str = "v1:";

/// Manages encryption-key-at-rest: seal (encrypt+persist) and open (decrypt+load).
///
/// The `seal` method encrypts a plaintext key and returns a `v1:`-prefixed
/// base64 string suitable for writing to a config file or env var.
///
/// The `open` method reverses the process: strip the prefix, base64-decode,
/// and AES-256-GCM decrypt.
#[derive(Clone)]
pub struct KeyAtRest {
    cipher: Aes256GcmCipher,
    key: [u8; 32],
}

impl KeyAtRest {
    /// Create a new `KeyAtRest` from an existing 32-byte key.
    ///
    /// The key is used as the AES-256-GCM encryption key for at-rest
    /// protection. A fresh CSPRNG-derived nonce is generated per seal
    /// operation via the internal `Aes256GcmCipher`.
    pub fn new(key: [u8; 32]) -> Self {
        Self { cipher: Aes256GcmCipher::default(), key }
    }

    /// Encrypt `plaintext` and return a `v1:`-prefixed base64 string.
    ///
    /// Output format: `v1:<base64(nonce ‖ ciphertext ‖ tag)>`
    /// where `nonce` is 12 bytes and `ciphertext` includes the GCM tag.
    pub fn seal(&self, plaintext: &[u8]) -> Result<String, ApiError> {
        let cipher_key = Aes256GcmKey::from_bytes(self.key);
        let encrypted = self
            .cipher
            .encrypt_with_nonce(&cipher_key, plaintext)
            .map_err(|_| ApiError::encryption_error("Key-at-rest encryption failed"))?;
        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&encrypted);
        Ok(format!("{AT_REST_VERSION_PREFIX}{b64}"))
    }

    /// Reverse `seal`: strip `v1:`, base64-decode, and decrypt.
    ///
    /// Returns the decrypted plaintext bytes. Decryption failure is a hard error.
    pub fn open(&self, stored: &str) -> Result<Vec<u8>, ApiError> {
        let body = stored
            .strip_prefix(AT_REST_VERSION_PREFIX)
            .ok_or_else(|| ApiError::internal("session key missing at-rest version prefix"))?;
        let sealed = base64::engine::general_purpose::STANDARD
            .decode(body)
            .map_err(|_| ApiError::internal("session key base64 decode failed"))?;
        if sealed.len() < 12 {
            return Err(ApiError::internal("session key at-rest format invalid"));
        }
        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&sealed)
            .map_err(|_| ApiError::internal("session key at-rest nonce invalid"))?;
        let cipher_key = Aes256GcmKey::from_bytes(self.key);
        Aes256GcmCipher::decrypt(&cipher_key, &nonce, ciphertext)
            .map_err(|_| ApiError::decryption_error("session key decryption failed"))
    }

    /// Load an encryption key from the file at `path`.
    ///
    /// Reads the file, expects `v1:`-prefixed base64 content, decrypts
    /// and returns the 32-byte key.
    ///
    /// NOTE: For `server.megolm_encryption_key_path` configuration,
    /// the file should contain PLAINTEXT base64 (not v1: prefix).
    /// Use [`Self::load_plaintext`] for that case.
    pub fn load(path: &str) -> Result<[u8; 32], ApiError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ApiError::internal(format!("Failed to read key-at-rest file {}: {}", path, e)))?;
        let plaintext = Self::new([0u8; 32]).open(&content)?;
        if plaintext.len() != 32 {
            return Err(ApiError::internal("Key-at-rest decrypted to wrong length"));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&plaintext);
        Ok(key)
    }

    /// Load a plaintext base64-encoded 32-byte key from file (for megolm_encryption_key_path).
    pub fn load_plaintext(path: &str) -> Result<[u8; 32], ApiError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ApiError::internal(format!("Failed to read key file {}: {}", path, e)))?;
        let trimmed = content.trim();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(trimmed)
            .map_err(|e| ApiError::internal(format!("Key file is not valid base64: {}", e)))?;
        if decoded.len() != 32 {
            return Err(ApiError::internal(format!("Key file has wrong length ({} != 32)", decoded.len())));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded);
        Ok(key)
    }

    /// Generate a fresh 32-byte key, encrypt it, and persist it to the file at
    /// `path`.
    ///
    /// On Unix, the file is created with permissions `0600`; on other platforms
    /// file permissions are best-effort.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] if the directory cannot be created, a key cannot
    /// be securely generated, or the file cannot be written. The previous
    /// doc-string was inaccurate: this function returns `Result`, it does not
    /// panic.
    pub fn generate_and_persist(path: &str) -> Result<[u8; 32], ApiError> {
        let path_buf = PathBuf::from(path);
        if let Some(parent) = path_buf.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ApiError::internal(format!("Cannot create directory for key file {}: {}", path_buf.display(), e))
            })?;
        }

        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);

        let at_rest = Self::new(key_bytes);
        let sealed =
            at_rest.seal(&key_bytes).map_err(|e| ApiError::internal(format!("Failed to seal generated key: {}", e)))?;
        std::fs::write(&path_buf, sealed.as_bytes())
            .map_err(|e| ApiError::internal(format!("Failed to persist key file {}: {}", path_buf.display(), e)))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path_buf, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
                ApiError::internal(format!("Failed to set 0600 permissions on key file {}: {}", path_buf.display(), e))
            })?;
        }

        Ok(key_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_open_roundtrip() {
        let mut key = [0u8; 32];
        rand::rng().fill_bytes(&mut key);
        let at_rest = KeyAtRest::new(key);

        let plaintext = b"test-secret-key-data-32bytes-long!!";
        let sealed = at_rest.seal(plaintext).unwrap();
        assert!(sealed.starts_with("v1:"));

        let opened = at_rest.open(&sealed).unwrap();
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn test_open_rejects_tampered_data() {
        let mut key = [0u8; 32];
        rand::rng().fill_bytes(&mut key);
        let at_rest = KeyAtRest::new(key);

        let sealed = at_rest.seal(b"hello").unwrap();
        // Tamper with the base64 payload
        let tampered = format!("{}tampered", sealed);
        let result = at_rest.open(&tampered);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_rejects_wrong_prefix() {
        let key = [0u8; 32];
        let at_rest = KeyAtRest::new(key);

        let result = at_rest.open("v2:abc123");
        assert!(result.is_err());
    }

    #[test]
    fn test_open_rejects_corrupted_base64() {
        let key = [0u8; 32];
        let at_rest = KeyAtRest::new(key);

        let result = at_rest.open("v1:!!!not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_plaintext_works() {
        let dir = std::env::temp_dir().join("synapse_e2ee_plaintext_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.key");

        let mut key = [0u8; 32];
        rand::rng().fill_bytes(&mut key);
        let b64 = base64::engine::general_purpose::STANDARD.encode(key);
        std::fs::write(&path, b64.as_bytes()).unwrap();

        let loaded = KeyAtRest::load_plaintext(&path.to_string_lossy()).unwrap();
        assert_eq!(loaded, key);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_load_plaintext_rejects_v1_prefix() {
        let dir = std::env::temp_dir().join("synapse_e2ee_v1_reject_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.key");

        // Write a v1: prefixed content (at-rest format) to a plaintext loader
        let at_rest = KeyAtRest::new([0u8; 32]);
        let sealed = at_rest.seal(b"hello").unwrap();
        std::fs::write(&path, sealed.as_bytes()).unwrap();

        let result = KeyAtRest::load_plaintext(&path.to_string_lossy());
        assert!(result.is_err());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_generate_and_persist_roundtrip() {
        let dir = std::env::temp_dir().join("synapse_e2ee_key_at_rest_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.key");

        // Generate a fresh key and persist it as encrypted (v1: prefix format)
        let key = KeyAtRest::generate_and_persist(&path.to_string_lossy()).unwrap();
        assert_eq!(key.len(), 32);

        // Load back using the encrypted loader with the same encryption key
        let at_rest = KeyAtRest::new(key);
        let content = std::fs::read_to_string(&path).unwrap();
        let opened = at_rest.open(&content).unwrap();

        // The decrypted content should be the original key bytes
        assert_eq!(opened.as_slice(), &key[..]);

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
