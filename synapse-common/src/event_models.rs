use serde::{Deserialize, Serialize};

/// Core room event model used across the codebase.
/// Originally defined in storage, but moved to common to resolve
/// the circular dependency between common → storage.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomEvent {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub depth: i64,
    pub origin_server_ts: i64,
    #[sqlx(rename = "processed_at")]
    pub processed_ts: i64,
    pub not_before: i64,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: String,
    pub stream_ordering: Option<i64>,
}
