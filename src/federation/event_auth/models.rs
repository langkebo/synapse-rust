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

#[cfg(test)]
mod tests {
    use super::*;

    // --- Constants ---

    #[test]
    fn test_auth_chain_constants() {
        assert_eq!(AUTH_CHAIN_CACHE_SIZE, 1000);
        assert_eq!(DEPTH_CACHE_SIZE, 2000);
        assert_eq!(AUTH_CHAIN_CACHE_TTL_SECS, 3600);
        assert_eq!(DEPTH_CACHE_TTL_SECS, 3600);
        assert_eq!(STATE_RESOLUTION_MAX_HOPS, 100);
    }

    // --- EventAuthChain ---

    #[test]
    fn test_event_auth_chain_new_creates_caches() {
        let chain = EventAuthChain::new();
        // Verify caches start empty
        assert!(!chain.auth_chain_cache.contains_key("nonexistent"));
        assert!(!chain.depth_cache.contains_key("nonexistent"));
    }

    #[test]
    fn test_event_auth_chain_default_equals_new() {
        let chain1 = EventAuthChain::default();
        let chain2 = EventAuthChain::new();
        // Both should have empty caches
        assert!(!chain1.auth_chain_cache.contains_key("nonexistent"));
        assert!(!chain2.auth_chain_cache.contains_key("nonexistent"));
    }

    #[test]
    fn test_event_auth_chain_insert_and_lookup() {
        let chain = EventAuthChain::new();
        chain.auth_chain_cache.insert("event1".to_string(), true);
        chain.auth_chain_cache.insert("event2".to_string(), false);

        assert_eq!(chain.auth_chain_cache.get("event1"), Some(true));
        assert_eq!(chain.auth_chain_cache.get("event2"), Some(false));
        assert_eq!(chain.auth_chain_cache.get("event3"), None);
    }

    #[test]
    fn test_event_auth_chain_depth_cache() {
        let chain = EventAuthChain::new();
        chain.depth_cache.insert("event_a".to_string(), 5);
        chain.depth_cache.insert("event_b".to_string(), 10);

        assert_eq!(chain.depth_cache.get("event_a"), Some(5));
        assert_eq!(chain.depth_cache.get("event_b"), Some(10));
        assert_eq!(chain.depth_cache.get("event_c"), None);
    }

    #[test]
    fn test_event_auth_chain_clone() {
        let chain = EventAuthChain::new();
        chain.auth_chain_cache.insert("key".to_string(), true);
        let cloned = chain.clone();
        assert_eq!(cloned.auth_chain_cache.get("key"), Some(true));
    }

    // --- EventData ---

    #[test]
    fn test_event_data_construction() {
        let data = EventData {
            event_id: "$evt1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            auth_events: vec!["$auth1:example.com".to_string()],
            prev_events: vec!["$prev1:example.com".to_string()],
            state_key: None,
            content: Some(serde_json::json!({"body": "hello"})),
        };
        assert_eq!(data.event_id, "$evt1:example.com");
        assert_eq!(data.room_id, "!room:example.com");
        assert_eq!(data.event_type, "m.room.message");
        assert_eq!(data.auth_events.len(), 1);
        assert_eq!(data.prev_events.len(), 1);
        assert!(data.state_key.is_none());
        assert!(data.content.is_some());
    }

    #[test]
    fn test_event_data_with_state_key() {
        let data = EventData {
            event_id: "$evt2:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_type: "m.room.member".to_string(),
            auth_events: vec![],
            prev_events: vec![],
            state_key: Some(serde_json::Value::String("@alice:example.com".to_string())),
            content: None,
        };
        assert_eq!(data.state_key.unwrap(), serde_json::Value::String("@alice:example.com".to_string()));
    }

    #[test]
    fn test_event_data_clone() {
        let data = EventData {
            event_id: "$evt:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            auth_events: vec![],
            prev_events: vec![],
            state_key: None,
            content: None,
        };
        let cloned = data.clone();
        assert_eq!(cloned.event_id, data.event_id);
    }

    // --- EventInfo ---

    #[test]
    fn test_event_info_construction() {
        let info = EventInfo {
            event_id: "$evt:example.com".to_string(),
            prev_events: Some(serde_json::json!(["$prev1", "$prev2"])),
        };
        assert_eq!(info.event_id, "$evt:example.com");
        assert!(info.prev_events.is_some());
    }

    #[test]
    fn test_event_info_no_prev_events() {
        let info = EventInfo { event_id: "$evt:example.com".to_string(), prev_events: None };
        assert!(info.prev_events.is_none());
    }

    // --- ConflictInfo ---

    #[test]
    fn test_conflict_info_construction() {
        let conflict = ConflictInfo {
            state_key: "@alice:example.com".to_string(),
            winning_event: "$win:example.com".to_string(),
            losing_events: vec!["$lose1:example.com".to_string(), "$lose2:example.com".to_string()],
            resolution_reason: "Higher depth".to_string(),
        };
        assert_eq!(conflict.state_key, "@alice:example.com");
        assert_eq!(conflict.winning_event, "$win:example.com");
        assert_eq!(conflict.losing_events.len(), 2);
        assert_eq!(conflict.resolution_reason, "Higher depth");
    }

    #[test]
    fn test_conflict_info_single_loser() {
        let conflict = ConflictInfo {
            state_key: "".to_string(),
            winning_event: "$win:example.com".to_string(),
            losing_events: vec!["$lose:example.com".to_string()],
            resolution_reason: "Tiebreaker by origin".to_string(),
        };
        assert_eq!(conflict.losing_events.len(), 1);
    }

    #[test]
    fn test_conflict_info_clone() {
        let conflict = ConflictInfo {
            state_key: "key".to_string(),
            winning_event: "$win".to_string(),
            losing_events: vec!["$lose".to_string()],
            resolution_reason: "reason".to_string(),
        };
        let cloned = conflict.clone();
        assert_eq!(cloned.state_key, conflict.state_key);
        assert_eq!(cloned.winning_event, conflict.winning_event);
    }
}
