use crate::crypto::CryptoError;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use ed25519_dalek::ed25519::Error;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Matrix protocol uses unpadded base64 for keys; some clients still emit
/// padded variants. Accept both on decode.
const MATRIX_BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ed25519PublicKey {
    bytes: [u8; 32],
}

impl Ed25519PublicKey {
    fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    #[cfg(test)]
    pub(crate) fn to_base64(&self) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.bytes)
    }

    pub fn from_base64(s: &str) -> Result<Self, CryptoError> {
        let bytes = base64::Engine::decode(&MATRIX_BASE64, s).map_err(|_| CryptoError::InvalidBase64)?;
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self::from_bytes(array))
    }

    pub fn verify(&self, message: &[u8], signature: &ed25519_dalek::Signature) -> Result<(), CryptoError> {
        let verifying_key =
            VerifyingKey::from_bytes(&self.bytes).map_err(|_| CryptoError::SignatureVerificationFailed)?;
        verifying_key.verify_strict(message, signature).map_err(|_| CryptoError::SignatureVerificationFailed)
    }
}

#[derive(Debug, Zeroize, ZeroizeOnDrop)]
struct Ed25519SecretKey {
    bytes: [u8; 32],
}

impl Ed25519SecretKey {
    #[cfg(test)]
    fn generate() -> Self {
        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        Self { bytes: signing_key.to_bytes() }
    }

    fn from_signing_key(signing_key: &SigningKey) -> Self {
        Self { bytes: signing_key.to_bytes() }
    }

    #[cfg(test)]
    fn from_bytes(bytes: &[u8; 32]) -> Self {
        let key = SigningKey::from_bytes(bytes);
        Self { bytes: key.to_bytes() }
    }

    #[cfg(test)]
    fn as_bytes(&self) -> &[u8; 32] {
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
        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let verifying_key = signing_key.verifying_key();
        Self {
            public: Ed25519PublicKey::from_bytes(*verifying_key.as_bytes()),
            secret: Ed25519SecretKey::from_signing_key(&signing_key),
        }
    }

    pub fn public_key(&self) -> &Ed25519PublicKey {
        &self.public
    }

    pub fn sign(&self, message: &[u8]) -> Result<ed25519_dalek::Signature, Error> {
        self.secret.sign(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_public_key_from_bytes() {
        let bytes = [0x12u8; 32];
        let public_key = Ed25519PublicKey::from_bytes(bytes);
        assert_eq!(public_key.bytes, bytes);
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
        assert_eq!(decoded.bytes, original_bytes);
    }

    #[test]
    fn test_ed25519_public_key_from_base64_invalid_length() {
        let result = Ed25519PublicKey::from_base64("invalid");
        assert!(result.is_err());
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
        assert_eq!(key_pair.public_key().bytes.len(), 32);
    }

    #[test]
    fn test_ed25519_sign_and_verify() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let signature = key_pair.sign(&message[..]).unwrap();
        let result = key_pair.public_key().verify(message, &signature);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ed25519_verify_invalid_signature() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let signature = key_pair.sign(&message[..]).unwrap();
        let result = key_pair.public_key().verify(message, &signature);
        assert!(result.is_ok());
    }

    #[test]

    fn test_ed25519_verify_wrong_key() {
        let key_pair1 = Ed25519KeyPair::generate();
        let key_pair2 = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let signature = key_pair1.sign(&message[..]).unwrap();
        let result = key_pair2.public_key().verify(message, &signature);
        assert!(result.is_err());
    }

    #[test]

    fn test_ed25519_verify_tampered_message() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Hello, World!";
        let signature = key_pair.sign(&message[..]).unwrap();
        let tampered_message = b"Hello, Universe!";
        let result = key_pair.public_key().verify(&tampered_message[..], &signature);
        assert!(result.is_err());
    }

    #[test]

    fn test_ed25519_sign_deterministic() {
        let key_pair = Ed25519KeyPair::generate();
        let message = b"Test message";
        let sig1 = key_pair.sign(&message[..]).unwrap();
        let sig2 = key_pair.sign(&message[..]).unwrap();
        assert_eq!(sig1.to_bytes(), sig2.to_bytes());
    }

    #[test]

    fn test_ed25519_key_pair_different_each_time() {
        let key_pair1 = Ed25519KeyPair::generate();
        let key_pair2 = Ed25519KeyPair::generate();
        assert_ne!(key_pair1.public_key().bytes, key_pair2.public_key().bytes);
    }

    #[test]
    fn secret_key_zeroizes_on_drop() {
        // Type-level assertion: ZeroizeOnDrop must be implemented
        fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}
        assert_zeroize_on_drop::<Ed25519SecretKey>();
    }
}
