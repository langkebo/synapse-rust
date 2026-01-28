use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature, Signer, Verifier};
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
        let keypair = Keypair::generate(&mut OsRng);
        Self {
            bytes: keypair.secret.to_bytes(),
        }
    }
    
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

#[derive(Debug)]
pub struct Ed25519KeyPair {
    public: Ed25519PublicKey,
    secret: Ed25519SecretKey,
}

impl Ed25519KeyPair {
    pub fn generate() -> Self {
        let keypair = Keypair::generate(&mut OsRng);
        Self {
            public: Ed25519PublicKey::from_bytes(keypair.public.to_bytes()),
            secret: Ed25519SecretKey::from_bytes(keypair.secret.to_bytes()),
        }
    }
    
    pub fn public_key(&self) -> &Ed25519PublicKey {
        &self.public
    }
    
    pub fn sign(&self, message: &[u8]) -> Signature {
        let secret = SecretKey::from_bytes(self.secret.as_bytes()).unwrap();
        let public = PublicKey::from_bytes(self.public.as_bytes()).unwrap();
        let keypair = Keypair { secret, public };
        keypair.sign(message)
    }
    
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), super::CryptoError> {
        let public = PublicKey::from_bytes(self.public.as_bytes()).unwrap();
        public.verify(message, signature)
            .map_err(|_| super::CryptoError::SignatureVerificationFailed)
    }
}