//! Thread data models.
//!
//! All persisted-entity structs and query parameter structs for the thread
//! storage domain.

use serde::{Deserialize, Serialize};

/// The `ThreadRoot` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadRoot {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `thread_id` field.
    pub thread_id: Option<String>,
    /// The `reply_count` field.
    pub reply_count: Option<i64>,
    /// The `last_reply_event_id` field.
    pub last_reply_event_id: Option<String>,
    /// The `last_reply_sender` field.
    pub last_reply_sender: Option<String>,
    /// The `last_reply_ts` field.
    pub last_reply_ts: Option<i64>,
    /// The `participants` field.
    pub participants: Option<serde_json::Value>,
    /// The `is_fetched` field.
    pub is_fetched: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// The `ThreadReply` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadReply {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `in_reply_to_event_id` field.
    pub in_reply_to_event_id: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `is_edited` field.
    pub is_edited: bool,
    /// The `is_redacted` field.
    pub is_redacted: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `ThreadSubscription` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSubscription {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `notification_level` field.
    pub notification_level: String,
    /// The `is_muted` field.
    pub is_muted: bool,
    /// The `is_pinned` field.
    pub is_pinned: bool,
    /// The `subscribed_ts` field.
    pub subscribed_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `ThreadReadReceipt` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadReadReceipt {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `last_read_event_id` field.
    pub last_read_event_id: Option<String>,
    /// The `last_read_ts` field.
    pub last_read_ts: i64,
    /// The `unread_count` field.
    pub unread_count: i32,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `ThreadRelation` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadRelation {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `relates_to_event_id` field.
    pub relates_to_event_id: String,
    /// The `relation_type` field.
    pub relation_type: String,
    /// The `thread_id` field.
    pub thread_id: Option<String>,
    /// The `is_falling_back` field.
    pub is_falling_back: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `ThreadSummary` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSummary {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `root_sender` field.
    pub root_sender: String,
    /// The `root_content` field.
    pub root_content: serde_json::Value,
    /// The `root_origin_server_ts` field.
    pub root_origin_server_ts: i64,
    /// The `latest_event_id` field.
    pub latest_event_id: Option<String>,
    /// The `latest_sender` field.
    pub latest_sender: Option<String>,
    /// The `latest_content` field.
    pub latest_content: Option<serde_json::Value>,
    /// The `latest_origin_server_ts` field.
    pub latest_origin_server_ts: Option<i64>,
    /// The `reply_count` field.
    pub reply_count: i32,
    /// The `participants` field.
    pub participants: serde_json::Value,
    /// The `is_frozen` field.
    pub is_frozen: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `ThreadStatistics` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadStatistics {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `total_replies` field.
    pub total_replies: i32,
    /// The `total_participants` field.
    pub total_participants: i32,
    /// The `total_edits` field.
    pub total_edits: i32,
    /// The `total_redactions` field.
    pub total_redactions: i32,
    /// The `first_reply_ts` field.
    pub first_reply_ts: Option<i64>,
    /// The `last_reply_ts` field.
    pub last_reply_ts: Option<i64>,
    /// The `avg_reply_time_ms` field.
    pub avg_reply_time_ms: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CreateThreadRootParams` struct.
#[derive(Debug, Clone)]
pub struct CreateThreadRootParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `thread_id` field.
    pub thread_id: Option<String>,
}

/// The `CreateThreadReplyParams` struct.
#[derive(Debug, Clone)]
pub struct CreateThreadReplyParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `in_reply_to_event_id` field.
    pub in_reply_to_event_id: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
}

/// The `ThreadListParams` struct.
#[derive(Debug, Clone)]
pub struct ThreadListParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// The `from` field.
    pub from: Option<String>,
    /// The `include_all` field.
    pub include_all: bool,
}

/// The `ThreadWithReplies` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadWithReplies {
    /// The `root` field.
    pub root: ThreadRoot,
    /// The `replies` field.
    pub replies: Vec<ThreadReply>,
    /// The `reply_count` field.
    pub reply_count: i32,
    /// The `participants` field.
    pub participants: Vec<String>,
}
