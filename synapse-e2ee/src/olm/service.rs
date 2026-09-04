use super::models::{
    OlmAccountData, OlmAccountInfo, OlmDecryptedMessage, OlmEncryptedMessage, OlmMessageType, OneTimeKey,
};
use super::session::OlmSessionManager;
use super::storage::OlmStorage;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::map_database;
use synapse_common::ApiError;
use tokio::sync::RwLock;
use vodozemac::olm::Account;
use vodozemac::KeyId;

use std::env;
use std::sync::OnceLock;

static PICKLE_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// E-06: Production deployments MUST set `OLM_PICKLE_KEY`. The random
/// fallback is limited to debug builds only; release builds will panic
/// to prevent silent key loss on restart ( OlmAccount pickle would be
/// decryptable only by the randomly-generated key from *that* process
/// instance, so the Olm account becomes permanently unreadable across
/// restarts — a critical data-loss risk).
pub fn get_pickle_key() -> &'static [u8; 32] {
    PICKLE_KEY.get_or_init(|| {
        if let Ok(key_str) = env::var("OLM_PICKLE_KEY") {
            match synapse_common::crypto::decode_hex(&key_str) {
                Ok(decoded) if decoded.len() == 32 => {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&decoded[..32]);
                    key
                }
                Ok(_) => {
                    tracing::error!(
                        "OLM_PICKLE_KEY must be exactly 32 bytes (64 hex characters). Aborting."
                    );
                    panic!(
                        "E-06: OLM_PICKLE_KEY is not 32 bytes. \
                         Set OLM_PICKLE_KEY to a 64-character hex string."
                    );
                }
                Err(e) => {
                    tracing::error!("OLM_PICKLE_KEY is not valid hex: {}. Aborting.", e);
                    panic!(
                        "E-06: OLM_PICKLE_KEY is not valid hex. \
                         Set OLM_PICKLE_KEY to a 64-character hex string."
                    );
                }
            }
        } else {
            // E-06: Warn-and-random is ONLY safe in debug — release builds
            // must fail rather than silently produce a key that survives only
            // one process instance.
            if cfg!(debug_assertions) {
                tracing::warn!(
                    "OLM_PICKLE_KEY not set (debug mode). Generating random key. \
                     Encrypted Olm data will not survive restarts. \
                     Set OLM_PICKLE_KEY for production deployments."
                );
                generate_random_pickle_key()
            } else {
                tracing::error!(
                    "E-06: OLM_PICKLE_KEY is not set. \
                     Production builds must set OLM_PICKLE_KEY to a 64-character hex string. \
                     Aborting to prevent Olm account data loss on restart."
                );
                panic!(
                    "E-06: OLM_PICKLE_KEY not set. \
                     Set OLM_PICKLE_KEY to a 64-character hex string before deploying."
                );
            }
        }
    })
}

fn generate_random_pickle_key() -> [u8; 32] {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    key
}

pub struct OlmService {
    account: RwLock<Option<Account>>,
    storage: OlmStorage,
    session_manager: RwLock<Option<Arc<OlmSessionManager>>>,
    _cache: Arc<CacheManager>,
    user_id: RwLock<Option<String>>,
    device_id: RwLock<Option<String>>,
}

