use super::models::{
    OlmAccountData, OlmAccountInfo, OlmDecryptedMessage, OlmEncryptedMessage, OlmMessageInfo,
    OlmMessageType, OneTimeKey,
};
use super::session::OlmSessionManager;
use super::storage::OlmStorage;
use crate::cache::CacheManager;
use crate::error::ApiError;
use std::sync::Arc;
use tokio::sync::RwLock;
use vodozemac::olm::{Account, SessionConfig};
use vodozemac::KeyId;

const PICKLE_KEY: [u8; 32] = [0u8; 32];

pub struct OlmService {
    account: RwLock<Option<Account>>,
    storage: OlmStorage,
    session_manager: RwLock<Option<Arc<OlmSessionManager>>>,
    #[allow(dead_code)]
    cache: Arc<CacheManager>,
    user_id: RwLock<Option<String>>,
    device_id: RwLock<Option<String>>,
}

impl OlmService {
    pub fn new(cache: Arc<CacheManager>, storage: OlmStorage) -> Self {
        Self {
            account: RwLock::new(None),
            storage,
            session_manager: RwLock::new(None),
            cache,
            user_id: RwLock::new(None),
            device_id: RwLock::new(None),
        }
    }

    pub async fn initialize(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        {
            let mut uid = self.user_id.write().await;
            *uid = Some(user_id.to_string());
        }
        {
            let mut did = self.device_id.write().await;
            *did = Some(device_id.to_string());
        }

        if let Some(account_data) = self.storage.load_account(user_id, device_id).await? {
            let pickle = vodozemac::olm::AccountPickle::from_encrypted(
                &account_data.serialized_account,
                &PICKLE_KEY,
            )
            .map_err(|e| ApiError::internal(format!("Failed to decode account pickle: {}", e)))?;
            let account = Account::from_pickle(pickle);

            {
                let mut acc = self.account.write().await;
                *acc = Some(account);
            }
        }

        let session_manager = Arc::new(OlmSessionManager::new(
            self.storage.clone(),
            user_id.to_string(),
            device_id.to_string(),
        ));
        session_manager.load_sessions().await?;

        {
            let mut sm = self.session_manager.write().await;
            *sm = Some(session_manager);
        }

        Ok(())
    }

    pub async fn persist(&self) -> Result<(), ApiError> {
        let user_id = self.user_id.read().await;
        let device_id = self.device_id.read().await;

        let (uid, did) = match (user_id.as_ref(), device_id.as_ref()) {
            (Some(u), Some(d)) => (u.clone(), d.clone()),
            _ => return Err(ApiError::internal("OlmService not initialized")),
        };

        let account = self.account.read().await;

        if let Some(ref account) = *account {
            let identity_keys = account.identity_keys();
            let pickle = account.pickle();
            let serialized = pickle.encrypt(&PICKLE_KEY);

            let account_data =
                OlmAccountData::new(uid, did, identity_keys.curve25519.to_base64(), serialized);

            self.storage.save_account(&account_data).await?;
        }

        if let Some(sm) = self.session_manager.read().await.as_ref() {
            sm.persist_sessions().await?;
        }

        Ok(())
    }

    pub async fn generate_one_time_keys(&self, count: usize) {
        let mut account = self.account.write().await;
        if let Some(ref mut account) = *account {
            account.generate_one_time_keys(count);
        }
    }

