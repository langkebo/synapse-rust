//! Data models for Matrix events (PDU/EDU, state events, message events).

use serde::{Deserialize, Serialize};

/// Core room event model used across the codebase.
/// Originally defined in storage, but moved to common to resolve
/// the circular dependency between common → storage.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
/// Represents RoomEvent.
pub struct RoomEvent {
    /// `event_id` field.
    pub event_id: String,
    /// `room_id` field.
    pub room_id: String,
    /// `user_id` field.
    pub user_id: String,
    /// `event_type` field.
    pub event_type: String,
    /// `content` field.
    pub content: serde_json::Value,
    /// `state_key` field.
    pub state_key: Option<String>,
    /// `depth` field.
    pub depth: i64,
    /// `origin_server_ts` field.
    pub origin_server_ts: i64,
    #[sqlx(rename = "processed_at")]
    /// `processed_ts` field.
    pub processed_ts: i64,
    /// `not_before` field.
    pub not_before: i64,
    /// `status` field.
    pub status: Option<String>,
    /// `reference_image` field.
    pub reference_image: Option<String>,
    /// `origin` field.
    pub origin: String,
    /// `stream_ordering` field.
    pub stream_ordering: Option<i64>,
}
