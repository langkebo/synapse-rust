use super::models::*;
use super::storage::SignatureStorage;
use crate::e2ee::crypto::ed25519::Ed25519KeyPair;
use crate::error::ApiError;
use chrono::Utc;
use ed25519_dalek::Verifier;
use ed25519_dalek::VerifyingKey;

pub struct SignatureService {
    storage: SignatureStorage<'static>,
}

impl SignatureService {
    pub fn new(storage: SignatureStorage<'static>) -> Self {
        Self { storage }
    }

    pub async fn sign_event(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        key_pair: &Ed25519KeyPair,
    ) -> Result<(), ApiError> {
        let message = event_id.as_bytes();
        let signature = key_pair.sign(message)?;

        let event_signature = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: event_id.to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            signature: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                signature.to_bytes(),
            ),
            key_id: format!("ed25519:{}", device_id),
            created_at: Utc::now().timestamp(),
        };

        self.storage.create_signature(&event_signature).await?;

        Ok(())
    }

    pub async fn verify_event(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        public_key: &[u8; 32],
    ) -> Result<bool, ApiError> {
        let message = event_id.as_bytes();
        let signature_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature)
            .map_err(|_| ApiError::invalid_input("Invalid signature encoding"))?;

        if signature_bytes.len() != 64 {
            return Err(ApiError::invalid_input("Invalid signature length"));
        }

        let sig = ed25519_dalek::Signature::try_from(&signature_bytes[..])
            .map_err(|_| ApiError::invalid_input("Invalid signature"))?;

        let public = VerifyingKey::from_bytes(public_key)
            .map_err(|_| ApiError::invalid_input("Invalid public key"))?;

        Ok(public.verify(message, &sig).is_ok())
    }

    pub async fn sign_key(
        &self,
        key: &str,
        signing_key: &Ed25519KeyPair,
    ) -> Result<String, ApiError> {
        let message = key.as_bytes();
        let signature = signing_key.sign(message)?;

        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            signature.to_bytes(),
        ))
    }

    pub async fn verify_key(
        &self,
        key: &str,
        signature: &str,
        public_key: &[u8; 32],
    ) -> Result<bool, ApiError> {
        let message = key.as_bytes();
        let signature_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature)
            .map_err(|_| ApiError::invalid_input("Invalid signature encoding"))?;

        if signature_bytes.len() != 64 {
            return Err(ApiError::invalid_input("Invalid signature length"));
        }

        let sig = ed25519_dalek::Signature::try_from(&signature_bytes[..])
            .map_err(|_| ApiError::invalid_input("Invalid signature"))?;

        let public = VerifyingKey::from_bytes(public_key)
            .map_err(|_| ApiError::invalid_input("Invalid public key"))?;

        Ok(public.verify(message, &sig).is_ok())
    }
}
