use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, NewAead}};
use rand::Rng;
use serde::{Deserialize, Serialize};

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
    
    pub fn from_bytes(bytes: [u8; 12]) -> Self {
        Self { bytes }
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

pub struct Aes256GcmCipher;

impl Aes256GcmCipher {
    pub fn encrypt(key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Aes256GcmCiphertext, super::CryptoError> {
        let nonce = Aes256GcmNonce::generate();
        let cipher_key = Key::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(nonce.as_bytes());
        
        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e| super::CryptoError::EncryptionError(e.to_string()))?;
        
        Ok(Aes256GcmCiphertext::new(nonce, ciphertext))
    }
    
    pub fn decrypt(key: &Aes256GcmKey, encrypted: &Aes256GcmCiphertext) -> Result<Vec<u8>, super::CryptoError> {
        let cipher_key = Key::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(encrypted.nonce().as_bytes());
        
        let plaintext = cipher
            .decrypt(nonce_bytes, encrypted.ciphertext().as_ref())
            .map_err(|e| super::CryptoError::DecryptionError(e.to_string()))?;
        
        Ok(plaintext)
    }
}