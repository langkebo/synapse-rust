use super::models::{OlmDecryptedMessage, OlmEncryptedMessage, OlmMessageType, OlmSessionData};
use super::storage::OlmStorage;
use crate::error::ApiError;
use base64::Engine;
use std::collections::HashMap;
use tokio::sync::RwLock;
use vodozemac::olm::{Account, Session, SessionConfig};

const PICKLE_KEY: [u8; 32] = [0u8; 32];

pub struct OlmSessionManager {
    storage: OlmStorage,
    sessions: RwLock<HashMap<String, OlmSessionEntry>>,
    user_id: String,
    device_id: String,
}

struct OlmSessionEntry {
    session: Session,
    #[allow(dead_code)]
    session_id: String,
    sender_key: String,
    dirty: bool,
}

impl OlmSessionManager {
    pub fn new(storage: OlmStorage, user_id: String, device_id: String) -> Self {
        Self {
            storage,
            sessions: RwLock::new(HashMap::new()),
            user_id,
            device_id,
        }
    }

    pub async fn load_sessions(&self) -> Result<(), ApiError> {
        let session_data = self
            .storage
            .load_sessions(&self.user_id, &self.device_id)
            .await?;

        let mut sessions = self.sessions.write().await;
        for data in session_data {
            if data.is_expired() {
                continue;
            }

            match vodozemac::olm::SessionPickle::from_encrypted(&data.serialized_state, &PICKLE_KEY)
            {
                Ok(pickle) => {
                    let session = Session::from_pickle(pickle);
                    let session_id = session.session_id();
                    sessions.insert(
                        session_id.clone(),
                        OlmSessionEntry {
                            session,
                            session_id: data.session_id,
                            sender_key: data.sender_key,
                            dirty: false,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to import session {}: {}", data.session_id, e);
                    if let Err(e) = self.storage.delete_session(&data.session_id).await {
                        tracing::error!("Failed to delete corrupted session: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn persist_sessions(&self) -> Result<(), ApiError> {
        let sessions = self.sessions.read().await;

        for (session_id, entry) in sessions.iter() {
            if entry.dirty {
                let pickle = entry.session.pickle();
                let serialized = pickle.encrypt(&PICKLE_KEY);
                let mut session_data = OlmSessionData::new(
                    session_id.clone(),
                    self.user_id.clone(),
                    self.device_id.clone(),
                    entry.sender_key.clone(),
                    String::new(),
                    serialized,
                );
                session_data.touch();

                self.storage.save_session(&session_data).await?;
            }
        }

        Ok(())
    }

    pub async fn create_outbound_session(
        &self,
        account: &mut Account,
        their_identity_key: vodozemac::Curve25519PublicKey,
        their_one_time_key: vodozemac::Curve25519PublicKey,
    ) -> Result<OlmEncryptedMessage, ApiError> {
        let session_config = SessionConfig::version_2();

        let mut session =
            account.create_outbound_session(session_config, their_identity_key, their_one_time_key);

        let session_id = session.session_id();

        let message = session.encrypt(b"");
        let ciphertext = match &message {
            vodozemac::olm::OlmMessage::PreKey(m) => m.to_base64(),
            vodozemac::olm::OlmMessage::Normal(m) => m.to_base64(),
        };

        let entry = OlmSessionEntry {
            session,
            session_id: session_id.clone(),
            sender_key: their_identity_key.to_base64(),
            dirty: true,
        };

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), entry);
        }

        Ok(OlmEncryptedMessage {
            session_id,
            message_type: OlmMessageType::PreKey,
            ciphertext,
        })
    }

    pub async fn create_inbound_session(
        &self,
        account: &mut Account,
        their_identity_key: vodozemac::Curve25519PublicKey,
        message: &str,
    ) -> Result<OlmDecryptedMessage, ApiError> {
        let pre_key_message = vodozemac::olm::PreKeyMessage::from_base64(message)
            .map_err(|e| ApiError::bad_request(format!("Invalid pre-key message: {}", e)))?;

        let result = account
            .create_inbound_session(their_identity_key, &pre_key_message)
            .map_err(|e| ApiError::internal(format!("Failed to create inbound session: {}", e)))?;

        let session_id = result.session.session_id();

        let entry = OlmSessionEntry {
            session: result.session,
            session_id: session_id.clone(),
            sender_key: their_identity_key.to_base64(),
            dirty: true,
        };

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), entry);
        }

        let plaintext = String::from_utf8_lossy(&result.plaintext).to_string();

        Ok(OlmDecryptedMessage {
            plaintext,
            session_id,
        })
    }

    pub async fn encrypt(
        &self,
        session_id: &str,
        plaintext: &str,
    ) -> Result<OlmEncryptedMessage, ApiError> {
        let mut sessions = self.sessions.write().await;

        let entry = sessions
            .get_mut(session_id)
            .ok_or_else(|| ApiError::not_found(format!("Session not found: {}", session_id)))?;

        let message = entry.session.encrypt(plaintext.as_bytes());

        entry.dirty = true;

        let message_type = if message.message_type() == vodozemac::olm::MessageType::PreKey {
            OlmMessageType::PreKey
        } else {
            OlmMessageType::Message
        };

        let ciphertext = match &message {
            vodozemac::olm::OlmMessage::PreKey(m) => m.to_base64(),
            vodozemac::olm::OlmMessage::Normal(m) => m.to_base64(),
        };

        Ok(OlmEncryptedMessage {
            session_id: session_id.to_string(),
            message_type,
            ciphertext,
        })
    }

    pub async fn decrypt(
        &self,
        session_id: &str,
        message_type: OlmMessageType,
        ciphertext: &str,
    ) -> Result<OlmDecryptedMessage, ApiError> {
        let mut sessions = self.sessions.write().await;

        let entry = sessions
            .get_mut(session_id)
            .ok_or_else(|| ApiError::not_found(format!("Session not found: {}", session_id)))?;

        let raw_ciphertext = base64::engine::general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|e| ApiError::bad_request(format!("Invalid base64: {}", e)))?;

        let msg_type = match message_type {
            OlmMessageType::PreKey => 0usize,
            OlmMessageType::Message => 1usize,
        };

        let message = vodozemac::olm::OlmMessage::from_parts(msg_type, &raw_ciphertext)
            .map_err(|e| ApiError::bad_request(format!("Invalid message: {}", e)))?;

        let plaintext = entry
            .session
            .decrypt(&message)
            .map_err(|e| ApiError::internal(format!("Failed to decrypt: {}", e)))?;

        entry.dirty = true;

        let plaintext_str = String::from_utf8_lossy(&plaintext).to_string();

        Ok(OlmDecryptedMessage {
            plaintext: plaintext_str,
            session_id: session_id.to_string(),
        })
    }

