use crate::crypto::CryptoError;
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use base64::Engine;
#[cfg(test)]
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use dashmap::DashSet;
use generic_array::GenericArray;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use typenum::U32;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Maximum number of nonces retained in the tracker before pruning.
const NONCE_HISTORY_SIZE: usize = 10000;
/// Maximum counter value before overflow (32-bit counter space).
///
/// Only the XChaCha path embeds the counter in the nonce, and that path is
/// test-only. The AES-GCM path draws all 96 bits at random and has no counter
/// space to exhaust, so it needs no overflow guard.
#[cfg(test)]
const NONCE_COUNTER_MAX: u64 = (1u64 << 32) - 1;
/// Longest nonce the tracker accepts (XChaCha20-Poly1305 uses 24 bytes;
/// AES-GCM uses 12). Anything longer is rejected rather than truncated.
const MAX_NONCE_LEN: usize = 24;
/// How many of the oldest entries a single registration may evict once the
/// tracker is full. Bounding the batch amortizes eviction: the previous
/// "drain half the set in one go" approach stalled the encrypt path for
/// thousands of removals every `NONCE_HISTORY_SIZE` messages (audit #5).
const NONCE_PRUNE_BATCH: usize = 256;

// E2EE-03: 密钥材料在 Clone/Drop 时必须零化，与 Ed25519SecretKey 对齐
#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
pub struct Aes256GcmKey {
    bytes: [u8; 32],
}

impl Aes256GcmKey {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
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
        rand::rng().fill_bytes(&mut bytes);
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

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct XChaCha20Poly1305Nonce {
    bytes: [u8; 24],
}

#[cfg(test)]
impl Serialize for XChaCha20Poly1305Nonce {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(self.bytes))
    }
}

#[cfg(test)]
impl<'de> Deserialize<'de> for XChaCha20Poly1305Nonce {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(&s).map_err(serde::de::Error::custom)?;
        Self::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
impl XChaCha20Poly1305Nonce {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 24];
        rand::rng().fill_bytes(&mut bytes);
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

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct Aes256GcmCiphertext {
    nonce: Aes256GcmNonce,
    ciphertext: Vec<u8>,
}

#[cfg(test)]
impl Serialize for Aes256GcmCiphertext {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Aes256GcmCiphertext", 2)?;
        state.serialize_field("nonce", &self.nonce)?;
        state.serialize_field("ciphertext", &base64::engine::general_purpose::STANDARD.encode(&self.ciphertext))?;
        state.end()
    }
}

#[cfg(test)]
impl<'de> Deserialize<'de> for Aes256GcmCiphertext {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let nonce: Aes256GcmNonce =
            serde_json::from_value(value.get("nonce").ok_or_else(|| serde::de::Error::missing_field("nonce"))?.clone())
                .map_err(serde::de::Error::custom)?;
        let ct_str = value
            .get("ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| serde::de::Error::missing_field("ciphertext"))?;
        let ciphertext = base64::engine::general_purpose::STANDARD.decode(ct_str).map_err(serde::de::Error::custom)?;
        Ok(Self::new(nonce, ciphertext))
    }
}

#[cfg(test)]
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

