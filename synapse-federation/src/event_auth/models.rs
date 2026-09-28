use moka::sync::Cache;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

pub(crate) const AUTH_CHAIN_CACHE_SIZE: u64 = 1000;
pub(crate) const DEPTH_CACHE_SIZE: u64 = 2000;
pub(crate) const AUTH_CHAIN_CACHE_TTL_SECS: u64 = 3600;
pub(crate) const DEPTH_CACHE_TTL_SECS: u64 = 3600;
pub(crate) const STATE_RESOLUTION_MAX_HOPS: usize = 100;

pub(crate) type StateKey = String;
pub(crate) type StateEntry = (i64, String, i64, Option<String>);
pub(crate) type StateByKey = HashMap<StateKey, Vec<StateEntry>>;

#[derive(Debug, Clone)]
/// The `EventAuthChain` type.
pub struct EventAuthChain {
    /// Caches the full computed auth chain (`Vec<String>`) keyed by event id.
    /// Previously this stored only a `bool`, which forced a full BFS
    /// recomputation even on cache hits.
    pub(crate) auth_chain_cache: Cache<String, Vec<String>>,
    pub(crate) depth_cache: Cache<String, i64>,
}

/// Default implementation for [`EventAuthChain`].
impl Default for EventAuthChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Implementation of [`EventAuthChain`] methods.
impl EventAuthChain {
    /// See [`new`.
    pub fn new() -> Self {
        Self {
            auth_chain_cache: Cache::builder()
                .max_capacity(AUTH_CHAIN_CACHE_SIZE)
                .time_to_live(Duration::from_secs(AUTH_CHAIN_CACHE_TTL_SECS))
                .build(),
            depth_cache: Cache::builder()
                .max_capacity(DEPTH_CACHE_SIZE)
                .time_to_live(Duration::from_secs(DEPTH_CACHE_TTL_SECS))
                .build(),
        }
    }
}

#[derive(Debug, Clone, Default)]
/// The `EventData` type.
pub struct EventData {
    /// The `event_id` field.
    /// The `room_id` field.
    /// The `event_type` field.
    /// The `auth_events` field.
    /// The `prev_events` field.
    /// The `state_key` field.
    /// The `content` field.
    pub event_id: String,
    /// The `room_id` field.
    /// The `event_type` field.
    /// The `auth_events` field.
    /// The `prev_events` field.
    /// The `state_key` field.
    /// The `content` field.
    pub room_id: String,
    /// The `event_type` field.
    /// The `auth_events` field.
    /// The `prev_events` field.
    /// The `state_key` field.
    /// The `content` field.
    pub event_type: String,
    /// The `auth_events` field.
    /// The `prev_events` field.
    /// The `state_key` field.
    /// The `content` field.
    pub auth_events: Vec<String>,
    /// The `prev_events` field.
    /// The `state_key` field.
    /// The `content` field.
    pub prev_events: Vec<String>,
    /// The `state_key` field.
    /// The `content` field.
    pub state_key: Option<Value>,
    /// The `content` field.
    pub content: Option<Value>,
    /// 事件发送者 (顶层 PDU 字段, 非 content 内).
    pub sender: String,
    /// 事件原始服务器时间戳 (顶层 PDU 字段, 毫秒).
    pub origin_server_ts: i64,
    /// 事件深度 (顶层 PDU 字段).
    pub depth: i64,
}

impl EventData {
    /// The event as the state event JSON value a resolver result carries.
    ///
    /// State resolution's result is a `(type, state_key) -> winning event` map,
    /// and every caller needs the winner's **`event_id`** to persist or serve it.
    /// Returning a bare `content` object (the previous shape for replayed keys)
    /// made the result un-consumable: a replayed winner had no `event_id`, so it
    /// could not be written to a state store or compared with the unconflicted
    /// entries, which *did* carry one. One shape for both halves of the result is
    /// what makes the resolver callable at all.
    pub fn to_state_event_value(&self) -> Value {
        serde_json::json!({
            "event_id": self.event_id,
            "room_id": self.room_id,
            "type": self.event_type,
            "sender": self.sender,
            "state_key": self.state_key.clone().unwrap_or_else(|| Value::String(String::new())),
            "content": self.content.clone().unwrap_or(Value::Null),
            "origin_server_ts": self.origin_server_ts,
        })
    }
}

#[derive(Debug, Clone)]
/// The `EventInfo` type.
pub struct EventInfo {
    /// The `event_id` field.
    /// The `prev_events` field.
    pub event_id: String,
    /// The `prev_events` field.
    pub prev_events: Option<Value>,
}

#[derive(Debug, Clone)]
/// The `ConflictInfo` type.
pub struct ConflictInfo {
    /// The `state_key` field.
    /// The `winning_event` field.
    /// The `losing_events` field.
    /// The `resolution_reason` field.
    pub state_key: String,
    /// The `winning_event` field.
    /// The `losing_events` field.
    /// The `resolution_reason` field.
    pub winning_event: String,
    /// The `losing_events` field.
    /// The `resolution_reason` field.
    pub losing_events: Vec<String>,
    /// The `resolution_reason` field.
    pub resolution_reason: String,
}
