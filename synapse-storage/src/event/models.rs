use sqlx::{Pool, Postgres};
use std::sync::Arc;

/// Unifies `origin_server_ts` vs `stream_ordering` filtering in batch state queries.
#[derive(Debug, Clone, Copy)]
pub enum SinceFilter {
    OriginServerTs(i64),
    StreamOrdering(i64),
}

impl SinceFilter {
    pub fn column(&self) -> &'static str {
        match self {
            SinceFilter::OriginServerTs(_) => "origin_server_ts",
            SinceFilter::StreamOrdering(_) => "stream_ordering",
        }
    }

    pub fn value(&self) -> i64 {
        match self {
            SinceFilter::OriginServerTs(v) | SinceFilter::StreamOrdering(v) => *v,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
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
    /// Target event_id for `m.room.redaction` events (P0-05).  `None` for
    /// non-redaction events or redaction events that do not specify a target.
    pub redacts: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StateEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: Option<String>,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub unsigned: Option<serde_json::Value>,
    pub is_redacted: Option<bool>,
    pub origin_server_ts: i64,
    pub depth: Option<i64>,
    #[sqlx(rename = "processed_at")]
    pub processed_ts: Option<i64>,
    pub not_before: Option<i64>,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: Option<String>,
    pub user_id: Option<String>,
    pub stream_ordering: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEphemeralEvent {
    pub event_type: String,
    pub user_id: String,
    pub content: serde_json::Value,
    pub stream_id: i64,
    pub created_ts: i64,
}

#[derive(Clone)]
pub struct EventStorage {
    pub pool: Arc<Pool<Postgres>>,
    pub server_name: String,
}

#[derive(Debug, Clone)]
pub struct CreateEventParams {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub origin_server_ts: i64,
    /// Target event_id for `m.room.redaction` events (P0-05).  Set to `None`
    /// for non-redaction events.  For v1-v10 this is populated from the
    /// top-level `redacts` PDU field; for v11+ from `content.redacts`.
    pub redacts: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventQueryFilter {
    pub types: Option<Vec<String>>,
    pub not_types: Option<Vec<String>>,
    pub senders: Option<Vec<String>>,
    pub not_senders: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Event signature model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventSignature {
    pub id: uuid::Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub created_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReportId {
    pub id: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReport {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reason: Option<String>,
    pub score: i32,
    pub received_ts: i64,
    #[sqlx(rename = "resolved_at")]
    pub resolved_ts: Option<i64>,
    pub resolved_by: Option<String>,
}
