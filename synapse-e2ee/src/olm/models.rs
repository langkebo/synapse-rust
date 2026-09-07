use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `OlmAccountInfo` type.
pub struct OlmAccountInfo {
    /// The `identity_key` field.
    /// The `one_time_keys` field.
    /// The `fallback_key` field.
    pub identity_key: String,
    /// The `one_time_keys` field.
    /// The `fallback_key` field.
    pub one_time_keys: Vec<String>,
    /// The `fallback_key` field.
    pub fallback_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `OlmMessageInfo` type.
pub struct OlmMessageInfo {
    /// The `message_type` field.
    /// The `ciphertext` field.
    pub message_type: OlmMessageType,
    /// The `ciphertext` field.
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// The `OlmMessageType` enum.
pub enum OlmMessageType {
    /// The `PreKey` variant.
    /// The `Message` variant.
    PreKey,
    /// The `Message` variant.
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `OlmAccountData` type.
pub struct OlmAccountData {
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `identity_key` field.
    /// The `serialized_account` field.
    /// The `has_published_one_time_keys` field.
    /// The `has_published_fallback_key` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `identity_key` field.
    /// The `serialized_account` field.
    /// The `has_published_one_time_keys` field.
    /// The `has_published_fallback_key` field.
    pub device_id: String,
    /// The `identity_key` field.
    /// The `serialized_account` field.
    /// The `has_published_one_time_keys` field.
    /// The `has_published_fallback_key` field.
    pub identity_key: String,
    /// The `serialized_account` field.
    /// The `has_published_one_time_keys` field.
    /// The `has_published_fallback_key` field.
    pub serialized_account: String,
    /// The `has_published_one_time_keys` field.
    /// The `has_published_fallback_key` field.
    pub has_published_one_time_keys: bool,
    /// The `has_published_fallback_key` field.
    pub has_published_fallback_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `OlmSessionData` type.
pub struct OlmSessionData {
    /// The `session_id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub session_id: String,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub device_id: String,
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub sender_key: String,
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub receiver_key: String,
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub serialized_state: String,
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub message_index: u32,
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub created_ts: i64,
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub last_used_ts: i64,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `OlmEncryptedMessage` type.
pub struct OlmEncryptedMessage {
    /// The `session_id` field.
    /// The `message_type` field.
    /// The `ciphertext` field.
    pub session_id: String,
    /// The `message_type` field.
    /// The `ciphertext` field.
    pub message_type: OlmMessageType,
    /// The `ciphertext` field.
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `OlmDecryptedMessage` type.
pub struct OlmDecryptedMessage {
    /// The `plaintext` field.
    /// The `session_id` field.
    pub plaintext: String,
    /// The `session_id` field.
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `OneTimeKey` type.
pub struct OneTimeKey {
    /// The `key_id` field.
    /// The `public_key` field.
    pub key_id: String,
    /// The `public_key` field.
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `FallbackKey` type.
pub struct FallbackKey {
    /// The `key_id` field.
    /// The `public_key` field.
    /// The `used` field.
    pub key_id: String,
    /// The `public_key` field.
    /// The `used` field.
    pub public_key: String,
    /// The `used` field.
    pub used: bool,
}

/// (see code)
impl OlmSessionData {
    /// See [`new`].
    pub fn new(
        session_id: String,
        user_id: String,
        device_id: String,
        sender_key: String,
        receiver_key: String,
        serialized_state: String,
    ) -> Self {
        let now = current_timestamp_millis();
        Self {
            session_id,
            user_id,
            device_id,
            sender_key,
            receiver_key,
            serialized_state,
            message_index: 0,
            created_ts: now,
            last_used_ts: now,
            expires_at: None,
        }
    }

    /// See [`touch`].
    pub fn touch(&mut self) {
        self.last_used_ts = current_timestamp_millis();
    }

    /// See [`increment_message_index`].
    pub fn increment_message_index(&mut self) {
        self.message_index += 1;
    }

    /// See [`is_expired`].
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = current_timestamp_millis();
            return now > expires_at;
        }
        false
    }
}

/// (see code)
impl OlmAccountData {
    /// See [`new`].
    pub fn new(user_id: String, device_id: String, identity_key: String, serialized_account: String) -> Self {
        Self {
            user_id,
            device_id,
            identity_key,
            serialized_account,
            has_published_one_time_keys: false,
            has_published_fallback_key: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn olm_session_new_sets_fields() {
        let session = OlmSessionData::new(
            "sid".into(),
            "alice".into(),
            "DEV1".into(),
            "sender_key".into(),
            "receiver_key".into(),
            "state".into(),
        );
        assert_eq!(session.session_id, "sid");
        assert_eq!(session.user_id, "alice");
        assert_eq!(session.device_id, "DEV1");
        assert_eq!(session.sender_key, "sender_key");
        assert_eq!(session.receiver_key, "receiver_key");
        assert_eq!(session.serialized_state, "state");
        assert_eq!(session.message_index, 0);
        assert!(session.created_ts > 0);
        assert_eq!(session.created_ts, session.last_used_ts);
        assert_eq!(session.expires_at, None);
    }

    #[test]
    fn olm_session_touch_updates_last_used_ts() {
        let mut session =
            OlmSessionData::new("sid".into(), "alice".into(), "DEV1".into(), "sk".into(), "rk".into(), "state".into());
        let old_ts = session.last_used_ts;
        session.touch();
        assert!(session.last_used_ts >= old_ts);
    }

    #[test]
    fn olm_session_increment_message_index() {
        let mut session =
            OlmSessionData::new("sid".into(), "alice".into(), "DEV1".into(), "sk".into(), "rk".into(), "state".into());
        assert_eq!(session.message_index, 0);
        session.increment_message_index();
        assert_eq!(session.message_index, 1);
        session.increment_message_index();
        assert_eq!(session.message_index, 2);
    }

    #[test]
    fn olm_session_is_expired_no_expiry() {
        let session =
            OlmSessionData::new("sid".into(), "alice".into(), "DEV1".into(), "sk".into(), "rk".into(), "state".into());
        assert!(!session.is_expired());
    }

    #[test]
    fn olm_session_is_expired_past() {
        let mut session =
            OlmSessionData::new("sid".into(), "alice".into(), "DEV1".into(), "sk".into(), "rk".into(), "state".into());
        session.expires_at = Some(1); // Unix epoch + 1ms — definitely in the past
        assert!(session.is_expired());
    }

    #[test]
    fn olm_session_is_expired_future() {
        let mut session =
            OlmSessionData::new("sid".into(), "alice".into(), "DEV1".into(), "sk".into(), "rk".into(), "state".into());
        session.expires_at = Some(9999999999999i64); // far future
        assert!(!session.is_expired());
    }

    #[test]
    fn olm_account_data_new() {
        let account = OlmAccountData::new("alice".into(), "DEV1".into(), "id_key".into(), "serial".into());
        assert_eq!(account.user_id, "alice");
        assert_eq!(account.device_id, "DEV1");
        assert_eq!(account.identity_key, "id_key");
        assert_eq!(account.serialized_account, "serial");
        assert!(!account.has_published_one_time_keys);
        assert!(!account.has_published_fallback_key);
    }
}