    pub async fn get_session(&self, session_id: &str) -> Option<String> {
        let sessions = self.sessions.read().await;
        if sessions.contains_key(session_id) {
            Some(session_id.to_string())
        } else {
            None
        }
    }

    pub async fn get_session_for_sender(&self, sender_key: &str) -> Option<String> {
        let sessions = self.sessions.read().await;
        for (session_id, entry) in sessions.iter() {
            if entry.sender_key == sender_key {
                return Some(session_id.clone());
            }
        }
        None
    }

    pub async fn session_exists(&self, session_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(session_id)
    }

    pub async fn remove_session(&self, session_id: &str) -> Result<(), ApiError> {
        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(session_id);
        }

        self.storage.delete_session(session_id).await?;

        Ok(())
    }

    pub async fn get_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    pub async fn clear_expired_sessions(&self) -> Result<u64, ApiError> {
        let deleted = self.storage.delete_expired_sessions().await?;

        let expired_session_ids: Vec<String> = {
            let sessions = self.sessions.read().await;
            let mut expired = Vec::new();
            for (id, _entry) in sessions.iter() {
                if let Ok(Some(session_data)) = self.storage.load_session(id).await {
                    if session_data.is_expired() {
                        expired.push(id.clone());
                    }
                }
            }
            expired
        };

        {
            let mut sessions = self.sessions.write().await;
            for id in expired_session_ids {
                sessions.remove(&id);
            }
        }

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_olm_session_data_creation() {
        let session_data = OlmSessionData::new(
            "session_123".to_string(),
            "@user:example.com".to_string(),
            "DEVICE".to_string(),
            "sender_key".to_string(),
            "receiver_key".to_string(),
            "state".to_string(),
        );

        assert_eq!(session_data.session_id, "session_123");
        assert_eq!(session_data.message_index, 0);
        assert!(!session_data.is_expired());
    }

    #[test]
    fn test_olm_session_data_touch() {
        let mut session_data = OlmSessionData::new(
            "session_123".to_string(),
            "@user:example.com".to_string(),
            "DEVICE".to_string(),
            "sender_key".to_string(),
            "receiver_key".to_string(),
            "state".to_string(),
        );

        let original_last_used = session_data.last_used_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        session_data.touch();

        assert!(session_data.last_used_at > original_last_used);
    }

    #[test]
    fn test_olm_session_data_expiration() {
        let mut session_data = OlmSessionData::new(
            "session_123".to_string(),
            "@user:example.com".to_string(),
            "DEVICE".to_string(),
            "sender_key".to_string(),
            "receiver_key".to_string(),
            "state".to_string(),
        );

        assert!(!session_data.is_expired());

        session_data.expires_at = Some(chrono::Utc::now().timestamp_millis() - 1000);
        assert!(session_data.is_expired());

        session_data.expires_at = Some(chrono::Utc::now().timestamp_millis() + 100000);
        assert!(!session_data.is_expired());
    }
}
