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
