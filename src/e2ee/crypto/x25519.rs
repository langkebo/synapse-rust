use crate::e2ee::crypto::CryptoError;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X25519PublicKey {
    bytes: [u8; 32],
}

impl X25519PublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    pub fn to_base64(&self) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.bytes)
    }

    pub fn from_base64(s: &str) -> Result<Self, CryptoError> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
            .map_err(|_| CryptoError::InvalidBase64)?;
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self::from_bytes(array))
    }
}

#[derive(Debug, Zeroize)]
pub struct X25519SecretKey {
    bytes: [u8; 32],
}

impl X25519SecretKey {
    pub fn generate() -> Self {
        let mut rng = OsRng {};
        let mut key_bytes = [0u8; 32];
        rng.fill_bytes(&mut key_bytes);
        Self { bytes: key_bytes }
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self { bytes: *bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

#[derive(Debug)]
pub struct X25519KeyPair {
    public: X25519PublicKey,
    secret: X25519SecretKey,
}

impl X25519KeyPair {
    pub fn generate() -> Self {
        let secret = X25519SecretKey::generate();
        let public = X25519PublicKey::from_bytes(compute_public_key(&secret));
        Self { public, secret }
    }

    pub fn public_key(&self) -> &X25519PublicKey {
        &self.public
    }

    pub fn diffie_hellman(&self, other_public: &X25519PublicKey) -> [u8; 32] {
        let secret = StaticSecret::from(*self.secret.as_bytes());
        let public = PublicKey::from(*other_public.as_bytes());
        let shared = secret.diffie_hellman(&public);
        let mut result = [0u8; 32];
        result.copy_from_slice(shared.as_bytes());
        result
    }
}

fn compute_public_key(secret: &X25519SecretKey) -> [u8; 32] {
    let secret = StaticSecret::from(*secret.as_bytes());
    let public = PublicKey::from(&secret);
    *public.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x25519_public_key_from_bytes() {
        let bytes = [0x12u8; 32];
        let public_key = X25519PublicKey::from_bytes(bytes);
        assert_eq!(public_key.as_bytes(), &bytes);
    }

    #[test]
    fn test_x25519_public_key_to_base64() {
        let bytes = [0x12u8; 32];
        let public_key = X25519PublicKey::from_bytes(bytes);
        let base64 = public_key.to_base64();
        assert!(!base64.is_empty());
        assert_eq!(base64.len(), 44);
    }

    #[test]
    fn test_x25519_public_key_from_base64_valid() {
        let original_bytes = [0xabu8; 32];
        let public_key = X25519PublicKey::from_bytes(original_bytes);
        let base64 = public_key.to_base64();
        let decoded = X25519PublicKey::from_base64(&base64).unwrap();
        assert_eq!(decoded.as_bytes(), &original_bytes);
    }

    #[test]
    fn test_x25519_public_key_from_base64_invalid_length() {
        let result = X25519PublicKey::from_base64("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_x25519_public_key_from_base64_invalid_base64() {
        let result = X25519PublicKey::from_base64("!!!invalid-base64!!!");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidBase64);
    }

    #[test]
    fn test_x25519_secret_key_generate() {
        let secret_key = X25519SecretKey::generate();
        assert_eq!(secret_key.as_bytes().len(), 32);
    }

    #[test]
    fn test_x25519_secret_key_from_bytes() {
        let bytes = [0x34u8; 32];
        let secret_key = X25519SecretKey::from_bytes(&bytes);
        assert_eq!(secret_key.as_bytes(), &bytes);
    }

    #[test]
    fn test_x25519_key_pair_generate() {
        let key_pair = X25519KeyPair::generate();
        assert_eq!(key_pair.public_key().as_bytes().len(), 32);
    }

    #[test]
    fn test_x25519_key_pair_different_each_time() {
        let key_pair1 = X25519KeyPair::generate();
        let key_pair2 = X25519KeyPair::generate();
        assert_ne!(
            key_pair1.public_key().as_bytes(),
            key_pair2.public_key().as_bytes()
        );
    }

    #[test]
    fn test_x25519_diffie_hellman_shared_secret() {
        let alice_key_pair = X25519KeyPair::generate();
        let bob_key_pair = X25519KeyPair::generate();

        let alice_shared = alice_key_pair.diffie_hellman(bob_key_pair.public_key());
        let bob_shared = bob_key_pair.diffie_hellman(alice_key_pair.public_key());

        assert_eq!(alice_shared, bob_shared);
        assert_eq!(alice_shared.len(), 32);
    }

    #[test]
    fn test_x25519_diffie_hellman_symmetric() {
        let alice_key_pair = X25519KeyPair::generate();
        let bob_key_pair = X25519KeyPair::generate();

        let shared1 = alice_key_pair.diffie_hellman(bob_key_pair.public_key());
        let shared2 = bob_key_pair.diffie_hellman(alice_key_pair.public_key());

        assert_eq!(shared1, shared2);
    }

    #[test]
    fn test_x25519_diffie_hellman_different_pairs() {
        let alice1 = X25519KeyPair::generate();
        let bob1 = X25519KeyPair::generate();
        let alice2 = X25519KeyPair::generate();
        let bob2 = X25519KeyPair::generate();

        let shared1 = alice1.diffie_hellman(bob1.public_key());
        let shared2 = alice2.diffie_hellman(bob2.public_key());

        assert_ne!(shared1, shared2);
    }

    #[test]
    fn test_x25519_shared_secret_not_zero() {
        let alice = X25519KeyPair::generate();
        let bob = X25519KeyPair::generate();

        let shared = alice.diffie_hellman(bob.public_key());

        let is_zero = shared.iter().all(|&b| b == 0);
        assert!(!is_zero, "Shared secret should not be all zeros");
    }

    #[test]
    fn test_x25519_compute_public_key() {
        let secret_key = X25519SecretKey::generate();
        let public_bytes = compute_public_key(&secret_key);
        assert_eq!(public_bytes.len(), 32);

        let expected_public = X25519PublicKey::from_bytes(public_bytes);
        let key_pair = X25519KeyPair {
            public: expected_public,
            secret: secret_key,
        };
        assert_eq!(key_pair.public_key().as_bytes(), &public_bytes);
    }
}
