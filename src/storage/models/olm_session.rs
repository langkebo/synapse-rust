use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct OlmSession {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub session_id: String,
    pub sender_key: String,
    pub receiver_key: String,
    pub serialized_state: String,
    pub message_index: i32,
    pub created_ts: i64,
    pub last_used_ts: i64,
    pub expires_at: Option<i64>,
}
