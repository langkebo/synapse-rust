use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmAccountInfo {
    pub identity_key: String,
    pub one_time_keys: Vec<String>,
    pub fallback_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmMessageInfo {
    pub message_type: OlmMessageType,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OlmMessageType {
    PreKey,
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmAccountData {
    pub user_id: String,
    pub device_id: String,
    pub identity_key: String,
    pub serialized_account: String,
    pub has_published_one_time_keys: bool,
    pub has_published_fallback_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmSessionData {
    pub session_id: String,
    pub user_id: String,
    pub device_id: String,
    pub sender_key: String,
    pub receiver_key: String,
    pub serialized_state: String,
    pub message_index: u32,
    pub created_ts: i64,
    pub last_used_ts: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmEncryptedMessage {
    pub session_id: String,
    pub message_type: OlmMessageType,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmDecryptedMessage {
    pub plaintext: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneTimeKey {
    pub key_id: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackKey {
    pub key_id: String,
    pub public_key: String,
    pub used: bool,
}

impl OlmSessionData {
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

    pub fn touch(&mut self) {
        self.last_used_ts = current_timestamp_millis();
    }

    pub fn increment_message_index(&mut self) {
        self.message_index += 1;
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = current_timestamp_millis();
            return now > expires_at;
        }
        false
    }
}

impl OlmAccountData {
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
