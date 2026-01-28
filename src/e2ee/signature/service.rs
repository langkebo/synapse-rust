use super::models::*;
use super::storage::SignatureStorage;
use crate::e2ee::crypto::ed25519::Ed25519KeyPair;
use std::sync::Arc;
use crate::error::ApiError;

pub struct SignatureService {
    storage: SignatureStorage<'static>,
}

impl SignatureService {
    pub fn new(storage: SignatureStorage<'static>) -> Self {
        Self { storage }
    }
    
    pub async fn sign_event(&self, event_id: &str, user_id: &str, device_id: &str, key_pair: &Ed25519KeyPair) -> Result<(), ApiError> {
        let message = event_id.as_bytes();
        let signature = key_pair.sign(message);
        
        let event_signature = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: event_id.to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            signature: base64::encode(signature.as_bytes()),
            key_id: format!("ed25519:{}", device_id),
            created_at: Utc::now(),
        };
        
        self.storage.create_signature(&event_signature).await?;
        
        Ok(())
    }
    
    pub async fn verify_event(&self, event_id: &str, user_id: &str, device_id: &str, signature: &str, public_key: &[u8; 32]) -> Result<bool, ApiError> {
        let event_signature = self.storage.get_signature(event_id, user_id, device_id, "ed25519").await?
            .ok_or_else(|| ApiError::NotFound("Signature not found".to_string()))?;
        
        let message = event_id.as_bytes();
        let signature_bytes = base64::decode(event_signature.signature)
            .map_err(|_| ApiError::InvalidInput("Invalid signature encoding".to_string()))?;
        
        let sig = ed25519_dalek::Signature::from_bytes(&signature_bytes)
            .map_err(|_| ApiError::InvalidInput("Invalid signature".to_string()))?;
        
        let public = ed25519_dalek::PublicKey::from_bytes(public_key)
            .map_err(|_| ApiError::InvalidInput("Invalid public key".to_string()))?;
        
        Ok(public.verify(message, &sig).is_ok())
    }
    
    pub async fn sign_key(&self, key: &str, signing_key: &Ed25519KeyPair) -> Result<String, ApiError> {
        let message = key.as_bytes();
        let signature = signing_key.sign(message);
        
        Ok(base64::encode(signature.as_bytes()))
    }
    
    pub async fn verify_key(&self, key: &str, signature: &str, public_key: &[u8; 32]) -> Result<bool, ApiError> {
        let message = key.as_bytes();
        let signature_bytes = base64::decode(signature)
            .map_err(|_| ApiError::InvalidInput("Invalid signature encoding".to_string()))?;
        
        let sig = ed25519_dalek::Signature::from_bytes(&signature_bytes)
            .map_err(|_| ApiError::InvalidInput("Invalid signature".to_string()))?;
        
        let public = ed25519_dalek::PublicKey::from_bytes(public_key)
            .map_err(|_| ApiError::InvalidInput("Invalid public key".to_string()))?;
        
        Ok(public.verify(message, &sig).is_ok())
    }
}