#[cfg(test)]
impl AsRef<[u8]> for Aes256GcmCiphertext {
    fn as_ref(&self) -> &[u8] {
        &self.ciphertext
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct XChaCha20Poly1305Ciphertext {
    nonce: XChaCha20Poly1305Nonce,
    ciphertext: Vec<u8>,
}

#[cfg(test)]
impl Serialize for XChaCha20Poly1305Ciphertext {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("XChaCha20Poly1305Ciphertext", 2)?;
        state.serialize_field("nonce", &self.nonce)?;
        state.serialize_field("ciphertext", &base64::engine::general_purpose::STANDARD.encode(&self.ciphertext))?;
        state.end()
    }
}

#[cfg(test)]
impl<'de> Deserialize<'de> for XChaCha20Poly1305Ciphertext {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let nonce: XChaCha20Poly1305Nonce =
            serde_json::from_value(value.get("nonce").ok_or_else(|| serde::de::Error::missing_field("nonce"))?.clone())
                .map_err(serde::de::Error::custom)?;
        let ct_str = value
            .get("ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| serde::de::Error::missing_field("ciphertext"))?;
        let ciphertext = base64::engine::general_purpose::STANDARD.decode(ct_str).map_err(serde::de::Error::custom)?;
        Ok(Self::new(nonce, ciphertext))
    }
}

#[cfg(test)]
#[allow(dead_code)]
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

#[cfg(test)]
impl AsRef<[u8]> for XChaCha20Poly1305Ciphertext {
    fn as_ref(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Tracks used nonces to detect and prevent nonce reuse in AES-256-GCM.
///
/// AES-256-GCM nonce reuse is catastrophic: reusing a nonce with the same key
/// reveals the authentication key and allows forgery. This tracker maintains
/// a bounded set of recently-used nonces and raises `NonceReuseDetected` on
/// collision.
/// Fixed-size key for the nonce reuse tracker.
///
/// The previous `Vec<u8>` key cost one heap allocation per encrypted message.
/// `len` is stored alongside the bytes so a 12-byte AES-GCM nonce can never
/// collide with a 24-byte XChaCha nonce that merely shares its prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NonceKey {
    len: u8,
    bytes: [u8; MAX_NONCE_LEN],
}

impl NonceKey {
    /// Returns `None` when the nonce is empty or longer than [`MAX_NONCE_LEN`],
    /// so oversized input is rejected instead of silently truncated.
    fn new(nonce: &[u8]) -> Option<Self> {
        if nonce.is_empty() || nonce.len() > MAX_NONCE_LEN {
            return None;
        }
        let mut bytes = [0u8; MAX_NONCE_LEN];
        bytes[..nonce.len()].copy_from_slice(nonce);
        Some(Self { len: nonce.len() as u8, bytes })
    }
}

#[derive(Debug)]
pub struct NonceTracker {
    used_nonces: DashSet<NonceKey>,
    /// Insertion order of the tracked nonces, so eviction can drop the
    /// *oldest* entries. Iterating the `DashSet` yields hash order, which is
    /// what the previous implementation did — it evicted a random subset and
    /// silently dropped reuse protection for recently used nonces.
    order: Mutex<VecDeque<NonceKey>>,
    counter: AtomicU64,
    max_history_size: usize,
}

impl NonceTracker {
    pub fn new() -> Self {
        Self::with_history_size(NONCE_HISTORY_SIZE)
    }

    pub fn with_history_size(max_history_size: usize) -> Self {
        Self {
            used_nonces: DashSet::new(),
            order: Mutex::new(VecDeque::new()),
            counter: AtomicU64::new(0),
            max_history_size,
        }
    }

    pub fn check_and_record(&self, nonce: &[u8]) -> Result<(), CryptoError> {
        let key = NonceKey::new(nonce).ok_or(CryptoError::InvalidNonceLength)?;

        if !self.used_nonces.insert(key) {
            return Err(CryptoError::NonceReuseDetected);
        }

        let mut order = self.order.lock().unwrap_or_else(|e| e.into_inner());
        order.push_back(key);
        drop(order);

        if self.used_nonces.len() >= self.max_history_size {
            self.prune_old_nonces();
        }

        self.counter.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    /// Evicts the oldest entries, at most [`NONCE_PRUNE_BATCH`] per call.
    ///
    /// The bound keeps a single registration from paying for draining the
    /// whole overflow; because it runs on every registration once the tracker
    /// is full, the set still converges back to `max_history_size / 2`.
    fn prune_old_nonces(&self) {
        // Always retain at least one entry so the just-registered nonce
        // survives even with a degenerate `max_history_size` of 1.
        let target = (self.max_history_size / 2).max(1);
        let mut order = self.order.lock().unwrap_or_else(|e| e.into_inner());

        let mut evicted = 0;
        while self.used_nonces.len() > target && evicted < NONCE_PRUNE_BATCH {
            let Some(oldest) = order.pop_front() else { break };
            self.used_nonces.remove(&oldest);
            evicted += 1;
        }
    }

    pub fn counter(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }

    pub fn is_nonce_used(&self, nonce: &[u8]) -> bool {
        NonceKey::new(nonce).is_some_and(|key| self.used_nonces.contains(&key))
    }

    pub fn clear(&self) {
        let mut order = self.order.lock().unwrap_or_else(|e| e.into_inner());
        order.clear();
        self.used_nonces.clear();
        self.counter.store(0, Ordering::SeqCst);
    }
}

impl Default for NonceTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Generates AES-GCM nonces from a CSPRNG, with reuse detection as backstop.
///
/// AES-GCM nonces are 96 bits drawn entirely from the CSPRNG, per NIST
/// SP 800-38D. The `NonceTracker` is defense-in-depth, not the primary
/// guarantee.
///
/// The previous scheme used a 4-byte random prefix plus an 8-byte counter
/// starting at zero. Uniqueness within one generator instance came from the
/// counter, so it collapsed to the 32-bit random prefix whenever the counter
/// restarted — and it restarts on every service rebuild, while the encryption
/// key is a long-lived configured secret. Reusing a nonce under the same key
/// is catastrophic for GCM (it leaks the authentication subkey and permits
/// forgery). Drawing all 96 bits at random removes the dependency on any
/// cross-restart state (audit #2).
#[derive(Debug)]
pub struct SecureNonceGenerator {
    /// Observability only: how many nonces this generator has produced.
    /// Deliberately not part of the nonce.
    counter: AtomicU64,
    tracker: Arc<NonceTracker>,
}

impl SecureNonceGenerator {
    pub fn new(tracker: Arc<NonceTracker>) -> Self {
        Self { counter: AtomicU64::new(0), tracker }
    }

