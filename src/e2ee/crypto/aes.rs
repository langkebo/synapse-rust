use crate::e2ee::crypto::CryptoError;
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use base64::Engine;
#[cfg(test)]
use dashmap::DashSet;
use generic_array::GenericArray;
use rand::Rng;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::sync::Arc;
use typenum::U32;

#[cfg(test)]
const NONCE_HISTORY_SIZE: usize = 10000;
#[cfg(test)]
const NONCE_COUNTER_MAX: u64 = (1u64 << 32) - 1;

#[derive(Debug, Clone)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Aes256GcmNonce {
    bytes: [u8; 12],
}

impl Serialize for Aes256GcmNonce {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(self.bytes))
    }
}

impl<'de> Deserialize<'de> for Aes256GcmNonce {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(&s).map_err(serde::de::Error::custom)?;
        Self::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }
}

impl Aes256GcmNonce {
    fn generate() -> Self {
        let mut bytes = [0u8; 12];
        rand::thread_rng().fill(&mut bytes);
        Self { bytes }
    }

    fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, CryptoError> {
        let bytes = bytes.as_ref();
        if bytes.len() != 12 {
            return Err(CryptoError::InvalidNonceLength);
        }
        let mut arr = [0u8; 12];
        arr.copy_from_slice(bytes);
        Ok(Self { bytes: arr })
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct NonceTracker {
    used_nonces: DashSet<Vec<u8>>,
    counter: AtomicU64,
    max_history_size: usize,
}

#[cfg(test)]
impl NonceTracker {
    pub fn new() -> Self {
        Self { used_nonces: DashSet::new(), counter: AtomicU64::new(0), max_history_size: NONCE_HISTORY_SIZE }
    }

    pub fn with_history_size(max_history_size: usize) -> Self {
        Self { used_nonces: DashSet::new(), counter: AtomicU64::new(0), max_history_size }
    }

    pub fn check_and_record(&self, nonce: &[u8]) -> Result<(), CryptoError> {
        if !self.used_nonces.insert(nonce.to_vec()) {
            return Err(CryptoError::NonceReuseDetected);
        }

        if self.used_nonces.len() >= self.max_history_size {
            self.prune_old_nonces();
        }

        self.counter.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    fn prune_old_nonces(&self) {
        let current_size = self.used_nonces.len();
        if current_size > self.max_history_size / 2 {
            let to_remove = current_size - self.max_history_size / 2;
            let mut removed = 0;
            let keys_to_remove: Vec<Vec<u8>> = 
                self.used_nonces.iter().take(to_remove).map(|entry| entry.clone()).collect();
            for key in keys_to_remove {
                self.used_nonces.remove(&key);
                removed += 1;
                if removed >= to_remove {
                    break;
                }
            }
        }
    }

    pub fn counter(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }

    pub fn is_nonce_used(&self, nonce: &[u8]) -> bool {
        self.used_nonces.contains(nonce)
    }

    pub fn clear(&self) {
        self.used_nonces.clear();
        self.counter.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl Default for NonceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct SecureNonceGenerator {
    counter: AtomicU64,
    tracker: Arc<NonceTracker>,
}

#[cfg(test)]
impl SecureNonceGenerator {
    pub fn new(tracker: Arc<NonceTracker>) -> Self {
        Self { counter: AtomicU64::new(0), tracker }
    }

    pub fn generate_aes_gcm_nonce(&self) -> Result<Aes256GcmNonce, CryptoError> {
        let counter = self.counter.fetch_add(1, Ordering::SeqCst);
        if counter >= NONCE_COUNTER_MAX {
            return Err(CryptoError::NonceCounterOverflow);
        }

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill(&mut nonce_bytes[0..4]);

        nonce_bytes[4..12].copy_from_slice(&counter.to_be_bytes());

        self.tracker.check_and_record(&nonce_bytes)?;

        Ok(Aes256GcmNonce { bytes: nonce_bytes })
    }

    pub fn counter(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }

    pub fn reset_counter(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }
}

#[derive(Debug, Default)]
pub struct Aes256GcmCipher {
    #[cfg(test)]
    nonce_generator: Option<Arc<SecureNonceGenerator>>,
}

impl Aes256GcmCipher {
    #[cfg(test)]
    pub fn with_nonce_tracker(tracker: Arc<NonceTracker>) -> Self {
        let nonce_generator = Arc::new(SecureNonceGenerator::new(tracker));
        Self { nonce_generator: Some(nonce_generator) }
    }

    #[cfg(test)]
    fn split_encrypted_data(encrypted: &[u8]) -> Result<(Aes256GcmNonce, &[u8]), CryptoError> {
        if encrypted.len() < 12 {
            return Err(CryptoError::InvalidNonceLength);
        }
        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12])?;
        let ciphertext = &encrypted[12..];
        Ok((nonce, ciphertext))
    }

    #[allow(dead_code)]
    fn encrypt(&self, key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = {
            #[cfg(test)]
            {
                if let Some(ref gen) = self.nonce_generator {
                    gen.generate_aes_gcm_nonce()?
                } else {
                    Aes256GcmNonce::generate()
                }
            }
            #[cfg(not(test))]
            {
                Aes256GcmNonce::generate()
            }
        };

        let cipher_key = GenericArray::<u8, U32>::from_slice(&key.bytes);
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(&nonce.bytes);

        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e: aes_gcm::aead::Error| CryptoError::EncryptionError(e.to_string()))?;

        let mut result = Vec::with_capacity(nonce.bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce.bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    pub fn encrypt_with_nonce(key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = Aes256GcmNonce::generate();
        let cipher_key = GenericArray::<u8, U32>::from_slice(&key.bytes);
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(&nonce.bytes);

        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e: aes_gcm::aead::Error| CryptoError::EncryptionError(e.to_string()))?;

        let mut result = Vec::with_capacity(nonce.bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce.bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    #[allow(dead_code)]
    fn decrypt(
        key: &Aes256GcmKey,
        nonce: &Aes256GcmNonce,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let cipher_key = GenericArray::<u8, U32>::from_slice(&key.bytes);
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(&nonce.bytes);

        let plaintext =
            cipher.decrypt(nonce_bytes, encrypted).map_err(|e| CryptoError::DecryptionError(e.to_string()))?;

        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes256_gcm_key_generate() {
        let key = Aes256GcmKey::generate();
        assert_eq!(key.bytes.len(), 32);
    }

    #[test]
    fn test_aes256_gcm_key_from_bytes() {
        let bytes = [0x12u8; 32];
        let key = Aes256GcmKey::from_bytes(bytes);
        assert_eq!(&key.bytes, &bytes);
    }

    #[test]
    fn test_aes256_gcm_key_different_each_time() {
        let key1 = Aes256GcmKey::generate();
        let key2 = Aes256GcmKey::generate();
        assert_ne!(&key1.bytes, &key2.bytes);
    }

    #[test]
    fn test_aes256_gcm_nonce_generate() {
        let nonce = Aes256GcmNonce::generate();
        assert_eq!(nonce.bytes.len(), 12);
    }

    #[test]
    fn test_aes256_gcm_nonce_from_bytes() {
        let bytes = [0x34u8; 12];
        let nonce = Aes256GcmNonce::from_bytes(bytes).unwrap();
        assert_eq!(&nonce.bytes, &bytes);
    }

    #[test]
    fn test_aes256_gcm_nonce_different_each_time() {
        let nonce1 = Aes256GcmNonce::generate();
        let nonce2 = Aes256GcmNonce::generate();
        assert_ne!(&nonce1.bytes, &nonce2.bytes);
    }

    #[test]
    fn test_aes256_gcm_encrypt_decrypt_roundtrip() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Hello, World! This is a secret message.";

        let encrypted = Aes256GcmCipher::default().encrypt(&key, plaintext.as_ref()).unwrap();
        assert!(encrypted.len() > 12);

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes256_gcm_encrypt_different_nonces() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Test message";

        let encrypted1 = Aes256GcmCipher::default().encrypt(&key, plaintext.as_ref()).unwrap();
        let encrypted2 = Aes256GcmCipher::default().encrypt(&key, plaintext.as_ref()).unwrap();

        assert_ne!(encrypted1[0..12], encrypted2[0..12]);
        assert_ne!(encrypted1[12..], encrypted2[12..]);
    }

    #[test]
    fn test_aes256_gcm_decrypt_wrong_key() {
        let key1 = Aes256GcmKey::generate();
        let key2 = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::default().encrypt(&key1, plaintext.as_ref()).unwrap();
        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let result = Aes256GcmCipher::decrypt(&key2, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_decrypt_wrong_nonce() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::default().encrypt(&key, plaintext.as_ref()).unwrap();
        let ciphertext = &encrypted[12..];

        let wrong_nonce = Aes256GcmNonce::generate();
        let result = Aes256GcmCipher::decrypt(&key, &wrong_nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_decrypt_tampered_ciphertext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let mut encrypted = Aes256GcmCipher::default().encrypt(&key, plaintext.as_ref()).unwrap();
        encrypted[12] ^= 0xff;

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let result = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_encrypt_empty_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"";

        let encrypted = Aes256GcmCipher::default().encrypt(&key, plaintext.as_ref()).unwrap();
        assert_eq!(encrypted.len(), 28);

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_aes256_gcm_encrypt_large_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = vec![0x42u8; 10000];

        let encrypted = Aes256GcmCipher::default().encrypt(&key, &plaintext).unwrap();
        assert_eq!(encrypted.len(), 10028);

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_nonce_tracker_detects_reuse() {
        let tracker = NonceTracker::new();
        let nonce = [1u8; 12];

        assert!(tracker.check_and_record(&nonce).is_ok());
        assert!(tracker.check_and_record(&nonce).is_err());
        assert_eq!(tracker.check_and_record(&nonce).unwrap_err(), CryptoError::NonceReuseDetected);
    }

    #[test]
    fn test_nonce_tracker_counter_increments() {
        let tracker = NonceTracker::new();
        assert_eq!(tracker.counter(), 0);

        tracker.check_and_record(&[1u8; 12]).unwrap();
        assert_eq!(tracker.counter(), 1);

        tracker.check_and_record(&[2u8; 12]).unwrap();
        assert_eq!(tracker.counter(), 2);
    }

    #[test]
    fn test_nonce_tracker_is_nonce_used() {
        let tracker = NonceTracker::new();
        let nonce = [1u8; 12];

        assert!(!tracker.is_nonce_used(&nonce));
        tracker.check_and_record(&nonce).unwrap();
        assert!(tracker.is_nonce_used(&nonce));
    }

    #[test]
    fn test_nonce_tracker_clear() {
        let tracker = NonceTracker::new();
        tracker.check_and_record(&[1u8; 12]).unwrap();
        tracker.check_and_record(&[2u8; 12]).unwrap();
        assert_eq!(tracker.counter(), 2);

        tracker.clear();
        assert_eq!(tracker.counter(), 0);
        assert!(!tracker.is_nonce_used(&[1u8; 12]));
    }

    #[test]
    fn test_secure_nonce_generator_aes_gcm() {
        let tracker = Arc::new(NonceTracker::new());
        let generator = SecureNonceGenerator::new(Arc::clone(&tracker));

        let nonce1 = generator.generate_aes_gcm_nonce().unwrap();
        let nonce2 = generator.generate_aes_gcm_nonce().unwrap();

        assert_ne!(&nonce1.bytes, &nonce2.bytes);
        assert_eq!(generator.counter(), 2);
    }

    #[test]
    fn test_secure_nonce_generator_counter_in_nonce() {
        let tracker = Arc::new(NonceTracker::new());
        let generator = SecureNonceGenerator::new(Arc::clone(&tracker));

        let nonce1 = generator.generate_aes_gcm_nonce().unwrap();
        let nonce2 = generator.generate_aes_gcm_nonce().unwrap();

        let counter1 = u64::from_be_bytes(nonce1.bytes[4..12].try_into().unwrap());
        let counter2 = u64::from_be_bytes(nonce2.bytes[4..12].try_into().unwrap());

        assert_eq!(counter1, 0);
        assert_eq!(counter2, 1);
    }

    #[test]
    fn test_nonce_tracker_pruning() {
        let tracker = NonceTracker::with_history_size(100);

        for i in 0..150u8 {
            let nonce = [i; 12];
            tracker.check_and_record(&nonce).unwrap();
        }

        assert!(tracker.used_nonces.len() <= 100);
    }

    #[test]
    fn test_secure_nonce_generator_prevents_reuse() {
        let tracker = Arc::new(NonceTracker::new());
        let generator = SecureNonceGenerator::new(Arc::clone(&tracker));

        let nonce = generator.generate_aes_gcm_nonce().unwrap();
        assert!(tracker.is_nonce_used(&nonce.bytes));

        let result = tracker.check_and_record(&nonce.bytes);
        assert_eq!(result.unwrap_err(), CryptoError::NonceReuseDetected);
    }

    #[test]
    fn test_aes_gcm_nonce_invalid_length() {
        let short_bytes = [0u8; 8];
        let result = Aes256GcmNonce::from_bytes(short_bytes);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidNonceLength);
    }
}
