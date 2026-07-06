use super::models::*;
use super::storage::SignatureStorage;
use crate::crypto::Ed25519KeyPair;
use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use synapse_common::ApiError;

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
        let signature = key_pair.sign(message).map_err(|e| ApiError::crypto(e.to_string()))?;

        let event_signature = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: event_id.to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            signature: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, signature.to_bytes()),
            key_id: format!("ed25519:{device_id}"),
            created_ts: Utc::now().timestamp(),
        };

        self.storage.create_signature(&event_signature).await?;

        Ok(())
    }

    pub fn verify_event(
        &self,
        event_id: &str,
        _user_id: &str,
        _device_id: &str,
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

        let public = VerifyingKey::from_bytes(public_key).map_err(|_| ApiError::invalid_input("Invalid public key"))?;

        Ok(public.verify_strict(message, &sig).is_ok())
    }

    pub fn sign_key(&self, key: &str, signing_key: &Ed25519KeyPair) -> Result<String, ApiError> {
        let message = key.as_bytes();
        let signature = signing_key.sign(message).map_err(|e| ApiError::crypto(e.to_string()))?;

        Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, signature.to_bytes()))
    }

    pub fn verify_key(&self, key: &str, signature: &str, public_key: &[u8; 32]) -> Result<bool, ApiError> {
        let message = key.as_bytes();
        let signature_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature)
            .map_err(|_| ApiError::invalid_input("Invalid signature encoding"))?;

        if signature_bytes.len() != 64 {
            return Err(ApiError::invalid_input("Invalid signature length"));
        }

        let sig = ed25519_dalek::Signature::try_from(&signature_bytes[..])
            .map_err(|_| ApiError::invalid_input("Invalid signature"))?;

        let public = VerifyingKey::from_bytes(public_key).map_err(|_| ApiError::invalid_input("Invalid public key"))?;

        Ok(public.verify_strict(message, &sig).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};

    fn make_service() -> SignatureService {
        let pool = sqlx::PgPool::connect_lazy("postgres:///test").expect("connect_lazy should not perform I/O");
        let leaked: &'static sqlx::PgPool = Box::leak(Box::new(pool));
        SignatureService::new(SignatureStorage::new(leaked))
    }

    #[tokio::test]
    async fn verify_key_valid_signature() {
        let svc = make_service();
        let signing_key = SigningKey::generate(&mut aes_gcm::aead::OsRng);
        let verifying_key = signing_key.verifying_key();
        let message = "test-key-data";
        let signature = signing_key.sign(message.as_bytes());
        let encoded = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        let result = svc.verify_key(message, &encoded, verifying_key.as_bytes()).unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn verify_key_tampered_message() {
        let svc = make_service();
        let signing_key = SigningKey::generate(&mut aes_gcm::aead::OsRng);
        let verifying_key = signing_key.verifying_key();
        let signature = signing_key.sign(b"original");
        let encoded = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        let result = svc.verify_key("tampered", &encoded, verifying_key.as_bytes()).unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn verify_key_wrong_public_key() {
        let svc = make_service();
        let signing_key = SigningKey::generate(&mut aes_gcm::aead::OsRng);
        let other_key = SigningKey::generate(&mut aes_gcm::aead::OsRng);
        let message = "test";
        let signature = signing_key.sign(message.as_bytes());
        let encoded = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        let result = svc.verify_key(message, &encoded, other_key.verifying_key().as_bytes()).unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn verify_key_rejects_invalid_base64() {
        let svc = make_service();
        let public_key = [0u8; 32];
        let result = svc.verify_key("test", "not-valid-base64!!!", &public_key);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn verify_key_rejects_wrong_signature_length() {
        let svc = make_service();
        let public_key = [0u8; 32];
        let short_sig = base64::engine::general_purpose::STANDARD.encode(&[0u8; 16]);
        let result = svc.verify_key("test", &short_sig, &public_key);
        assert!(result.is_err());
    }
}
