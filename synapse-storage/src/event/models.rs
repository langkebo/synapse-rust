use sqlx::{Pool, Postgres};
use std::sync::Arc;

/// Unifies `origin_server_ts` vs `stream_ordering` filtering in batch state queries.
#[derive(Debug, Clone, Copy)]
pub enum SinceFilter {
    /// The `OriginServerTs` variant.
    OriginServerTs(i64),
    /// The `StreamOrdering` variant.
    StreamOrdering(i64),
}

impl SinceFilter {
    /// See [`column`].
    /// See [`column`].
    pub fn column(&self) -> &'static str {
        match self {
            SinceFilter::OriginServerTs(_) => "origin_server_ts",
            SinceFilter::StreamOrdering(_) => "stream_ordering",
        }
    }

    /// See [`value`].
    /// See [`value`].
    pub fn value(&self) -> i64 {
        match self {
            SinceFilter::OriginServerTs(v) | SinceFilter::StreamOrdering(v) => *v,
        }
    }
}

/// The `RoomEvent` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEvent {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `depth` field.
    pub depth: i64,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    #[sqlx(rename = "processed_at")]
    /// The `processed_ts` field.
    pub processed_ts: i64,
    /// The `not_before` field.
    pub not_before: i64,
    /// The `status` field.
    pub status: Option<String>,
    /// The `origin` field.
    pub origin: String,
    /// The `stream_ordering` field.
    pub stream_ordering: Option<i64>,
    /// Target event_id for `m.room.redaction` events (P0-05).  `None` for
    /// non-redaction events or redaction events that do not specify a target.
    pub redacts: Option<String>,
}

/// The `StateEvent` struct.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct StateEvent {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `unsigned` field.
    pub unsigned: Option<serde_json::Value>,
    /// The `is_redacted` field.
    pub is_redacted: Option<bool>,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `depth` field.
    pub depth: Option<i64>,
    #[sqlx(rename = "processed_at")]
    /// The `processed_ts` field.
    pub processed_ts: Option<i64>,
    /// The `not_before` field.
    pub not_before: Option<i64>,
    /// The `status` field.
    pub status: Option<String>,
    /// The `origin` field.
    pub origin: Option<String>,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `stream_ordering` field.
    pub stream_ordering: Option<i64>,
}

/// The `RoomEphemeralEvent` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEphemeralEvent {
    /// The `event_type` field.
    pub event_type: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `stream_id` field.
    pub stream_id: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `EventStorage` struct.
#[derive(Clone)]
pub struct EventStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
    /// The `server_name` field.
    pub server_name: String,
}

/// The `CreateEventParams` struct.
#[derive(Debug, Clone)]
pub struct CreateEventParams {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// Target event_id for `m.room.redaction` events (P0-05).  Set to `None`
    /// for non-redaction events.  For v1-v10 this is populated from the
    /// top-level `redacts` PDU field; for v11+ from `content.redacts`.
    pub redacts: Option<String>,
}

/// The `EventQueryFilter` struct.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventQueryFilter {
    /// The `types` field.
    pub types: Option<Vec<String>>,
    /// The `not_types` field.
    pub not_types: Option<Vec<String>>,
    /// The `senders` field.
    pub senders: Option<Vec<String>>,
    /// The `not_senders` field.
    pub not_senders: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Event signature model
// ---------------------------------------------------------------------------

/// The `EventSignature` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventSignature {
    /// The `id` field.
    pub id: uuid::Uuid,
    /// The `event_id` field.
    pub event_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `signature` field.
    pub signature: String,
    /// The `key_id` field.
    pub key_id: String,
    /// The `created_ts` field.
    pub created_ts: Option<i64>,
}

/// The `EventReportId` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReportId {
    /// The `id` field.
    pub id: i64,
}

/// The `EventReport` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReport {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `reporter_user_id` field.
    pub reporter_user_id: String,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `score` field.
    pub score: i32,
    /// The `received_ts` field.
    pub received_ts: i64,
    #[sqlx(rename = "resolved_at")]
    /// The `resolved_ts` field.
    pub resolved_ts: Option<i64>,
    /// The `resolved_by` field.
    pub resolved_by: Option<String>,
}
