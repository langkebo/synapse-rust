use x25519_dalek::PublicKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
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
pub struct X25519SecretKey {
    bytes: [u8; 32],
}

impl X25519SecretKey {
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let secret = x25519_dalek::EphemeralSecret::random_from_rng(&mut csprng);
        Self {
            bytes: secret.to_bytes(),
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
pub struct X25519KeyPair {
    public: X25519PublicKey,
    secret: X25519SecretKey,
}

impl X25519KeyPair {
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let secret = x25519_dalek::EphemeralSecret::random_from_rng(&mut csprng);
        let public = PublicKey::from(&secret);
        Self {
            public: X25519PublicKey::from_bytes(public.to_bytes()),
            secret: X25519SecretKey::from_bytes(secret.to_bytes()),
        }
    }
    
    pub fn public_key(&self) -> &X25519PublicKey {
        &self.public
    }
    
    pub fn diffie_hellman(&self, other_public: &X25519PublicKey) -> [u8; 32] {
        let secret = x25519_dalek::EphemeralSecret::from(self.secret.as_bytes().as_ref());
        let public = x25519_dalek::PublicKey::from(other_public.as_bytes().as_ref());
        secret.diffie_hellman(&public).to_bytes()
    }
}