    pub fn generate_aes_gcm_nonce(&self) -> Result<Aes256GcmNonce, CryptoError> {
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_bytes);

        self.tracker.check_and_record(&nonce_bytes)?;

        self.counter.fetch_add(1, Ordering::SeqCst);

        Ok(Aes256GcmNonce { bytes: nonce_bytes })
    }

    #[cfg(test)]
    pub fn generate_xchacha_nonce(&self) -> Result<XChaCha20Poly1305Nonce, CryptoError> {
        let counter = self.counter.fetch_add(1, Ordering::SeqCst);
        if counter >= NONCE_COUNTER_MAX {
            return Err(CryptoError::NonceCounterOverflow);
        }

        let mut nonce_bytes = [0u8; 24];
        rand::rng().fill_bytes(&mut nonce_bytes[0..16]);

        nonce_bytes[16..24].copy_from_slice(&counter.to_be_bytes());

        self.tracker.check_and_record(&nonce_bytes)?;

        Ok(XChaCha20Poly1305Nonce { bytes: nonce_bytes })
    }

    #[cfg(test)]
    pub fn counter(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }

    /// Borrow the underlying `NonceTracker` for inspection.
    pub fn tracker(&self) -> &NonceTracker {
        &self.tracker
    }
}

/// AES-256-GCM cipher with built-in nonce reuse detection.
///
/// The default constructor wires up a `SecureNonceGenerator` backed by a
/// `NonceTracker` so that every encryption operation uses a counter-based
/// nonce with collision detection. This is critical for AES-256-GCM where
/// nonce reuse with the same key is catastrophic.
#[derive(Debug, Clone)]
pub struct Aes256GcmCipher {
    nonce_generator: Option<Arc<SecureNonceGenerator>>,
}

impl Default for Aes256GcmCipher {
    fn default() -> Self {
        let tracker = Arc::new(NonceTracker::new());
        let nonce_generator = Arc::new(SecureNonceGenerator::new(tracker));
        Self { nonce_generator: Some(nonce_generator) }
    }
}

impl Aes256GcmCipher {
    pub fn with_nonce_tracker(tracker: Arc<NonceTracker>) -> Self {
        let nonce_generator = Arc::new(SecureNonceGenerator::new(tracker));
        Self { nonce_generator: Some(nonce_generator) }
    }

    /// Split encrypted data (nonce || ciphertext) into its components.
    pub fn split_encrypted_data(encrypted: &[u8]) -> Result<(Aes256GcmNonce, &[u8]), CryptoError> {
        if encrypted.len() < 12 {
            return Err(CryptoError::InvalidNonceLength);
        }
        let nonce = Aes256GcmNonce::from_bytes(&encrypted[0..12])?;
        let ciphertext = &encrypted[12..];
        Ok((nonce, ciphertext))
    }

