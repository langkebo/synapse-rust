mod aes;
mod ed25519;

pub use aes::{Aes256GcmCipher, Aes256GcmKey, Aes256GcmNonce};
pub use ed25519::{Ed25519KeyPair, Ed25519PublicKey};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Invalid base64 encoding")]
    InvalidBase64,

    #[error("Invalid key length")]
    InvalidKeyLength,

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Decryption error: {0}")]
    DecryptionError(String),

    #[error("Hash error: {0}")]
    HashError(String),

    #[error("Nonce reuse detected")]
    NonceReuseDetected,

    #[error("Nonce counter overflow")]
    NonceCounterOverflow,

    #[error("Invalid nonce length")]
    InvalidNonceLength,
}

impl PartialEq for CryptoError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::InvalidBase64, Self::InvalidBase64) => true,
            (Self::InvalidKeyLength, Self::InvalidKeyLength) => true,
            (Self::SignatureVerificationFailed, Self::SignatureVerificationFailed) => true,
            (Self::EncryptionError(s1), Self::EncryptionError(s2)) => s1 == s2,
            (Self::DecryptionError(s1), Self::DecryptionError(s2)) => s1 == s2,
            (Self::HashError(s1), Self::HashError(s2)) => s1 == s2,
            (Self::NonceReuseDetected, Self::NonceReuseDetected) => true,
            (Self::NonceCounterOverflow, Self::NonceCounterOverflow) => true,
            (Self::InvalidNonceLength, Self::InvalidNonceLength) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_error_partial_eq_same_variant() {
        assert_eq!(CryptoError::InvalidBase64, CryptoError::InvalidBase64);
        assert_eq!(CryptoError::InvalidKeyLength, CryptoError::InvalidKeyLength);
        assert_eq!(CryptoError::SignatureVerificationFailed, CryptoError::SignatureVerificationFailed);
        assert_eq!(CryptoError::NonceReuseDetected, CryptoError::NonceReuseDetected);
        assert_eq!(CryptoError::NonceCounterOverflow, CryptoError::NonceCounterOverflow);
        assert_eq!(CryptoError::InvalidNonceLength, CryptoError::InvalidNonceLength);
    }

    #[test]
    fn test_crypto_error_partial_eq_different_variant() {
        assert_ne!(CryptoError::InvalidBase64, CryptoError::InvalidKeyLength);
        assert_ne!(CryptoError::InvalidBase64, CryptoError::NonceReuseDetected);
        assert_ne!(CryptoError::SignatureVerificationFailed, CryptoError::NonceCounterOverflow);
    }

    #[test]
    fn test_crypto_error_partial_eq_with_message() {
        let e1 = CryptoError::EncryptionError("bad key".to_string());
        let e2 = CryptoError::EncryptionError("bad key".to_string());
        let e3 = CryptoError::EncryptionError("other error".to_string());
        assert_eq!(e1, e2);
        assert_ne!(e1, e3);
        // Different variant types with same message
        assert_ne!(e1, CryptoError::DecryptionError("bad key".to_string()));
    }

    #[test]
    fn test_crypto_error_display() {
        let err = CryptoError::InvalidBase64;
        assert_eq!(err.to_string(), "Invalid base64 encoding");

        let err = CryptoError::InvalidKeyLength;
        assert_eq!(err.to_string(), "Invalid key length");

        let err = CryptoError::SignatureVerificationFailed;
        assert_eq!(err.to_string(), "Signature verification failed");

        let err = CryptoError::EncryptionError("key too short".to_string());
        assert_eq!(err.to_string(), "Encryption error: key too short");

        let err = CryptoError::DecryptionError("bad padding".to_string());
        assert_eq!(err.to_string(), "Decryption error: bad padding");

        let err = CryptoError::HashError("unsupported algo".to_string());
        assert_eq!(err.to_string(), "Hash error: unsupported algo");

        let err = CryptoError::NonceReuseDetected;
        assert_eq!(err.to_string(), "Nonce reuse detected");

        let err = CryptoError::NonceCounterOverflow;
        assert_eq!(err.to_string(), "Nonce counter overflow");

        let err = CryptoError::InvalidNonceLength;
        assert_eq!(err.to_string(), "Invalid nonce length");
    }

    #[test]
    fn test_crypto_error_debug() {
        let err = CryptoError::InvalidBase64;
        assert!(format!("{:?}", err).contains("InvalidBase64"));
        let err = CryptoError::EncryptionError("test".to_string());
        assert!(format!("{:?}", err).contains("EncryptionError"));
    }
}
