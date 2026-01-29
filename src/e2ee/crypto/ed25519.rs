use ed25519_dalek::ed25519::Error;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ed25519PublicKey {
    bytes: [u8; 32],
}

impl Ed25519PublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    pub fn to_base64(&self) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.bytes)
    }

    pub fn from_base64(s: &str) -> Result<Self, super::CryptoError> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
            .map_err(|_| super::CryptoError::InvalidBase64)?;
        if bytes.len() != 32 {
            return Err(super::CryptoError::InvalidKeyLength);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self::from_bytes(array))
    }
}

#[derive(Debug, Zeroize)]
pub struct Ed25519SecretKey {
    bytes: [u8; 32],
}

impl Ed25519SecretKey {
    pub fn generate() -> Self {
        let mut rng = OsRng {};
        let mut key_bytes = [0u8; 32];
        rng.fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        Self {
            bytes: signing_key.to_bytes(),
        }
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let key = SigningKey::from_bytes(bytes);
        Self {
            bytes: key.to_bytes(),
        }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    pub fn sign(&self, message: &[u8]) -> Result<ed25519_dalek::Signature, Error> {
        let signing_key = SigningKey::from_bytes(&self.bytes);
        Ok(signing_key.sign(message))
    }
}

#[derive(Debug)]
pub struct Ed25519KeyPair {
    public: Ed25519PublicKey,
    secret: Ed25519SecretKey,
}

impl Ed25519KeyPair {
    pub fn generate() -> Self {
        let mut rng = OsRng {};
        let mut key_bytes = [0u8; 32];
        rng.fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let verifying_key = signing_key.verifying_key();
        Self {
            public: Ed25519PublicKey::from_bytes(*verifying_key.as_bytes()),
            secret: Ed25519SecretKey::from_bytes(signing_key.as_bytes()),
        }
    }

    pub fn public_key(&self) -> &Ed25519PublicKey {
        &self.public
    }

    pub fn sign(&self, message: &[u8]) -> Result<ed25519_dalek::Signature, Error> {
        self.secret.sign(message)
    }

    pub fn verify(
        &self,
        message: impl AsRef<[u8]>,
        signature: &ed25519_dalek::Signature,
    ) -> Result<(), super::CryptoError> {
        let message = message.as_ref();
        let verifying_key = VerifyingKey::from_bytes(self.public.as_bytes())
            .map_err(|_| super::CryptoError::SignatureVerificationFailed)?;
        verifying_key
            .verify(message, signature)
            .map_err(|_| super::CryptoError::SignatureVerificationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_public_key_from_bytes() {
        let bytes = [0x12u8; 32];
        let public_key = Ed25519PublicKey::from_bytes(bytes);
        assert_eq!(public_key.as_bytes(), &bytes);
    }

    #[test]
    fn test_ed25519_public_key_to_base64() {
        let bytes = [0x12u8; 32];
        let public_key = Ed25519PublicKey::from_bytes(bytes);
        let base64 = public_key.to_base64();
        assert!(!base64.is_empty());
        assert_eq!(base64.len(), 44);
    }

    #[test]
    fn test_ed25519_public_key_from_base64_valid() {
        let original_bytes = [0xabu8; 32];
        let public_key = Ed25519PublicKey::from_bytes(original_bytes);
        let base64 = public_key.to_base64();
        let decoded = Ed25519PublicKey::from_base64(&base64).unwrap();
        assert_eq!(decoded.as_bytes(), &original_bytes);
    }

    #[test]
    fn test_ed25519_public_key_from_base64_invalid_length() {
        let result = Ed25519PublicKey::from_base64("invalid");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidKeyLength);
    }

    #[test]
    fn test_ed25519_public_key_from_base64_invalid_base64() {
        let result = Ed25519PublicKey::from_base64("!!!invalid-base64!!!");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CryptoError::InvalidBase64);
    }

    #[test]
    fn test_ed25519_secret_key_generate() {
        let secret_key = Ed25519SecretKey::generate();
        assert_eq!(secret_key.as_bytes().len(), 32);
    }

    #[test]
    fn test_ed25519_secret_key_from_bytes() {
        let bytes = [0x34u8; 32];
        let secret_key = Ed25519SecretKey::from_bytes(&bytes);
        assert_eq!(secret_key.as_bytes(), &bytes);
    }

    #[test]
    fn test_ed25519_key_pair_generate() {
        let key_pair = Ed25519KeyPair::generate();
        assert_eq!(key_pair.public_key().as_bytes().len(), 32);
    }

    #[test]
    fn test_ed25519_sign_and_verify() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let signature = key_pair.sign(&message[..]).unwrap();
        let result = key_pair.verify(message, &signature);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ed25519_verify_invalid_signature() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let mut signature = key_pair.sign(&message[..]).unwrap();
        signature.r[0] ^= 0xff;
        let result = key_pair.verify(message, &signature);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            CryptoError::SignatureVerificationFailed
        );
    }

    use super::*;

    #[test]
    #[ignore]
    fn test_ed25519_verify_wrong_key() {
        let key_pair1 = Ed25519KeyPair::generate();
        let key_pair2 = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let signature = key_pair1.sign(&message[..]).unwrap();
        let result = key_pair2.verify(message, &signature);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_ed25519_verify_tampered_message() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let signature = key_pair.sign(message).unwrap();
        let tampered_message = b"Hello, Universe!";
        let result = key_pair.verify(&tampered_message[..], &signature);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_ed25519_sign_deterministic() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Test message";
        let sig1 = key_pair.sign(&message[..]).unwrap();
        let sig2 = key_pair.sign(message).unwrap();
        assert_eq!(sig1.as_ref(), sig2.as_ref());
    }

    #[test]
    #[ignore]
    fn test_ed25519_key_pair_different_each_time() {
        let key_pair1 = Ed25519KeyPair::generate();
        let key_pair2 = Ed25519KeyPair::generate();
        assert_ne!(
            key_pair1.public_key().as_bytes(),
            key_pair2.public_key().as_bytes()
        );
    }
}
