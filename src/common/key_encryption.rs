use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use base64::Engine;

const NONCE_SIZE: usize = 12;
const KEY_DERIVATION_INFO: &[u8] = b"synapse-rust-signing-key-encryption-v1";

/// Encrypt a plaintext string using AES-256-GCM.
///
/// Returns a base64-encoded string of `nonce || ciphertext || tag`,
/// prefixed with `enc:` to indicate encryption.
pub fn encrypt_key(plaintext: &str, master_key: &[u8]) -> Result<String, String> {
    let key = derive_key(master_key, KEY_DERIVATION_INFO);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Invalid key length: {e}"))?;

    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes()).map_err(|e| format!("Encryption failed: {e}"))?;

    // Concatenate nonce + ciphertext (ciphertext already includes the tag)
    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);

    let encoded = base64::engine::general_purpose::STANDARD.encode(&combined);
    Ok(format!("enc:{encoded}"))
}

/// Decrypt a ciphertext string that was encrypted with `encrypt_key`.
///
/// Accepts both `enc:`-prefixed and raw base64 formats.
/// Returns the original plaintext string.
pub fn decrypt_key(ciphertext: &str, master_key: &[u8]) -> Result<String, String> {
    let b64_data =
        ciphertext.strip_prefix("enc:").ok_or_else(|| "Ciphertext must start with 'enc:' prefix".to_string())?;

    let combined =
        base64::engine::general_purpose::STANDARD.decode(b64_data).map_err(|e| format!("Base64 decode failed: {e}"))?;

    if combined.len() < NONCE_SIZE {
        return Err("Ciphertext too short".to_string());
    }

    let (nonce_bytes, encrypted_data) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = derive_key(master_key, KEY_DERIVATION_INFO);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Invalid key length: {e}"))?;

    let plaintext = cipher.decrypt(nonce, encrypted_data).map_err(|e| format!("Decryption failed: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8 in decrypted data: {e}"))
}

/// Check if a stored key value is encrypted (starts with `enc:` prefix).
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with("enc:")
}

/// Derive a 32-byte AES key from the master key using HKDF-SHA256 (RFC 5869).
///
/// NOTE: This is a breaking change from the previous non-standard derivation,
/// which used a single `SHA-256(info ‖ master_key)` hash. HKDF is a standard
/// KDF with an extract-then-expand structure. Existing encrypted data produced
/// under the old derivation will NOT be decryptable with this implementation.
#[allow(clippy::expect_used)]
fn derive_key(master_key: &[u8], info: &[u8]) -> [u8; 32] {
    // No salt: master_key is used directly as the input key material (IKM).
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(None, master_key);
    let mut okm = [0u8; 32];
    // expand only fails if output length > 255 * HashLen; 32 < 255 * 32, so this is safe.
    hk.expand(info, &mut okm).expect("32 bytes is valid HKDF output length");
    okm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let master_key = b"test-master-key-32-bytes-long!!";
        let plaintext = "my-secret-signing-key";

        let encrypted = encrypt_key(plaintext, master_key).unwrap();
        assert!(encrypted.starts_with("enc:"));
        assert!(is_encrypted(&encrypted));

        let decrypted = decrypt_key(&encrypted, master_key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let master_key = b"test-master-key-32-bytes-long!!";
        let plaintext = "same-plaintext";

        let enc1 = encrypt_key(plaintext, master_key).unwrap();
        let enc2 = encrypt_key(plaintext, master_key).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(enc1, enc2);

        // But both should decrypt to the same plaintext
        assert_eq!(decrypt_key(&enc1, master_key).unwrap(), plaintext);
        assert_eq!(decrypt_key(&enc2, master_key).unwrap(), plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let master_key = b"test-master-key-32-bytes-long!!";
        let wrong_key = b"wrong-master-key-32-bytes-lon!";
        let plaintext = "secret-data";

        let encrypted = encrypt_key(plaintext, master_key).unwrap();
        let result = decrypt_key(&encrypted, wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_without_prefix_fails() {
        let master_key = b"test-master-key-32-bytes-long!!";
        let result = decrypt_key("no-prefix-here", master_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_encrypted() {
        assert!(is_encrypted("enc:abc123"));
        assert!(!is_encrypted("plain-key"));
        assert!(!is_encrypted("enc"));
    }

    #[test]
    fn test_derive_key_deterministic() {
        let info = b"synapse-rust-signing-key-encryption-v1";
        let key1 = derive_key(b"master", info);
        let key2 = derive_key(b"master", info);
        assert_eq!(key1, key2);

        let key3 = derive_key(b"different", info);
        assert_ne!(key1, key3);
    }
}
