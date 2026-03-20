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
    pub fn new(
        user_id: String,
        device_id: String,
        identity_key: String,
        serialized_account: String,
    ) -> Self {
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
