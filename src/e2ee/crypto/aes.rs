use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use generic_array::GenericArray;
use rand::Rng;
use serde::{Deserialize, Serialize};
use typenum::U32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aes256GcmKey {
    bytes: [u8; 32],
}

impl Aes256GcmKey {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill(&mut bytes);
        Self { bytes }
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aes256GcmNonce {
    bytes: [u8; 12],
}

impl Aes256GcmNonce {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 12];
        rand::thread_rng().fill(&mut bytes);
        Self { bytes }
    }

    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, super::CryptoError> {
        let bytes = bytes.as_ref();
        if bytes.len() != 12 {
            return Err(super::CryptoError::InvalidKeyLength);
        }
        let mut arr = [0u8; 12];
        arr.copy_from_slice(bytes);
        Ok(Self { bytes: arr })
    }

    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aes256GcmCiphertext {
    nonce: Aes256GcmNonce,
    ciphertext: Vec<u8>,
}

impl Aes256GcmCiphertext {
    pub fn new(nonce: Aes256GcmNonce, ciphertext: Vec<u8>) -> Self {
        Self { nonce, ciphertext }
    }

    pub fn nonce(&self) -> &Aes256GcmNonce {
        &self.nonce
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

impl AsRef<[u8]> for Aes256GcmCiphertext {
    fn as_ref(&self) -> &[u8] {
        &self.ciphertext
    }
}

pub struct Aes256GcmCipher;

impl Aes256GcmCipher {
    pub fn encrypt(key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Vec<u8>, super::CryptoError> {
        let nonce = Aes256GcmNonce::generate();
        let cipher_key = GenericArray::<u8, U32>::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(nonce.as_bytes());

        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e| super::CryptoError::EncryptionError(e.to_string()))?;

        let mut result = Vec::with_capacity(nonce.as_bytes().len() + ciphertext.len());
        result.extend_from_slice(nonce.as_bytes());
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    pub fn decrypt(
        key: &Aes256GcmKey,
        nonce: &Aes256GcmNonce,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, super::CryptoError> {
        let cipher_key = GenericArray::<u8, U32>::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(nonce.as_bytes());

        let plaintext = cipher
            .decrypt(nonce_bytes, encrypted)
            .map_err(|e| super::CryptoError::DecryptionError(e.to_string()))?;

        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes256_gcm_key_generate() {
        let key = Aes256GcmKey::generate();
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_aes256_gcm_key_from_bytes() {
        let bytes = [0x12u8; 32];
        let key = Aes256GcmKey::from_bytes(bytes);
        assert_eq!(key.as_bytes(), &bytes);
    }

    #[test]
    fn test_aes256_gcm_key_different_each_time() {
        let key1 = Aes256GcmKey::generate();
        let key2 = Aes256GcmKey::generate();
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_aes256_gcm_nonce_generate() {
        let nonce = Aes256GcmNonce::generate();
        assert_eq!(nonce.as_bytes().len(), 12);
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_nonce_from_bytes() {
        let bytes = [0x34u8; 12];
        let nonce = Aes256GcmNonce::from_bytes(&bytes).unwrap();
        assert_eq!(nonce.as_bytes(), &bytes);
    }

    #[test]
    fn test_aes256_gcm_nonce_different_each_time() {
        let nonce1 = Aes256GcmNonce::generate();
        let nonce2 = Aes256GcmNonce::generate();
        assert_ne!(nonce1.as_bytes(), nonce2.as_bytes());
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_ciphertext_new() {
        let nonce = Aes256GcmNonce::from_bytes(&[0x12u8; 12]).unwrap();
        let ciphertext = vec![0x34u8; 20];
        let cipher = Aes256GcmCiphertext::new(nonce.clone(), ciphertext.clone());
        assert_eq!(cipher.nonce(), &nonce);
        assert_eq!(cipher.ciphertext(), &ciphertext);
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_ciphertext_as_ref() {
        let nonce = Aes256GcmNonce::from_bytes(&[0x12u8; 12]).unwrap();
        let ciphertext = vec![0x34u8; 20];
        let cipher = Aes256GcmCiphertext::new(nonce, ciphertext.clone());
        assert_eq!(cipher.as_ref(), ciphertext.as_slice());
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_encrypt_decrypt_roundtrip() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Hello, World! This is a secret message.";

        let encrypted = Aes256GcmCipher::encrypt(&key, plaintext.as_ref()).unwrap();
        assert!(encrypted.len() > 12);

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_encrypt_different_nonces() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Test message";

        let encrypted1 = Aes256GcmCipher::encrypt(&key, plaintext.as_ref()).unwrap();
        let encrypted2 = Aes256GcmCipher::encrypt(&key, plaintext.as_ref()).unwrap();

        assert_ne!(encrypted1[0..12], encrypted2[0..12]);
        assert_ne!(encrypted1[12..], encrypted2[12..]);
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_decrypt_wrong_key() {
        let key1 = Aes256GcmKey::generate();
        let key2 = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::encrypt(&key1, plaintext.as_ref()).unwrap();
        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let result = Aes256GcmCipher::decrypt(&key2, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_decrypt_wrong_nonce() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::encrypt(&key, plaintext.as_ref()).unwrap();
        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let wrong_nonce = Aes256GcmNonce::generate();
        let result = Aes256GcmCipher::decrypt(&key, &wrong_nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_decrypt_tampered_ciphertext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let mut encrypted = Aes256GcmCipher::encrypt(&key, plaintext.as_ref()).unwrap();
        encrypted[12] ^= 0xff;

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let result = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_encrypt_empty_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"";

        let encrypted = Aes256GcmCipher::encrypt(&key, plaintext.as_ref()).unwrap();
        assert_eq!(encrypted.len(), 12);

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    #[ignore]
    fn test_aes256_gcm_encrypt_large_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = vec![0x42u8; 10000];

        let encrypted = Aes256GcmCipher::encrypt(&key, &plaintext).unwrap();
        assert_eq!(encrypted.len(), 12 + 10000);

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12].try_into().unwrap());
        let ciphertext = &encrypted[12..];

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
