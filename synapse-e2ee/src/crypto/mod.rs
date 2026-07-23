use synapse_common::ApiError;
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

impl From<CryptoError> for ApiError {
    fn from(err: CryptoError) -> Self {
        match err {
            CryptoError::EncryptionError(msg) => Self::encryption_error(msg),
            CryptoError::DecryptionError(msg) => Self::decryption_error(msg),
            _ => Self::crypto(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_error_partial_eq_same_variants() {
        assert_eq!(CryptoError::InvalidBase64, CryptoError::InvalidBase64);
        assert_eq!(CryptoError::InvalidKeyLength, CryptoError::InvalidKeyLength);
        assert_eq!(CryptoError::SignatureVerificationFailed, CryptoError::SignatureVerificationFailed);
        assert_eq!(CryptoError::NonceReuseDetected, CryptoError::NonceReuseDetected);
        assert_eq!(CryptoError::NonceCounterOverflow, CryptoError::NonceCounterOverflow);
        assert_eq!(CryptoError::InvalidNonceLength, CryptoError::InvalidNonceLength);
    }

    #[test]
    fn test_crypto_error_partial_eq_string_variants() {
        assert_eq!(CryptoError::EncryptionError("a".into()), CryptoError::EncryptionError("a".into()));
        assert_ne!(CryptoError::EncryptionError("a".into()), CryptoError::EncryptionError("b".into()));
        assert_eq!(CryptoError::DecryptionError("x".into()), CryptoError::DecryptionError("x".into()));
        assert_ne!(CryptoError::DecryptionError("x".into()), CryptoError::DecryptionError("y".into()));
        assert_eq!(CryptoError::HashError("h".into()), CryptoError::HashError("h".into()));
        assert_ne!(CryptoError::HashError("h".into()), CryptoError::HashError("i".into()));
    }

    #[test]
    fn test_crypto_error_partial_eq_different_variants() {
        assert_ne!(CryptoError::InvalidBase64, CryptoError::InvalidKeyLength);
        assert_ne!(CryptoError::EncryptionError("a".into()), CryptoError::DecryptionError("a".into()));
        assert_ne!(CryptoError::NonceReuseDetected, CryptoError::InvalidNonceLength);
    }

    #[test]
    fn test_crypto_error_to_api_error_encryption() {
        let api_err: ApiError = CryptoError::EncryptionError("enc failure".into()).into();
        assert!(!api_err.to_string().is_empty());
    }

    #[test]
    fn test_crypto_error_to_api_error_decryption() {
        let api_err: ApiError = CryptoError::DecryptionError("dec failure".into()).into();
        assert!(!api_err.to_string().is_empty());
    }

    #[test]
    fn test_crypto_error_to_api_error_other_variants() {
        // The catch-all `_` arm covers all non-encryption/decryption variants.
        let api_err: ApiError = CryptoError::InvalidBase64.into();
        assert!(!api_err.to_string().is_empty());

        let api_err: ApiError = CryptoError::NonceReuseDetected.into();
        assert!(!api_err.to_string().is_empty());

        let api_err: ApiError = CryptoError::HashError("h".into()).into();
        assert!(!api_err.to_string().is_empty());
    }
}
