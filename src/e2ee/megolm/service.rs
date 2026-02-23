use super::models::*;
use super::storage::MegolmSessionStorage;
use crate::cache::CacheManager;
use crate::e2ee::crypto::aes::{
    Aes256GcmCipher, Aes256GcmCiphertext, Aes256GcmKey, Aes256GcmNonce,
};
use crate::e2ee::crypto::CryptoError;
use crate::error::ApiError;
use chrono::Utc;
use std::sync::Arc;

#[derive(Clone)]
pub struct MegolmService {
    storage: MegolmSessionStorage,
    cache: Arc<CacheManager>,
    encryption_key: [u8; 32],
}

impl MegolmService {
    pub fn new(
        storage: MegolmSessionStorage,
        cache: Arc<CacheManager>,
        encryption_key: [u8; 32],
    ) -> Self {
        Self {
            storage,
            cache,
            encryption_key,
        }
    }

    pub async fn create_session(
        &self,
        room_id: &str,
        sender_key: &str,
    ) -> Result<MegolmSession, ApiError> {
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
        let _ = self.cache.set(&cache_key, &session, 600).await;

        Ok(session)
    }

    pub async fn load_session(&self, session_id: &str) -> Result<MegolmSession, ApiError> {
        let cache_key = format!("megolm_session:{}", session_id);
        if let Ok(Some(session)) = self.cache.get::<MegolmSession>(&cache_key).await {
            return Ok(session);
        }

        // Cache miss - load from storage
        let session = self
            .storage
            .get_session(session_id)
            .await?
            .ok_or_else(|| ApiError::NotFound("Session not found".to_string()))?;

        // Update cache with the loaded session
        let _ = self.cache.set(&cache_key, &session, 600).await;

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

    pub async fn decrypt(
        &self,
        session_id: &str,
        ciphertext: &[u8],
        nonce: &[u8],
    ) -> Result<Vec<u8>, ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;

        let cipher_key = Aes256GcmKey::from_bytes(session_key);
        let nonce_obj = Aes256GcmNonce::from_bytes(nonce)
            .map_err(|_| ApiError::DecryptionError("Invalid nonce length".to_string()))?;
        let decrypted = Aes256GcmCipher::decrypt(&cipher_key, &nonce_obj, ciphertext)?;

        Ok(decrypted)
    }

    pub async fn rotate_session(&self, session_id: &str) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;

        self.storage.delete_session(session_id).await?;

        self.create_session(&session.room_id, &session.sender_key)
            .await?;

        Ok(())
    }

    pub async fn share_session(
        &self,
        session_id: &str,
        user_ids: &[String],
    ) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;

        for user_id in user_ids {
            let cache_key = format!("megolm_session_key:{}:{}", user_id, session_id);
            let _ = self.cache.set(&cache_key, &session_key, 600).await;
        }

        Ok(())
    }

    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        self.storage
            .get_room_sessions(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room sessions: {}", e)))
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.storage
            .delete_session(session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete session: {}", e)))
    }

    fn encrypt_session_key(&self, key: &Aes256GcmKey) -> Result<String, ApiError> {
        let cipher_key = Aes256GcmKey::from_bytes(self.encryption_key);
        let encrypted = Aes256GcmCipher::encrypt(&cipher_key, &key.as_bytes()[..])?;
        let json = serde_json::to_string(&encrypted)
            .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            json.as_bytes(),
        ))
    }

    fn decrypt_session_key(&self, encrypted: &str) -> Result<[u8; 32], ApiError> {
        let json_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted)
                .map_err(|_| ApiError::DecryptionError("Invalid base64".to_string()))?;
        let json_str = String::from_utf8(json_bytes)
            .map_err(|_| ApiError::DecryptionError("Invalid UTF-8".to_string()))?;
        let ciphertext: Aes256GcmCiphertext = serde_json::from_str(&json_str)
            .map_err(|e| ApiError::DecryptionError(e.to_string()))?;
        let cipher_key = Aes256GcmKey::from_bytes(self.encryption_key);
        let decrypted =
            Aes256GcmCipher::decrypt(&cipher_key, ciphertext.nonce(), ciphertext.ciphertext())?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&decrypted);
        Ok(key)
    }

    pub async fn get_outbound_session(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        self.get_room_key_distribution(room_id).await
    }

    pub async fn get_room_key_distribution(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        let sessions = self.get_room_sessions(room_id).await?;

        if let Some(session) = sessions.first() {
            let session_key = self.decrypt_session_key(&session.session_key)?;

            Ok(Some(RoomKeyDistributionData {
                session_id: session.session_id.clone(),
                session_key: base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    session_key,
                ),
                algorithm: session.algorithm.clone(),
                room_id: room_id.to_string(),
            }))
        } else {
            Ok(None)
        }
    }
}
