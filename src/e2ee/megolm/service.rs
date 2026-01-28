use super::models::*;
use super::storage::MegolmSessionStorage;
use crate::e2ee::crypto::aes::{Aes256GcmCipher, Aes256GcmKey};
use crate::cache::CacheManager;
use std::sync::Arc;
use crate::error::ApiError;

pub struct MegolmService {
    storage: MegolmSessionStorage<'static>,
    cache: Arc<CacheManager>,
    encryption_key: [u8; 32],
}

impl MegolmService {
    pub fn new(storage: MegolmSessionStorage<'static>, cache: Arc<CacheManager>, encryption_key: [u8; 32]) -> Self {
        Self { storage, cache, encryption_key }
    }
    
    pub async fn create_session(&self, room_id: &str, sender_key: &str) -> Result<MegolmSession, ApiError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        
        let session_key = Aes256GcmKey::generate();
        let encrypted_key = self.encrypt_session_key(&session_key)?;
        
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.clone(),
            room_id: room_id.to_string(),
            sender_key: sender_key.to_string(),
            session_key: encrypted_key,
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_at: Utc::now(),
            last_used_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(7)),
        };
        
        self.storage.create_session(&session).await?;
        
        let cache_key = format!("megolm_session:{}", session_id);
        self.cache.set(&cache_key, &session, 600).await;
        
        Ok(session)
    }
    
    pub async fn load_session(&self, session_id: &str) -> Result<MegolmSession, ApiError> {
        let cache_key = format!("megolm_session:{}", session_id);
        if let Some(session) = self.cache.get::<MegolmSession>(&cache_key).await {
            return Ok(session);
        }
        
        let session = self.storage.get_session(session_id).await?
            .ok_or_else(|| ApiError::NotFound("Session not found".to_string()))?;
        
        self.cache.set(&cache_key, &session, 600).await;
        
        Ok(session)
    }
    
    pub async fn encrypt(&self, session_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;
        
        let cipher_key = Aes256GcmKey::from_bytes(session_key);
        let encrypted = Aes256GcmCipher::encrypt(&cipher_key, plaintext)?;
        
        let mut updated_session = session.clone();
        updated_session.message_index += 1;
        updated_session.last_used_at = Utc::now();
        self.storage.update_session(&updated_session).await?;
        
        Ok(encrypted)
    }
    
    pub async fn decrypt(&self, session_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;
        
        let cipher_key = Aes256GcmKey::from_bytes(session_key);
        let decrypted = Aes256GcmCipher::decrypt(&cipher_key, ciphertext)?;
        
        Ok(decrypted)
    }
    
    pub async fn rotate_session(&self, session_id: &str) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;
        
        self.storage.delete_session(session_id).await?;
        
        self.create_session(&session.room_id, &session.sender_key).await?;
        
        Ok(())
    }
    
    pub async fn share_session(&self, session_id: &str, user_ids: &[String]) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;
        
        for user_id in user_ids {
        let cache_key = format!("megolm_session_key:{}:{}", user_id, session_id);
            self.cache.set(&cache_key, &session_key, 600).await;
        }
        
        Ok(())
    }
    
    fn encrypt_session_key(&self, key: &Aes256GcmKey) -> Result<String, ApiError> {
        let cipher_key = Aes256GcmKey::from_bytes(self.encryption_key);
        let encrypted = Aes256GcmCipher::encrypt(&cipher_key, key.as_bytes())?;
        Ok(base64::encode(&encrypted))
    }
    
    fn decrypt_session_key(&self, encrypted: &str) -> Result<[u8; 32], ApiError> {
        let encrypted_bytes = base64::decode(encrypted)
            .map_err(|_| ApiError::DecryptionError("Invalid base64".to_string()))?;
        let cipher_key = Aes256GcmKey::from_bytes(self.encryption_key);
        let decrypted = Aes256GcmCipher::decrypt(&cipher_key, &encrypted_bytes)?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&decrypted);
        Ok(key)
    }
}