    /// Encrypt plaintext using AES-256-GCM with nonce reuse detection.
    ///
    /// The nonce is generated via `SecureNonceGenerator` (counter-based with
    /// collision detection) when available, falling back to random generation
    /// only when no tracker is configured.
    pub fn encrypt_with_nonce(&self, key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = if let Some(ref gen) = self.nonce_generator {
            gen.generate_aes_gcm_nonce()?
        } else {
            Aes256GcmNonce::generate()
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

    /// Check whether a nonce has been used by this cipher's tracker.
    pub fn is_nonce_used(&self, nonce: &[u8]) -> bool {
        self.nonce_generator.as_ref().map(|gen| gen.tracker().is_nonce_used(nonce)).unwrap_or(false)
    }

    /// Return the total number of nonces generated by this cipher's tracker.
    pub fn nonce_counter(&self) -> u64 {
        self.nonce_generator.as_ref().map(|gen| gen.tracker().counter()).unwrap_or(0)
    }

    pub fn decrypt(key: &Aes256GcmKey, nonce: &Aes256GcmNonce, encrypted: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher_key = GenericArray::<u8, U32>::from_slice(&key.bytes);
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(&nonce.bytes);

        let plaintext =
            cipher.decrypt(nonce_bytes, encrypted).map_err(|e| CryptoError::DecryptionError(e.to_string()))?;

        Ok(plaintext)
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct XChaCha20Poly1305Cipher {
    nonce_generator: Option<Arc<SecureNonceGenerator>>,
}

#[cfg(test)]
impl XChaCha20Poly1305Cipher {
    pub fn new() -> Self {
        Self { nonce_generator: None }
    }

    pub fn with_nonce_tracker(tracker: Arc<NonceTracker>) -> Self {
        let nonce_generator = Arc::new(SecureNonceGenerator::new(tracker));
        Self { nonce_generator: Some(nonce_generator) }
    }

    pub fn encrypt(&self, key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = if let Some(ref gen) = self.nonce_generator {
            gen.generate_xchacha_nonce()?
        } else {
            XChaCha20Poly1305Nonce::generate()
        };

        let cipher = XChaCha20Poly1305::new(key.into());
        let nonce_bytes = XNonce::from_slice(nonce.as_bytes());

        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e: chacha20poly1305::aead::Error| CryptoError::EncryptionError(e.to_string()))?;

        let mut result = Vec::with_capacity(nonce.as_bytes().len() + ciphertext.len());
        result.extend_from_slice(nonce.as_bytes());
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    pub fn decrypt(key: &[u8; 32], nonce: &XChaCha20Poly1305Nonce, encrypted: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = XChaCha20Poly1305::new(key.into());
        let nonce_bytes = XNonce::from_slice(nonce.as_bytes());

        let plaintext =
            cipher.decrypt(nonce_bytes, encrypted).map_err(|e| CryptoError::DecryptionError(e.to_string()))?;

        Ok(plaintext)
    }
}

#[cfg(test)]
impl Default for XChaCha20Poly1305Cipher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct E2eeCryptoProvider {
    aes_cipher: Aes256GcmCipher,
    xchacha_cipher: XChaCha20Poly1305Cipher,
    nonce_tracker: Arc<NonceTracker>,
}

#[cfg(test)]
#[allow(dead_code)]
impl E2eeCryptoProvider {
    pub fn new() -> Self {
        let nonce_tracker = Arc::new(NonceTracker::new());
        let aes_cipher = Aes256GcmCipher::with_nonce_tracker(Arc::clone(&nonce_tracker));
        let xchacha_cipher = XChaCha20Poly1305Cipher::with_nonce_tracker(Arc::clone(&nonce_tracker));

        Self { aes_cipher, xchacha_cipher, nonce_tracker }
    }

    pub fn with_history_size(max_history_size: usize) -> Self {
        let nonce_tracker = Arc::new(NonceTracker::with_history_size(max_history_size));
        let aes_cipher = Aes256GcmCipher::with_nonce_tracker(Arc::clone(&nonce_tracker));
        let xchacha_cipher = XChaCha20Poly1305Cipher::with_nonce_tracker(Arc::clone(&nonce_tracker));

        Self { aes_cipher, xchacha_cipher, nonce_tracker }
    }

    pub fn encrypt_aes(&self, key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.aes_cipher.encrypt_with_nonce(key, plaintext)
    }

    pub fn decrypt_aes(
        &self,
        key: &Aes256GcmKey,
        nonce: &Aes256GcmNonce,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        Aes256GcmCipher::decrypt(key, nonce, encrypted)
    }

    pub fn encrypt_xchacha(&self, key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
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

#[cfg(test)]
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

    // E2EE-03: AES 密钥材料必须与 Ed25519 私钥一样实现零化
    #[test]
    fn test_aes256_gcm_key_implements_zeroize_on_drop() {
        fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}
        assert_zeroize_on_drop::<Aes256GcmKey>();
    }

    #[test]
    fn test_aes256_gcm_key_zeroize_clears_bytes() {
        let mut key = Aes256GcmKey::from_bytes([0xABu8; 32]);
        key.zeroize();
        assert_eq!(&key.bytes, &[0u8; 32]);
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

        let encrypted = Aes256GcmCipher::default().encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();
        assert!(encrypted.len() > 12);

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes256_gcm_encrypt_different_nonces() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Test message";

        let encrypted1 = Aes256GcmCipher::default().encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();
        let encrypted2 = Aes256GcmCipher::default().encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();

        assert_ne!(encrypted1[0..12], encrypted2[0..12]);
        assert_ne!(encrypted1[12..], encrypted2[12..]);
    }

    #[test]
    fn test_aes256_gcm_decrypt_wrong_key() {
        let key1 = Aes256GcmKey::generate();
        let key2 = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::default().encrypt_with_nonce(&key1, plaintext.as_ref()).unwrap();
        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let result = Aes256GcmCipher::decrypt(&key2, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_decrypt_wrong_nonce() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let encrypted = Aes256GcmCipher::default().encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();
        let ciphertext = &encrypted[12..];

        let wrong_nonce = Aes256GcmNonce::generate();
        let result = Aes256GcmCipher::decrypt(&key, &wrong_nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_decrypt_tampered_ciphertext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret data";

        let mut encrypted = Aes256GcmCipher::default().encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();
        encrypted[12] ^= 0xff;

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let result = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes256_gcm_encrypt_empty_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = b"";

        let encrypted = Aes256GcmCipher::default().encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();
        assert_eq!(encrypted.len(), 28);

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_aes256_gcm_encrypt_large_plaintext() {
        let key = Aes256GcmKey::generate();
        let plaintext = vec![0x42u8; 10000];

        let encrypted = Aes256GcmCipher::default().encrypt_with_nonce(&key, &plaintext).unwrap();
        assert_eq!(encrypted.len(), 10028);

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

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
        assert_ne!(&nonce1.bytes, &nonce2.bytes);
    }

    #[test]
    fn test_xchacha20_poly1305_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let plaintext = b"Hello, XChaCha20-Poly1305!";

        let encrypted = XChaCha20Poly1305Cipher::new().encrypt(&key, plaintext.as_ref()).unwrap();
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

        let encrypted1 = XChaCha20Poly1305Cipher::new().encrypt(&key, plaintext.as_ref()).unwrap();
        let encrypted2 = XChaCha20Poly1305Cipher::new().encrypt(&key, plaintext.as_ref()).unwrap();

        assert_ne!(encrypted1[0..24], encrypted2[0..24]);
        assert_ne!(encrypted1[24..], encrypted2[24..]);
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
    // 审计 #2：AES-GCM 的 96 位 nonce 必须全部来自 CSPRNG。
    //
    // 旧实现是「4 字节随机 + 8 字节计数器」，计数器在服务重建时归零，而加密密钥
    // 是配置里的长期密钥——跨重启后唯一性退化到 32 位随机前缀。这里锁住的是
    // 「计数器不再参与 nonce 构造」这件事：后 8 字节必须不可预测且随样本变化。
    fn test_aes_gcm_nonce_is_fully_random_not_counter_derived() {
        let tracker = Arc::new(NonceTracker::new());
        let generator = SecureNonceGenerator::new(Arc::clone(&tracker));

        let nonce1 = generator.generate_aes_gcm_nonce().unwrap();
        let nonce2 = generator.generate_aes_gcm_nonce().unwrap();

        // 旧实现会把计数器 0 / 1 直接写进后 8 字节。
        assert_ne!(&nonce1.bytes[4..12], &0u64.to_be_bytes(), "nonce must not embed a zeroed counter");
        assert_ne!(&nonce2.bytes[4..12], &1u64.to_be_bytes(), "nonce must not embed an incrementing counter");

        // 曾经的计数器位必须是随机的：32 个样本的末字节应呈现高基数分布，
        // 若退回计数器方案，这里只会拿到极少数几个不同取值。
        let last_bytes: std::collections::HashSet<u8> =
            (0..32).map(|_| generator.generate_aes_gcm_nonce().unwrap().bytes[11]).collect();
        assert!(
            last_bytes.len() > 8,
            "trailing nonce byte must be random, got only {} distinct values in 32 samples",
            last_bytes.len()
        );
    }

    // 审计 #2：服务重建（新生成器 + 同一持久密钥）不得产生可预测的 nonce。
    #[test]
    fn test_aes_gcm_nonce_unique_across_generator_restart() {
        let tracker = Arc::new(NonceTracker::new());
        let before = SecureNonceGenerator::new(Arc::clone(&tracker));
        // 模拟服务重建：全新生成器，但共用同一个 tracker（同一持久密钥场景）。
        let after = SecureNonceGenerator::new(Arc::clone(&tracker));

        assert_ne!(
            before.generate_aes_gcm_nonce().unwrap().bytes,
            after.generate_aes_gcm_nonce().unwrap().bytes,
            "a restarted generator must not repeat the previous instance's nonce"
        );
    }

    #[test]
    fn test_e2ee_crypto_provider_aes() {
        let provider = E2eeCryptoProvider::new();
        let key = Aes256GcmKey::generate();
        let plaintext = b"Secret message";

        let encrypted = provider.encrypt_aes(&key, plaintext.as_ref()).unwrap();
        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

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
        let (nonce, _) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();

        assert!(provider.is_nonce_used(&nonce.bytes));
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
            let (nonce, _) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();
            assert!(nonces.insert(nonce.bytes.to_vec()), "Duplicate nonce detected!");
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

    // 审计 #5：剪枝必须淘汰**最旧**的条目。此前直接迭代 DashSet，得到的是哈希序，
    // 等于随机丢弃一半——近期用过的 nonce 也可能被丢掉而失去重用保护。
    #[test]
    fn test_nonce_tracker_prunes_oldest_first() {
        let tracker = NonceTracker::with_history_size(4);

        for i in 0..4u8 {
            tracker.check_and_record(&[i; 12]).unwrap();
        }

        // 填满后剪到 max/2 = 2 条，留下的必须是最新的两条。
        assert_eq!(tracker.used_nonces.len(), 2, "tracker should retain max_history_size / 2 entries");
        assert!(!tracker.is_nonce_used(&[0u8; 12]), "oldest nonce must be evicted first");
        assert!(!tracker.is_nonce_used(&[1u8; 12]), "second-oldest nonce must be evicted next");
        assert!(tracker.is_nonce_used(&[2u8; 12]), "recently used nonce must survive pruning");
        assert!(tracker.is_nonce_used(&[3u8; 12]), "most recent nonce must survive pruning");
    }

    // 审计 #5：键按 (长度, 字节) 索引，12 字节的 AES-GCM nonce 不能和同前缀的
    // 24 字节 XChaCha nonce 视为同一条，否则会误报 nonce 重用。
    #[test]
    fn test_nonce_tracker_distinguishes_nonce_lengths() {
        let tracker = NonceTracker::new();

        tracker.check_and_record(&[1u8; 12]).unwrap();
        tracker.check_and_record(&[1u8; 24]).unwrap();

        assert!(tracker.is_nonce_used(&[1u8; 12]));
        assert!(tracker.is_nonce_used(&[1u8; 24]));
    }

    #[test]
    fn test_nonce_tracker_rejects_invalid_lengths() {
        let tracker = NonceTracker::new();

        assert_eq!(tracker.check_and_record(&[]).unwrap_err(), CryptoError::InvalidNonceLength);
        assert_eq!(tracker.check_and_record(&[0u8; 25]).unwrap_err(), CryptoError::InvalidNonceLength);
        assert_eq!(tracker.counter(), 0, "rejected nonces must not advance the counter");
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
                    let (nonce, _) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();
                    nonces.push(nonce.bytes.to_vec());
                }
                nonces
            }));
        }

        let all_nonces: Vec<Vec<u8>> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        let unique_nonces: std::collections::HashSet<Vec<u8>> = all_nonces.iter().cloned().collect();

        assert_eq!(all_nonces.len(), 100);
        assert_eq!(unique_nonces.len(), 100, "Duplicate nonces detected in concurrent test!");
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

    #[test]
    fn test_xchacha_nonce_invalid_length() {
        let short_bytes = [0u8; 12];
        let result = XChaCha20Poly1305Nonce::from_bytes(short_bytes);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidNonceLength);
    }

    // =========================================================================
    // B.3 batch 4/6 — supplemental coverage for production (non-cfg-test) paths.
    // =========================================================================

    #[test]
    fn test_encrypt_with_nonce_roundtrip() {
        // Covers Aes256GcmCipher::encrypt_with_nonce (instance method with nonce tracking).
        let cipher = Aes256GcmCipher::default();
        let key = Aes256GcmKey::generate();
        let plaintext = b"encrypt_with_nonce roundtrip test";

        let encrypted = cipher.encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();
        assert!(encrypted.len() > 12);

        let (nonce, ciphertext) = Aes256GcmCipher::split_encrypted_data(&encrypted).unwrap();
        let decrypted = Aes256GcmCipher::decrypt(&key, &nonce, ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_with_nonce_empty_plaintext() {
        let cipher = Aes256GcmCipher::default();
        let key = Aes256GcmKey::generate();
        let encrypted = cipher.encrypt_with_nonce(&key, b"").unwrap();
        // 12 (nonce) + 0 (ciphertext) + 16 (GCM tag)
        assert_eq!(encrypted.len(), 28);
    }

    #[test]
    fn test_aes256_gcm_nonce_serde_roundtrip() {
        // Covers Serialize/Deserialize for Aes256GcmNonce (non-cfg-test impls).
        let nonce = Aes256GcmNonce::from_bytes([0xABu8; 12]).unwrap();
        let json = serde_json::to_string(&nonce).unwrap();
        // Base64 of [0xAB; 12] = "q6urq6urq6urq6ur" (16 chars, no padding).
        assert_eq!(json, r#""q6urq6urq6urq6ur""#);

        let deserialized: Aes256GcmNonce = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.as_bytes(), nonce.as_bytes());
    }

    #[test]
    fn test_aes256_gcm_nonce_serde_invalid_base64() {
        let result: Result<Aes256GcmNonce, _> = serde_json::from_str(r#""!!!not-base64!!!""#);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_encrypted_data_too_short() {
        // Covers the error branch in split_encrypted_data (len < 12).
        let short_data = [0u8; 5];
        let result = Aes256GcmCipher::split_encrypted_data(&short_data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidNonceLength);
    }

    // =========================================================================
    // E2EE-04: Production nonce reuse detection
    // =========================================================================

    #[test]
    fn test_default_cipher_tracks_nonces_in_production() {
        // E2EE-04: Default cipher must include nonce reuse detection
        // even in production (non-test) builds. The Default impl must
        // wire up a SecureNonceGenerator backed by a NonceTracker.
        let cipher = Aes256GcmCipher::default();
        let key = Aes256GcmKey::generate();
        let plaintext = b"production nonce tracking";

        let encrypted = cipher.encrypt_with_nonce(&key, plaintext.as_ref()).unwrap();
        let nonce_bytes = &encrypted[0..12];

        assert!(cipher.is_nonce_used(nonce_bytes), "Default cipher must track nonce reuse");
        assert_eq!(cipher.nonce_counter(), 1);
    }
}