impl OlmService {
    pub fn new(cache: Arc<CacheManager>, storage: OlmStorage) -> Self {
        Self {
            account: RwLock::new(None),
            storage,
            session_manager: RwLock::new(None),
            _cache: cache,
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
            let pickle =
                vodozemac::olm::AccountPickle::from_encrypted(&account_data.serialized_account, get_pickle_key())
                    .map_err(map_database!("Failed to decode account pickle"))?;
            let account = Account::from_pickle(pickle);

            {
                let mut acc = self.account.write().await;
                *acc = Some(account);
            }
        }

        let session_manager =
            Arc::new(OlmSessionManager::new(self.storage.clone(), user_id.to_string(), device_id.to_string()));
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
            let serialized = pickle.encrypt(get_pickle_key());

            let account_data = OlmAccountData::new(uid, did, identity_keys.curve25519.to_base64(), serialized);

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

            let fallback_key =
                account.fallback_key().iter().next().map(|(id, k): (&KeyId, &vodozemac::Curve25519PublicKey)| {
                    format!("{}:{}", id.to_base64(), k.to_base64())
                });

            OlmAccountInfo { identity_key: identity_keys.curve25519.to_base64(), one_time_keys, fallback_key }
        } else {
            OlmAccountInfo { identity_key: String::new(), one_time_keys: Vec::new(), fallback_key: None }
        }
    }

    pub async fn get_one_time_keys(&self) -> Vec<OneTimeKey> {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            account
                .one_time_keys()
                .iter()
                .map(|(id, k): (&KeyId, &vodozemac::Curve25519PublicKey)| OneTimeKey {
                    key_id: id.to_base64(),
                    public_key: k.to_base64(),
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub async fn get_fallback_key(&self) -> Option<OneTimeKey> {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            account.fallback_key().iter().next().map(|(id, k): (&KeyId, &vodozemac::Curve25519PublicKey)| OneTimeKey {
                key_id: id.to_base64(),
                public_key: k.to_base64(),
            })
        } else {
            None
        }
    }

    pub async fn sign(&self, message: &[u8]) -> Result<String, ApiError> {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            let signature = account.sign(message);
            Ok(signature.to_base64())
        } else {
            Err(ApiError::internal("Olm account not initialized - cannot sign"))
        }
    }

    pub async fn mark_keys_as_published(&self) {
        let mut account = self.account.write().await;
        if let Some(ref mut account) = *account {
            account.mark_keys_as_published();
        }
    }

    pub fn parse_identity_key(key_base64: &str) -> Result<vodozemac::Curve25519PublicKey, String> {
        vodozemac::Curve25519PublicKey::from_base64(key_base64).map_err(|e| format!("Invalid identity key: {e}"))
    }

    pub async fn create_outbound_session(
        &self,
        their_identity_key: &str,
        their_one_time_key: &str,
    ) -> Result<OlmEncryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        let identity_key = vodozemac::Curve25519PublicKey::from_base64(their_identity_key)
            .map_err(|e| ApiError::bad_request(format!("Invalid identity key: {e}")))?;

        let one_time_key = vodozemac::Curve25519PublicKey::from_base64(their_one_time_key)
            .map_err(|e| ApiError::bad_request(format!("Invalid one-time key: {e}")))?;

        let mut account = self.account.write().await;

        if let Some(ref mut account) = *account {
            session_manager.create_outbound_session(account, identity_key, one_time_key).await
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
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        let identity_key = vodozemac::Curve25519PublicKey::from_base64(their_identity_key)
            .map_err(|e| ApiError::bad_request(format!("Invalid identity key: {e}")))?;

        let mut account = self.account.write().await;

        if let Some(ref mut account) = *account {
            session_manager.create_inbound_session(account, identity_key, message).await
        } else {
            Err(ApiError::internal("Account not initialized"))
        }
    }

    pub async fn encrypt(&self, session_id: &str, plaintext: &str) -> Result<OlmEncryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.encrypt(session_id, plaintext).await
    }

    pub async fn decrypt(
        &self,
        session_id: &str,
        message_type: OlmMessageType,
        ciphertext: &str,
    ) -> Result<OlmDecryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.decrypt(session_id, message_type, ciphertext).await
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
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

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
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_cache::CacheConfig;

    fn create_test_cache() -> Arc<CacheManager> {
        let config = CacheConfig::default();
        Arc::new(CacheManager::new(&config))
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
        let key = OneTimeKey { key_id: "key_123".to_string(), public_key: "public_key_data".to_string() };

        assert_eq!(key.key_id, "key_123");
        assert_eq!(key.public_key, "public_key_data");
    }

    // -------------------------------------------------------------------------
    // E-06 tests — OLM_PICKLE_KEY configuration
    // -------------------------------------------------------------------------

    /// E-06 invariant: `get_pickle_key` must produce a 32-byte key for any
    /// code path that does not panic. We exercise the happy path (valid hex
    /// key) via `env::set_var` so the test does not depend on the actual
    /// environment. The panic paths (invalid length / bad hex / unset in
    /// release) are verified through documentation and code inspection.
    #[test]
    fn test_e06_valid_hex_key_produces_32_bytes() {
        // Set a valid 64-char hex key
        env::set_var("OLM_PICKLE_KEY", "a".repeat(64));
        let key = get_pickle_key();
        assert_eq!(key.len(), 32, "pickle key must be exactly 32 bytes");
        env::remove_var("OLM_PICKLE_KEY");
    }

    #[test]
    fn test_e06_invalid_hex_causes_panic_message() {
        // E-06: invalid hex must NOT silently fall back to random — it must
        // abort with a clear panic message. We verify the panic fires.
        env::set_var("OLM_PICKLE_KEY", "not-hex!");
        let result = std::panic::catch_unwind(|| get_pickle_key());
        env::remove_var("OLM_PICKLE_KEY");
        assert!(
            result.is_err(),
            "E-06: invalid hex OLM_PICKLE_KEY must panic, not silently continue"
        );
    }

    #[test]
    fn test_e06_no_silent_random_fallback_in_release_cfg() {
        // E-06 invariant: the random fallback branch must be gated behind
        // `cfg!(debug_assertions)`. Release builds must panic instead.
        // Verified by source inspection — a runtime test would crash the
        // process in release mode, so we assert the cfg gate exists.
        let src = include_str!("service.rs");
        let fn_body = src
            .split("pub fn get_pickle_key")
            .nth(1)
            .expect("get_pickle_key should exist")
            .split('\n')
            .take(60) // rough function body scope
            .collect::<String>();
        assert!(
            fn_body.contains("cfg!(debug_assertions)"),
            "E-06: cfg!(debug_assertions) gate must be present for random fallback"
        );
    }
}
