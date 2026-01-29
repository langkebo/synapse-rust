pub mod aes;
pub mod argon2;
pub mod ed25519;
pub mod x25519;

pub use aes::*;
pub use argon2::*;
pub use ed25519::*;
pub use x25519::*;

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
}
