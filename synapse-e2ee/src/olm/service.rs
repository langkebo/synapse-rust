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
static PICKLE_KEY_ERROR: OnceLock<String> = OnceLock::new();

/// Pure helper: decode a hex `OLM_PICKLE_KEY` value into a 32-byte key.
/// Returns `Ok([u8; 32])` for a 64-character hex string, otherwise an
/// error message describing the failure.
///
/// E-06: extracted from `get_pickle_key_strict` so it can be unit-tested
/// without `OnceLock` pollution between tests. The cached wrappers below
/// use this function to do the actual validation.
pub fn decode_pickle_key_from_env(value: Option<&str>) -> Result<[u8; 32], String> {
    let key_str = value.ok_or_else(|| {
        "E-06: OLM_PICKLE_KEY is not set. \
         Set it to a 64-character hex string (32 bytes) before starting the server."
            .to_string()
    })?;
    let decoded = synapse_common::crypto::decode_hex(key_str)
        .map_err(|e| format!("E-06: OLM_PICKLE_KEY is not valid hex: {e}"))?;
    if decoded.len() != 32 {
        return Err(format!("E-06: OLM_PICKLE_KEY is {} bytes, must be exactly 32 (64 hex characters)", decoded.len()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded[..32]);
    Ok(key)
}

/// E-06: result-style pickle key lookup. Returns `Ok(key)` when
/// `OLM_PICKLE_KEY` is set to a 64-character hex string, otherwise
/// `Err(message)` carrying a clear remediation message.
///
/// Production callers (e.g. `OlmService::initialize`) should call this
/// at startup and refuse to start the service if the key is missing or
/// malformed — every restart would otherwise invalidate every
/// persisted Olm account.
pub fn get_pickle_key_strict() -> Result<&'static [u8; 32], ApiError> {
    if let Some(key) = PICKLE_KEY.get() {
        return Ok(key);
    }
    if let Some(err_msg) = PICKLE_KEY_ERROR.get() {
        return Err(ApiError::internal(err_msg.clone()));
    }
    let value = env::var("OLM_PICKLE_KEY").ok();
    match decode_pickle_key_from_env(value.as_deref()) {
        Ok(key) => Ok(PICKLE_KEY.get_or_init(|| key)),
        Err(msg) => {
            let _ = PICKLE_KEY_ERROR.set(msg.clone());
            Err(ApiError::internal(msg))
        }
    }
}

/// The `OlmService` type.
pub struct OlmService {
    account: RwLock<Option<Account>>,
    storage: OlmStorage,
    session_manager: RwLock<Option<Arc<OlmSessionManager>>>,
    _cache: Arc<CacheManager>,
    user_id: RwLock<Option<String>>,
    device_id: RwLock<Option<String>>,
}

/// (see code)
impl OlmService {
    /// See [`new`].
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

    /// See [`initialize`].
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
            // E-06: use the strict pickle-key lookup. In production this
            // must not silently fall back to a random key — startup must
            // fail loudly if OLM_PICKLE_KEY is missing or malformed.
            let pickle_key = get_pickle_key_strict()?;
            let pickle = vodozemac::olm::AccountPickle::from_encrypted(&account_data.serialized_account, pickle_key)
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

    /// See [`persist`].
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
            // E-06: use the strict pickle-key lookup. A missing or malformed
            // OLM_PICKLE_KEY must abort the persist, not silently encrypt
            // with a random per-process key.
            let pickle_key = get_pickle_key_strict()?;
            let serialized = pickle.encrypt(pickle_key);

            let account_data = OlmAccountData::new(uid, did, identity_keys.curve25519.to_base64(), serialized);

            self.storage.save_account(&account_data).await?;
        }

        if let Some(sm) = self.session_manager.read().await.as_ref() {
            sm.persist_sessions().await?;
        }

        Ok(())
    }

    /// See [`generate_one_time_keys`].
    pub async fn generate_one_time_keys(&self, count: usize) {
        let mut account = self.account.write().await;
        if let Some(ref mut account) = *account {
            account.generate_one_time_keys(count);
        }
    }

    /// See [`get_account_info`].
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

    /// See [`get_one_time_keys`].
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

    /// See [`get_fallback_key`].
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

    /// See [`sign`].
    pub async fn sign(&self, message: &[u8]) -> Result<String, ApiError> {
        let account = self.account.read().await;

        if let Some(ref account) = *account {
            let signature = account.sign(message);
            Ok(signature.to_base64())
        } else {
            Err(ApiError::internal("Olm account not initialized - cannot sign"))
        }
    }

    /// See [`mark_keys_as_published`].
    pub async fn mark_keys_as_published(&self) {
        let mut account = self.account.write().await;
        if let Some(ref mut account) = *account {
            account.mark_keys_as_published();
        }
    }

    /// See [`parse_identity_key`].
    pub fn parse_identity_key(key_base64: &str) -> Result<vodozemac::Curve25519PublicKey, String> {
        vodozemac::Curve25519PublicKey::from_base64(key_base64).map_err(|e| format!("Invalid identity key: {e}"))
    }

    /// See [`create_outbound_session`].
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

    /// See [`create_inbound_session`].
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

    /// See [`encrypt`].
    pub async fn encrypt(&self, session_id: &str, plaintext: &str) -> Result<OlmEncryptedMessage, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.encrypt(session_id, plaintext).await
    }

    /// See [`decrypt`].
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

    /// See [`get_session_for_sender`].
    pub async fn get_session_for_sender(&self, sender_key: &str) -> Option<String> {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.get_session_for_sender(sender_key).await
        } else {
            None
        }
    }

    /// See [`session_exists`].
    pub async fn session_exists(&self, session_id: &str) -> bool {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.session_exists(session_id).await
        } else {
            false
        }
    }

    /// See [`remove_session`].
    pub async fn remove_session(&self, session_id: &str) -> Result<(), ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.remove_session(session_id).await
    }

    /// See [`get_session_count`].
    pub async fn get_session_count(&self) -> usize {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.get_session_count().await
        } else {
            0
        }
    }

    /// See [`list_sessions`].
    pub async fn list_sessions(&self) -> Vec<String> {
        let sm = self.session_manager.read().await;
        if let Some(session_manager) = sm.as_ref() {
            session_manager.list_sessions().await
        } else {
            Vec::new()
        }
    }

    /// See [`clear_expired_sessions`].
    pub async fn clear_expired_sessions(&self) -> Result<u64, ApiError> {
        let sm = self.session_manager.read().await;
        let session_manager = sm.as_ref().ok_or_else(|| ApiError::internal("OlmService not initialized"))?;

        session_manager.clear_expired_sessions().await
    }

    /// See [`get_identity_key`].
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

    /// E-06 invariant: `decode_pickle_key_from_env` (the pure decode path)
    /// returns Ok for a valid 64-char hex string.
    #[test]
    fn test_e06_valid_hex_key_produces_32_bytes() {
        let key = decode_pickle_key_from_env(Some(&"a".repeat(64))).expect("valid 64-char hex must succeed");
        assert_eq!(key.len(), 32, "pickle key must be exactly 32 bytes");
    }

    /// E-06: invalid hex must be rejected with a descriptive error.
    #[test]
    fn test_e06_invalid_hex_returns_error() {
        let result = decode_pickle_key_from_env(Some("not-hex!"));
        assert!(result.is_err(), "E-06: invalid hex must return Err");
        let err = result.unwrap_err();
        assert!(
            err.contains("E-06") && err.contains("not valid hex"),
            "E-06: error should mention E-06 and 'not valid hex': {err}"
        );
    }

    /// E-06: missing env value must surface an error with the E-06 tag.
    #[test]
    fn test_e06_missing_env_returns_error() {
        let result = decode_pickle_key_from_env(None);
        assert!(result.is_err(), "E-06: missing key must return Err");
        let err = result.unwrap_err();
        assert!(err.contains("E-06"), "E-06: error should carry the E-06 tag: {err}");
    }

    /// E-06: 16-byte (32 hex char) keys must be rejected.
    #[test]
    fn test_e06_wrong_length_returns_error() {
        let result = decode_pickle_key_from_env(Some(&"a".repeat(32)));
        assert!(result.is_err(), "E-06: 16-byte key must be rejected");
        let err = result.unwrap_err();
        assert!(err.contains("32 bytes") || err.contains("64 hex"), "E-06: error should mention correct length: {err}");
    }
}
