use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MegolmSession {
    pub id: Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEvent {
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub algorithm: String,
    pub session_id: String,
    pub ciphertext: String,
    pub device_id: String,
}