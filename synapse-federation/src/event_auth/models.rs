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
pub struct EventAuthChain {
    pub(crate) auth_chain_cache: Cache<String, bool>,
    pub(crate) depth_cache: Cache<String, i64>,
}

impl Default for EventAuthChain {
    fn default() -> Self {
        Self::new()
    }
}

impl EventAuthChain {
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

#[derive(Debug, Clone)]
pub struct EventData {
    pub event_id: String,
    pub room_id: String,
    pub event_type: String,
    pub auth_events: Vec<String>,
    pub prev_events: Vec<String>,
    pub state_key: Option<Value>,
    pub content: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct EventInfo {
    pub event_id: String,
    pub prev_events: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct ConflictInfo {
    pub state_key: String,
    pub winning_event: String,
    pub losing_events: Vec<String>,
    pub resolution_reason: String,
}
