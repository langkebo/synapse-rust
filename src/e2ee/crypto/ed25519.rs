use ed25519_dalek::{Signature, Signer, Verifier};
use ed25519_dalek::ed25519::Error;
use rand::rngs::OsRng;
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
        base64::encode(&self.bytes)
    }
    
    pub fn from_base64(s: &str) -> Result<Self, super::CryptoError> {
        let bytes = base64::decode(s)
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
        let mut csprng = OsRng;
        let keypair = ed25519_dalek::SigningKey::generate(&mut csprng);
        Self {
            bytes: keypair.to_bytes(),
        }
    }
    
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
    
    pub fn sign(&self, message: &[u8]) -> Result<Signature, Error> {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(self.bytes.as_ref());
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
        let mut csprng = OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        Self {
            public: Ed25519PublicKey::from_bytes(verifying_key.to_bytes()),
            secret: Ed25519SecretKey::from_bytes(signing_key.to_bytes()),
        }
    }
    
    pub fn public_key(&self) -> &Ed25519PublicKey {
        &self.public
    }
    
    pub fn sign(&self, message: &[u8]) -> Result<Signature, Error> {
        self.secret.sign(message)
    }
    
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), super::CryptoError> {
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(self.public.as_bytes().as_ref())
            .map_err(|_| super::CryptoError::SignatureVerificationFailed)?;
        verifying_key.verify(message, signature)
            .map_err(|_| super::CryptoError::SignatureVerificationFailed)
    }
}
