use crate::e2ee::crypto::CryptoError;
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use dashmap::DashSet;
use generic_array::GenericArray;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use typenum::U32;

const NONCE_HISTORY_SIZE: usize = 10000;
const NONCE_COUNTER_MAX: u64 = (1u64 << 32) - 1;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Aes256GcmNonce {
    bytes: [u8; 12],
}

impl Aes256GcmNonce {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 12];
        rand::thread_rng().fill(&mut bytes);
        Self { bytes }
    }

    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, CryptoError> {
        let bytes = bytes.as_ref();
        if bytes.len() != 12 {
            return Err(CryptoError::InvalidNonceLength);
        }
        let mut arr = [0u8; 12];
        arr.copy_from_slice(bytes);
        Ok(Self { bytes: arr })
    }

    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct XChaCha20Poly1305Nonce {
    bytes: [u8; 24],
}

impl XChaCha20Poly1305Nonce {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 24];
        rand::thread_rng().fill(&mut bytes);
        Self { bytes }
    }

    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, CryptoError> {
        let bytes = bytes.as_ref();
        if bytes.len() != 24 {
            return Err(CryptoError::InvalidNonceLength);
        }
        let mut arr = [0u8; 24];
        arr.copy_from_slice(bytes);
        Ok(Self { bytes: arr })
    }

    pub fn as_bytes(&self) -> &[u8; 24] {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XChaCha20Poly1305Ciphertext {
    nonce: XChaCha20Poly1305Nonce,
    ciphertext: Vec<u8>,
}

impl XChaCha20Poly1305Ciphertext {
    pub fn new(nonce: XChaCha20Poly1305Nonce, ciphertext: Vec<u8>) -> Self {
        Self { nonce, ciphertext }
    }

    pub fn nonce(&self) -> &XChaCha20Poly1305Nonce {
        &self.nonce
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

impl AsRef<[u8]> for XChaCha20Poly1305Ciphertext {
    fn as_ref(&self) -> &[u8] {
        &self.ciphertext
    }
}

#[derive(Debug)]
pub struct NonceTracker {
    used_nonces: DashSet<Vec<u8>>,
    counter: AtomicU64,
    max_history_size: usize,
}

impl NonceTracker {
    pub fn new() -> Self {
        Self {
            used_nonces: DashSet::new(),
            counter: AtomicU64::new(0),
            max_history_size: NONCE_HISTORY_SIZE,
        }
    }

    pub fn with_history_size(max_history_size: usize) -> Self {
        Self {
            used_nonces: DashSet::new(),
            counter: AtomicU64::new(0),
            max_history_size,
        }
    }

    pub fn check_and_record(&self, nonce: &[u8]) -> Result<(), CryptoError> {
        if self.used_nonces.contains(nonce) {
            return Err(CryptoError::NonceReuseDetected);
        }

        if self.used_nonces.len() >= self.max_history_size {
            self.prune_old_nonces();
        }

        self.used_nonces.insert(nonce.to_vec());
        self.counter.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    fn prune_old_nonces(&self) {
        let current_size = self.used_nonces.len();
        if current_size > self.max_history_size / 2 {
            let to_remove = current_size - self.max_history_size / 2;
            let mut removed = 0;
            let keys_to_remove: Vec<Vec<u8>> = self
                .used_nonces
                .iter()
                .take(to_remove)
                .map(|entry| entry.clone())
                .collect();
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

impl Default for NonceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct SecureNonceGenerator {
    counter: AtomicU64,
    tracker: Arc<NonceTracker>,
}

impl SecureNonceGenerator {
    pub fn new(tracker: Arc<NonceTracker>) -> Self {
        Self {
            counter: AtomicU64::new(0),
            tracker,
        }
    }

    pub fn generate_aes_gcm_nonce(&self) -> Result<Aes256GcmNonce, CryptoError> {
        let counter = self.counter.fetch_add(1, Ordering::SeqCst);
        if counter > NONCE_COUNTER_MAX {
            return Err(CryptoError::NonceCounterOverflow);
        }

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill(&mut nonce_bytes[0..4]);

        nonce_bytes[4..12].copy_from_slice(&counter.to_be_bytes());

        self.tracker.check_and_record(&nonce_bytes)?;

        Ok(Aes256GcmNonce { bytes: nonce_bytes })
    }

    pub fn generate_xchacha_nonce(&self) -> Result<XChaCha20Poly1305Nonce, CryptoError> {
        let counter = self.counter.fetch_add(1, Ordering::SeqCst);
        if counter > NONCE_COUNTER_MAX {
            return Err(CryptoError::NonceCounterOverflow);
        }

        let mut nonce_bytes = [0u8; 24];
        rand::thread_rng().fill(&mut nonce_bytes[0..16]);

        nonce_bytes[16..24].copy_from_slice(&counter.to_be_bytes());

        self.tracker.check_and_record(&nonce_bytes)?;

        Ok(XChaCha20Poly1305Nonce { bytes: nonce_bytes })
    }

    pub fn counter(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }

    pub fn reset_counter(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }
}

#[derive(Debug)]
pub struct Aes256GcmCipher {
    nonce_generator: Option<Arc<SecureNonceGenerator>>,
}

impl Aes256GcmCipher {
    pub fn new() -> Self {
        Self {
            nonce_generator: None,
        }
    }

    pub fn with_nonce_tracker(tracker: Arc<NonceTracker>) -> Self {
        let nonce_generator = Arc::new(SecureNonceGenerator::new(tracker));
        Self {
            nonce_generator: Some(nonce_generator),
        }
    }

    pub fn encrypt(&self, key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = if let Some(ref gen) = self.nonce_generator {
            gen.generate_aes_gcm_nonce()?
        } else {
            Aes256GcmNonce::generate()
        };

        let cipher_key = GenericArray::<u8, U32>::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(nonce.as_bytes());

        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e: aes_gcm::aead::Error| CryptoError::EncryptionError(e.to_string()))?;

        let mut result = Vec::with_capacity(nonce.as_bytes().len() + ciphertext.len());
        result.extend_from_slice(nonce.as_bytes());
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    pub fn encrypt_with_nonce(
        key: &Aes256GcmKey,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let nonce = Aes256GcmNonce::generate();
        let cipher_key = GenericArray::<u8, U32>::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(nonce.as_bytes());

        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e: aes_gcm::aead::Error| CryptoError::EncryptionError(e.to_string()))?;

        let mut result = Vec::with_capacity(nonce.as_bytes().len() + ciphertext.len());
        result.extend_from_slice(nonce.as_bytes());
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    pub fn decrypt(
        key: &Aes256GcmKey,
        nonce: &Aes256GcmNonce,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let cipher_key = GenericArray::<u8, U32>::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(nonce.as_bytes());

        let plaintext = cipher
            .decrypt(nonce_bytes, encrypted)
            .map_err(|e| CryptoError::DecryptionError(e.to_string()))?;

        Ok(plaintext)
    }
}

impl Default for Aes256GcmCipher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct XChaCha20Poly1305Cipher {
    nonce_generator: Option<Arc<SecureNonceGenerator>>,
}

impl XChaCha20Poly1305Cipher {
    pub fn new() -> Self {
        Self {
            nonce_generator: None,
        }
    }

    pub fn with_nonce_tracker(tracker: Arc<NonceTracker>) -> Self {
        let nonce_generator = Arc::new(SecureNonceGenerator::new(tracker));
        Self {
            nonce_generator: Some(nonce_generator),
        }
    }

    pub fn encrypt(&self, key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = if let Some(ref gen) = self.nonce_generator {
            gen.generate_xchacha_nonce()?
        } else {
            XChaCha20Poly1305Nonce::generate()
        };

        let cipher = XChaCha20Poly1305::new(key.into());
        let nonce_bytes = XNonce::from_slice(nonce.as_bytes());

        let ciphertext = cipher.encrypt(nonce_bytes, plaintext).map_err(
            |e: chacha20poly1305::aead::Error| CryptoError::EncryptionError(e.to_string()),
        )?;

        let mut result = Vec::with_capacity(nonce.as_bytes().len() + ciphertext.len());
        result.extend_from_slice(nonce.as_bytes());
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    pub fn decrypt(
        key: &[u8; 32],
        nonce: &XChaCha20Poly1305Nonce,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let cipher = XChaCha20Poly1305::new(key.into());
        let nonce_bytes = XNonce::from_slice(nonce.as_bytes());

        let plaintext = cipher
            .decrypt(nonce_bytes, encrypted)
            .map_err(|e| CryptoError::DecryptionError(e.to_string()))?;

        Ok(plaintext)
    }
}

impl Default for XChaCha20Poly1305Cipher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct E2eeCryptoProvider {
    aes_cipher: Aes256GcmCipher,
    xchacha_cipher: XChaCha20Poly1305Cipher,
    nonce_tracker: Arc<NonceTracker>,
}

impl E2eeCryptoProvider {
    pub fn new() -> Self {
        let nonce_tracker = Arc::new(NonceTracker::new());
        let aes_cipher = Aes256GcmCipher::with_nonce_tracker(Arc::clone(&nonce_tracker));
        let xchacha_cipher =
            XChaCha20Poly1305Cipher::with_nonce_tracker(Arc::clone(&nonce_tracker));

        Self {
            aes_cipher,
            xchacha_cipher,
            nonce_tracker,
        }
    }

    pub fn with_history_size(max_history_size: usize) -> Self {
        let nonce_tracker = Arc::new(NonceTracker::with_history_size(max_history_size));
        let aes_cipher = Aes256GcmCipher::with_nonce_tracker(Arc::clone(&nonce_tracker));
        let xchacha_cipher =
            XChaCha20Poly1305Cipher::with_nonce_tracker(Arc::clone(&nonce_tracker));

        Self {
            aes_cipher,
            xchacha_cipher,
            nonce_tracker,
        }
    }

    pub fn encrypt_aes(
        &self,
        key: &Aes256GcmKey,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        self.aes_cipher.encrypt(key, plaintext)
    }

    pub fn decrypt_aes(
        &self,
        key: &Aes256GcmKey,
        nonce: &Aes256GcmNonce,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        Aes256GcmCipher::decrypt(key, nonce, encrypted)
    }

    pub fn encrypt_xchacha(
        &self,
        key: &[u8; 32],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        self.xchacha_cipher.encrypt(key, plaintext)
    }

    pub fn decrypt_xchacha(
        key: &[u8; 32],
        nonce: &XChaCha20Poly1305Nonce,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        XChaCha20Poly1305Cipher::decrypt(key, nonce, encrypted)
    }

    pub fn nonce_tracker(&self) -> &NonceTracker {
        &self.nonce_tracker
    }

    pub fn nonce_counter(&self) -> u64 {
        self.nonce_tracker.counter()
    }

    pub fn is_nonce_used(&self, nonce: &[u8]) -> bool {
        self.nonce_tracker.is_nonce_used(nonce)
    }

    pub fn clear_nonce_history(&self) {
        self.nonce_tracker.clear();
    }
}

impl Default for E2eeCryptoProvider {
    fn default() -> Self {
        Self::new()
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
    fn test_aes256_gcm_nonce_from_bytes() {
        let bytes = [0x34u8; 12];
        let nonce = Aes256GcmNonce::from_bytes(bytes).unwrap();
        assert_eq!(nonce.as_bytes(), &bytes);
    }

    #[test]
    fn test_aes256_gcm_nonce_different_each_time() {
        let nonce1 = Aes256GcmNonce::generate();
        let nonce2 = Aes256GcmNonce::generate();
        assert_ne!(nonce1.as_bytes(), nonce2.as_bytes());
    }

    #[test]
    fn test_aes256_gcm_ciphertext_new() {
        let nonce = Aes256GcmNonce::from_bytes([0x12u8; 12]).unwrap();
        let ciphertext = vec![0x34u8; 20];
        let cipher = Aes256GcmCiphertext::new(nonce.clone(), ciphertext.clone());
        assert_eq!(cipher.nonce(), &nonce);
        assert_eq!(cipher.ciphertext(), &ciphertext);
    }

    #[test]
    fn test_aes256_gcm_ciphertext_as_ref() {
        let nonce = Aes256GcmNonce::from_bytes([0x12u8; 12]).unwrap();
        let ciphertext = vec![0x34u8; 20];
        let cipher = Aes256GcmCiphertext::new(nonce, ciphertext.clone());
        assert_eq!(cipher.as_ref(), &ciphertext);
    }

    #[test]
    fn test_aes256_gcm_encrypt_decrypt_roundtrip() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Hello, World! This is a secret message.";

        let encrypted = Aes256GcmCipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();
        assert!(encrypted.len() > 12);

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes256_gcm_encrypt_different_nonces() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Test message";

        let encrypted1 = Aes256GcmCipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();
        let encrypted2 = Aes256GcmCipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();

        assert_ne!(encrypted1[0..12], encrypted2[0..12]);
        assert_ne!(encrypted1[12..], encrypted2[12..]);
    }

    #[test]
    fn test_aes256_gcm_decrypt_wrong_key() {
        let key1 = Aes256GcmKey::generate();
        let key2 = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::new()
            .encrypt(&key1, plaintext.as_ref())
            .unwrap();
        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let result = Aes256GcmCipher::decrypt(&key2, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_decrypt_wrong_nonce() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();
        let ciphertext = &encrypted[12..];

        let wrong_nonce = Aes256GcmNonce::generate();
        let result = Aes256GcmCipher::decrypt(&key, &wrong_nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_decrypt_tampered_ciphertext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let mut encrypted = Aes256GcmCipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();
        encrypted[12] ^= 0xff;

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let result = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_encrypt_empty_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"";

        let encrypted = Aes256GcmCipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();
        assert_eq!(encrypted.len(), 28);

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_aes256_gcm_encrypt_large_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = vec![0x42u8; 10000];

        let encrypted = Aes256GcmCipher::new().encrypt(&key, &plaintext).unwrap();
        assert_eq!(encrypted.len(), 10028);

        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_xchacha20_poly1305_nonce_generate() {
        let nonce = XChaCha20Poly1305Nonce::generate();
        assert_eq!(nonce.as_bytes().len(), 24);
    }

    #[test]
    fn test_xchacha20_poly1305_nonce_from_bytes() {
        let bytes = [0x56u8; 24];
        let nonce = XChaCha20Poly1305Nonce::from_bytes(bytes).unwrap();
        assert_eq!(nonce.as_bytes(), &bytes);
    }

    #[test]
    fn test_xchacha20_poly1305_nonce_different_each_time() {
        let nonce1 = XChaCha20Poly1305Nonce::generate();
        let nonce2 = XChaCha20Poly1305Nonce::generate();
        assert_ne!(nonce1.as_bytes(), nonce2.as_bytes());
    }

    #[test]
    fn test_xchacha20_poly1305_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let plaintext = b"Hello, XChaCha20-Poly1305!";

        let encrypted = XChaCha20Poly1305Cipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();
        assert!(encrypted.len() > 24);

        let nonce = XChaCha20Poly1305Nonce::from_bytes(&encrypted[0..24]).unwrap();
        let ciphertext = &encrypted[24..];

        let decrypted = XChaCha20Poly1305Cipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_xchacha20_poly1305_encrypt_different_nonces() {
        let key = [0x42u8; 32];
        let plaintext = b"Test message";

        let encrypted1 = XChaCha20Poly1305Cipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();
        let encrypted2 = XChaCha20Poly1305Cipher::new()
            .encrypt(&key, plaintext.as_ref())
            .unwrap();

        assert_ne!(encrypted1[0..24], encrypted2[0..24]);
        assert_ne!(encrypted1[24..], encrypted2[24..]);
    }

    #[test]
    fn test_nonce_tracker_detects_reuse() {
        let tracker = NonceTracker::new();
        let nonce = [1u8; 12];

        assert!(tracker.check_and_record(&nonce).is_ok());
        assert!(tracker.check_and_record(&nonce).is_err());
        assert_eq!(
            tracker.check_and_record(&nonce).unwrap_err(),
            CryptoError::NonceReuseDetected
        );
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

        assert_ne!(nonce1.as_bytes(), nonce2.as_bytes());
        assert_eq!(generator.counter(), 2);
    }

    #[test]
    fn test_secure_nonce_generator_xchacha() {
        let tracker = Arc::new(NonceTracker::new());
        let generator = SecureNonceGenerator::new(Arc::clone(&tracker));

        let nonce1 = generator.generate_xchacha_nonce().unwrap();
        let nonce2 = generator.generate_xchacha_nonce().unwrap();

        assert_ne!(nonce1.as_bytes(), nonce2.as_bytes());
        assert_eq!(generator.counter(), 2);
    }

    #[test]
    fn test_secure_nonce_generator_counter_in_nonce() {
        let tracker = Arc::new(NonceTracker::new());
        let generator = SecureNonceGenerator::new(Arc::clone(&tracker));

        let nonce1 = generator.generate_aes_gcm_nonce().unwrap();
        let nonce2 = generator.generate_aes_gcm_nonce().unwrap();

        let counter1 = u64::from_be_bytes(nonce1.as_bytes()[4..12].try_into().unwrap());
        let counter2 = u64::from_be_bytes(nonce2.as_bytes()[4..12].try_into().unwrap());

        assert_eq!(counter1, 0);
        assert_eq!(counter2, 1);
    }

    #[test]
    fn test_e2ee_crypto_provider_aes() {
        let provider = E2eeCryptoProvider::new();
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret message";

        let encrypted = provider.encrypt_aes(&key, plaintext.as_ref()).unwrap();
        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12]).unwrap();
        let ciphertext = &encrypted[12..];

        let decrypted = provider.decrypt_aes(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_e2ee_crypto_provider_xchacha() {
        let provider = E2eeCryptoProvider::new();
        let key = [0x42u8; 32];
        let plaintext = b"Secret message";

        let encrypted = provider.encrypt_xchacha(&key, plaintext.as_ref()).unwrap();
        let nonce = XChaCha20Poly1305Nonce::from_bytes(&encrypted[0..24]).unwrap();
        let ciphertext = &encrypted[24..];

        let decrypted = E2eeCryptoProvider::decrypt_xchacha(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_e2ee_crypto_provider_nonce_tracking() {
        let provider = E2eeCryptoProvider::new();
        let key = Aes256GcmKey::generate();
        let plaintext = b"Test";

        let encrypted = provider.encrypt_aes(&key, plaintext.as_ref()).unwrap();
        let nonce_bytes = &encrypted[0..12];

        assert!(provider.is_nonce_used(nonce_bytes));
        assert_eq!(provider.nonce_counter(), 1);
    }

    #[test]
    fn test_nonce_uniqueness_stress() {
        let provider = E2eeCryptoProvider::with_history_size(1000);
        let key = Aes256GcmKey::generate();
        let plaintext = b"Test";

        let mut nonces = std::collections::HashSet::new();
        for _ in 0..100 {
            let encrypted = provider.encrypt_aes(&key, plaintext.as_ref()).unwrap();
            let nonce_bytes = encrypted[0..12].to_vec();
            assert!(nonces.insert(nonce_bytes), "Duplicate nonce detected!");
        }

        assert_eq!(nonces.len(), 100);
    }

    #[test]
    fn test_xchacha_larger_nonce_space() {
        let provider = E2eeCryptoProvider::new();
        let key = [0x42u8; 32];
        let plaintext = b"Test";

        let encrypted = provider.encrypt_xchacha(&key, plaintext.as_ref()).unwrap();
        assert_eq!(encrypted.len(), 24 + plaintext.len() + 16);
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
    fn test_concurrent_nonce_generation() {
        use std::sync::Arc;
        use std::thread;

        let provider = Arc::new(E2eeCryptoProvider::new());
        let key = Arc::new(Aes256GcmKey::generate());
        let mut handles = vec![];

        for _ in 0..10 {
            let provider_clone = Arc::clone(&provider);
            let key_clone = Arc::clone(&key);
            handles.push(thread::spawn(move || {
                let mut nonces = vec![];
                for _ in 0..10 {
                    let encrypted = provider_clone.encrypt_aes(&key_clone, b"test").unwrap();
                    nonces.push(encrypted[0..12].to_vec());
                }
                nonces
            }));
        }

        let all_nonces: Vec<Vec<u8>> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        let unique_nonces: std::collections::HashSet<Vec<u8>> =
            all_nonces.iter().cloned().collect();

        assert_eq!(all_nonces.len(), 100);
        assert_eq!(
            unique_nonces.len(),
            100,
            "Duplicate nonces detected in concurrent test!"
        );
    }

    #[test]
    fn test_secure_nonce_generator_prevents_reuse() {
        let tracker = Arc::new(NonceTracker::new());
        let generator = SecureNonceGenerator::new(Arc::clone(&tracker));

        let nonce = generator.generate_aes_gcm_nonce().unwrap();
        assert!(tracker.is_nonce_used(nonce.as_bytes()));

        let result = tracker.check_and_record(nonce.as_bytes());
        assert_eq!(result.unwrap_err(), CryptoError::NonceReuseDetected);
    }

    #[test]
    fn test_aes_gcm_nonce_invalid_length() {
        let short_bytes = [0u8; 8];
        let result = Aes256GcmNonce::from_bytes(&short_bytes);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidNonceLength);
    }

    #[test]
    fn test_xchacha_nonce_invalid_length() {
        let short_bytes = [0u8; 12];
        let result = XChaCha20Poly1305Nonce::from_bytes(&short_bytes);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidNonceLength);
    }
}
