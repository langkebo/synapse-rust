use serde::{Deserialize, Serialize};

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
        let now = chrono::Utc::now().timestamp_millis();
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
        self.last_used_ts = chrono::Utc::now().timestamp_millis();
    }

    pub fn increment_message_index(&mut self) {
        self.message_index += 1;
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = chrono::Utc::now().timestamp_millis();
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
    fn test_olm_message_type() {
        assert_eq!(serde_json::to_string(&OlmMessageType::PreKey).unwrap(), r#""PreKey""#);
        assert_eq!(serde_json::to_string(&OlmMessageType::Message).unwrap(), r#""Message""#);
    }

    #[test]
    fn test_olm_session_data_new() {
        let session = OlmSessionData::new(
            "session_1".to_string(),
            "@alice:example.com".to_string(),
            "DEVICE1".to_string(),
            "sender_key".to_string(),
            "receiver_key".to_string(),
            "serialized_state".to_string(),
        );
        assert_eq!(session.session_id, "session_1");
        assert_eq!(session.user_id, "@alice:example.com");
        assert_eq!(session.message_index, 0);
        assert!(session.created_ts > 0);
        assert_eq!(session.created_ts, session.last_used_ts);
        assert!(!session.is_expired());
    }

    #[test]
    fn test_olm_session_data_touch() {
        let mut session = OlmSessionData::new(
            "s1".to_string(),
            "@user:example.com".to_string(),
            "DEV".to_string(),
            "sk".to_string(),
            "rk".to_string(),
            "state".to_string(),
        );
        let old_ts = session.last_used_ts;
        std::thread::sleep(std::time::Duration::from_millis(5));
        session.touch();
        assert!(session.last_used_ts > old_ts);
    }

    #[test]
    fn test_olm_session_data_increment_message_index() {
        let mut session = OlmSessionData::new(
            "s1".to_string(),
            "@user:example.com".to_string(),
            "DEV".to_string(),
            "sk".to_string(),
            "rk".to_string(),
            "state".to_string(),
        );
        assert_eq!(session.message_index, 0);
        session.increment_message_index();
        assert_eq!(session.message_index, 1);
        session.increment_message_index();
        assert_eq!(session.message_index, 2);
    }

    #[test]
    fn test_olm_session_data_is_expired() {
        let mut session = OlmSessionData::new(
            "s1".to_string(),
            "@user:example.com".to_string(),
            "DEV".to_string(),
            "sk".to_string(),
            "rk".to_string(),
            "state".to_string(),
        );
        assert!(!session.is_expired());

        let now = chrono::Utc::now().timestamp_millis();
        session.expires_at = Some(now - 1000);
        assert!(session.is_expired());

        session.expires_at = Some(now + 3600000);
        assert!(!session.is_expired());
    }

    #[test]
    fn test_olm_account_data_new() {
        let account = OlmAccountData::new(
            "@alice:example.com".to_string(),
            "DEVICE1".to_string(),
            "identity_key".to_string(),
            "serialized".to_string(),
        );
        assert_eq!(account.user_id, "@alice:example.com");
        assert_eq!(account.device_id, "DEVICE1");
        assert_eq!(account.identity_key, "identity_key");
        assert!(!account.has_published_one_time_keys);
        assert!(!account.has_published_fallback_key);
    }

    #[test]
    fn test_olm_account_info() {
        let info = OlmAccountInfo {
            identity_key: "id_key".to_string(),
            one_time_keys: vec!["otk1".to_string(), "otk2".to_string()],
            fallback_key: Some("fbk".to_string()),
        };
        assert_eq!(info.identity_key, "id_key");
        assert_eq!(info.one_time_keys.len(), 2);
        assert!(info.fallback_key.is_some());
    }

    #[test]
    fn test_olm_message_info() {
        let info = OlmMessageInfo { message_type: OlmMessageType::PreKey, ciphertext: "encrypted_text".to_string() };
        assert_eq!(info.message_type, OlmMessageType::PreKey);
        assert_eq!(info.ciphertext, "encrypted_text");
    }

    #[test]
    fn test_olm_encrypted_message() {
        let msg = OlmEncryptedMessage {
            session_id: "s1".to_string(),
            message_type: OlmMessageType::Message,
            ciphertext: "cipher".to_string(),
        };
        assert_eq!(msg.session_id, "s1");
    }

    #[test]
    fn test_olm_decrypted_message() {
        let msg = OlmDecryptedMessage { plaintext: "hello".to_string(), session_id: "s1".to_string() };
        assert_eq!(msg.plaintext, "hello");
    }

    #[test]
    fn test_one_time_key() {
        let key = OneTimeKey { key_id: "signed_curve25519:AAAAAA".to_string(), public_key: "base64key".to_string() };
        assert_eq!(key.key_id, "signed_curve25519:AAAAAA");
    }

    #[test]
    fn test_fallback_key() {
        let key = FallbackKey {
            key_id: "signed_curve25519:BBBBBB".to_string(),
            public_key: "base64key".to_string(),
            used: false,
        };
        assert!(!key.used);
    }

    #[test]
    fn test_fallback_key_used() {
        let key = FallbackKey {
            key_id: "signed_curve25519:CCCCCC".to_string(),
            public_key: "base64key".to_string(),
            used: true,
        };
        assert!(key.used);
    }
}