    pub async fn get_account_info(&self) -> OlmAccountInfo {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            let identity_keys = account.identity_keys();

            let one_time_keys: Vec<String> = account
                .one_time_keys()
                .iter()
                .map(|(id, k): (&KeyId, &vodozemac::Curve25519PublicKey)| {
                    format!("{}:{}", id.to_base64(), k.to_base64())
                })
                .collect();

            let fallback_key = account.fallback_key().iter().next().map(
                |(id, k): (&KeyId, &vodozemac::Curve25519PublicKey)| {
                    format!("{}:{}", id.to_base64(), k.to_base64())
                },
            );

            OlmAccountInfo {
                identity_key: identity_keys.curve25519.to_base64(),
                one_time_keys,
                fallback_key,
            }
        } else {
            OlmAccountInfo {
                identity_key: String::new(),
                one_time_keys: Vec::new(),
                fallback_key: None,
            }
        }
    }

    pub async fn get_one_time_keys(&self) -> Vec<OneTimeKey> {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            account
                .one_time_keys()
                .iter()
                .map(
                    |(id, k): (&KeyId, &vodozemac::Curve25519PublicKey)| OneTimeKey {
                        key_id: id.to_base64(),
                        public_key: k.to_base64(),
                    },
                )
                .collect()
        } else {
            Vec::new()
        }
    }

    pub async fn get_fallback_key(&self) -> Option<OneTimeKey> {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            account.fallback_key().iter().next().map(
                |(id, k): (&KeyId, &vodozemac::Curve25519PublicKey)| OneTimeKey {
                    key_id: id.to_base64(),
                    public_key: k.to_base64(),
                },
            )
        } else {
            None
        }
    }

    pub async fn sign(&self, message: &[u8]) -> String {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            let signature = account.sign(message);
            signature.to_base64()
        } else {
            String::new()
        }
    }

    pub async fn mark_keys_as_published(&self) {
        let mut account = self.account.write().await;
        if let Some(ref mut account) = *account {
            account.mark_keys_as_published();
        }
    }

    pub fn parse_identity_key(key_base64: &str) -> Result<vodozemac::Curve25519PublicKey, String> {
        vodozemac::Curve25519PublicKey::from_base64(key_base64)
            .map_err(|e| format!("Invalid identity key: {}", e))
    }

    pub async fn create_outbound_session(
        &self,
        their_identity_key: &str,
        their_one_time_key: &str,
    ) -> Result<OlmEncryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm
            .as_ref()
            .ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        let identity_key = vodozemac::Curve25519PublicKey::from_base64(their_identity_key)
            .map_err(|e| ApiError::bad_request(format!("Invalid identity key: {}", e)))?;

        let one_time_key = vodozemac::Curve25519PublicKey::from_base64(their_one_time_key)
            .map_err(|e| ApiError::bad_request(format!("Invalid one-time key: {}", e)))?;

        let mut account = self.account.write().await;

        if let Some(ref mut account) = *account {
            session_manager
                .create_outbound_session(account, identity_key, one_time_key)
                .await
        } else {
            Err(ApiError::internal("Account not initialized"))
        }
    }

    pub async fn create_inbound_session(
        &self,
        their_identity_key: &str,
        message: &str,
    ) -> Result<OlmDecryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm
            .as_ref()
            .ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        let identity_key = vodozemac::Curve25519PublicKey::from_base64(their_identity_key)
            .map_err(|e| ApiError::bad_request(format!("Invalid identity key: {}", e)))?;

        let mut account = self.account.write().await;

        if let Some(ref mut account) = *account {
            session_manager
                .create_inbound_session(account, identity_key, message)
                .await
        } else {
            Err(ApiError::internal("Account not initialized"))
        }
    }

    pub async fn encrypt(
        &self,
        session_id: &str,
        plaintext: &str,
    ) -> Result<OlmEncryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm
            .as_ref()
            .ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.encrypt(session_id, plaintext).await
    }

    pub async fn decrypt(
        &self,
        session_id: &str,
        message_type: OlmMessageType,
        ciphertext: &str,
    ) -> Result<OlmDecryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm
            .as_ref()
            .ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager
            .decrypt(session_id, message_type, ciphertext)
            .await
    }

    pub async fn get_session_for_sender(&self, sender_key: &str) -> Option<String> {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.get_session_for_sender(sender_key).await
        } else {
            None
        }
    }

    pub async fn session_exists(&self, session_id: &str) -> bool {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.session_exists(session_id).await
        } else {
            false
        }
    }

    pub async fn remove_session(&self, session_id: &str) -> Result<(), ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm
            .as_ref()
            .ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.remove_session(session_id).await
    }

    pub async fn get_session_count(&self) -> usize {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.get_session_count().await
        } else {
            0
        }
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.list_sessions().await
        } else {
            Vec::new()
        }
    }

    pub async fn clear_expired_sessions(&self) -> Result<u64, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm
            .as_ref()
            .ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.clear_expired_sessions().await
    }

    pub async fn get_identity_key(&self) -> String {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            let identity_keys = account.identity_keys();
            identity_keys.curve25519.to_base64()
        } else {
            String::new()
        }
    }

    pub async fn create_outbound_session_legacy(
        &self,
        their_identity_key: &vodozemac::Curve25519PublicKey,
    ) -> Result<OlmMessageInfo, String> {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            let session_config = SessionConfig::version_2();

            let one_time_keys_map = account.one_time_keys();
            let one_time_keys: Vec<_> = one_time_keys_map.iter().collect();
            if one_time_keys.is_empty() {
                return Err("No one-time keys available".to_string());
            }

            let one_time_key = one_time_keys[0].1;

            let mut session =
                account.create_outbound_session(session_config, *their_identity_key, *one_time_key);

            let message = session.encrypt(b"");
            let message_type = if message.message_type() == vodozemac::olm::MessageType::PreKey {
                OlmMessageType::PreKey
            } else {
                OlmMessageType::Message
            };

            let ciphertext = match &message {
                vodozemac::olm::OlmMessage::PreKey(m) => m.to_base64(),
                vodozemac::olm::OlmMessage::Normal(m) => m.to_base64(),
            };

            Ok(OlmMessageInfo {
                message_type,
                ciphertext,
            })
        } else {
            Err("Account not initialized".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheConfig;

    fn create_test_cache() -> Arc<CacheManager> {
        let config = CacheConfig::default();
        Arc::new(CacheManager::new(config))
    }

    #[test]
    fn test_olm_account_info() {
        let _cache = create_test_cache();
        let account = Account::new();
        let identity_keys = account.identity_keys();

        assert!(!identity_keys.curve25519.to_base64().is_empty());
    }

    #[test]
    fn test_generate_one_time_keys_count() {
        let mut account = Account::new();
        account.generate_one_time_keys(5);

        let keys_map = account.one_time_keys();
        let keys: Vec<_> = keys_map.iter().collect();
        assert_eq!(keys.len(), 5);
    }

    #[test]
    fn test_sign_message() {
        let account = Account::new();
        let message = b"Test message";
        let signature = account.sign(message);

        assert!(!signature.to_base64().is_empty());
    }

    #[test]
    fn test_parse_identity_key() {
        let account = Account::new();
        let key = account.curve25519_key().to_base64();
        let result = OlmService::parse_identity_key(&key);
        assert!(result.is_ok());
    }

    #[test]
    fn test_one_time_key_structure() {
        let key = OneTimeKey {
            key_id: "key_123".to_string(),
            public_key: "public_key_data".to_string(),
        };

        assert_eq!(key.key_id, "key_123");
        assert_eq!(key.public_key, "public_key_data");
    }
}
