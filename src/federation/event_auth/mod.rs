pub mod chain;
pub mod models;
pub mod state_resolution;

pub use models::*;

use std::collections::{HashMap, VecDeque};

impl EventAuthChain {
    pub fn get_cached_auth_chain(&self, event_id: &str) -> Option<bool> {
        self.auth_chain_cache.get(event_id)
    }

    pub fn cache_auth_chain_result(&self, event_id: &str, result: bool) {
        self.auth_chain_cache.insert(event_id.to_string(), result);
    }

    pub fn get_cached_depth(&self, event_id: &str) -> Option<i64> {
        self.depth_cache.get(event_id)
    }

    pub fn cache_depth(&self, event_id: &str, depth: i64) {
        self.depth_cache.insert(event_id.to_string(), depth);
    }

    pub fn is_auth_event(event_type: &str) -> bool {
        matches!(
            event_type,
            "m.room.create"
                | "m.room.member"
                | "m.room.power_levels"
                | "m.room.join_rules"
                | "m.room.history_visibility"
                | "m.room.encryption"
                | "m.room.guest_access"
                | "m.room.name"
                | "m.room.topic"
                | "m.room.avatar"
        )
    }

    pub fn calculate_event_depth(&self, events: &[EventInfo]) -> HashMap<String, i64> {
        let mut event_map: HashMap<String, &EventInfo> = HashMap::new();
        let mut in_degree: HashMap<String, i64> = HashMap::new();
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();

        for event in events {
            let event_id = &event.event_id;
            event_map.insert(event_id.clone(), event);
            in_degree.insert(event_id.clone(), 0);
            graph.insert(event_id.clone(), Vec::new());
        }

        for event in events {
            if let Some(prev_events) = &event.prev_events {
                if let Some(prev_array) = prev_events.as_array() {
                    for prev_entry in prev_array {
                        if let Some(inner_array) = prev_entry.as_array() {
                            if let Some(prev_id) = inner_array.first().and_then(|v| v.as_str()) {
                                if event_map.contains_key(prev_id) {
                                    graph.entry(prev_id.to_string()).or_default().push(event.event_id.clone());

                                    *in_degree.entry(event.event_id.clone()).or_default() += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut depth: HashMap<String, i64> = HashMap::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        for (event_id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(event_id.clone());
                depth.insert(event_id.clone(), 1);
            }
        }

        while let Some(current) = queue.pop_front() {
            if let Some(neighbors) = graph.get(&current) {
                for neighbor in neighbors {
                    let new_depth = depth[&current] + 1;
                    if new_depth > depth.get(neighbor).copied().unwrap_or(0) {
                        depth.insert(neighbor.clone(), new_depth);
                    }

                    *in_degree.entry(neighbor.clone()).or_insert(0) -= 1;
                    if in_degree[neighbor] == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        depth
    }

    pub fn calculate_event_depth_with_cache(&self, events: &[EventInfo], event_id: &str) -> Option<i64> {
        let cache_key = format!("depth:{event_id}");

        if let Some(cached) = self.get_cached_depth(&cache_key) {
            return Some(cached);
        }

        let depth_map = self.calculate_event_depth(events);

        if let Some(&depth) = depth_map.get(event_id) {
            self.cache_depth(&cache_key, depth);
            Some(depth)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn test_is_auth_event() {
        assert!(EventAuthChain::is_auth_event("m.room.create"));
        assert!(EventAuthChain::is_auth_event("m.room.member"));
        assert!(EventAuthChain::is_auth_event("m.room.power_levels"));
        assert!(EventAuthChain::is_auth_event("m.room.join_rules"));
        assert!(EventAuthChain::is_auth_event("m.room.history_visibility"));
        assert!(EventAuthChain::is_auth_event("m.room.encryption"));
        assert!(EventAuthChain::is_auth_event("m.room.guest_access"));
        assert!(EventAuthChain::is_auth_event("m.room.name"));
        assert!(EventAuthChain::is_auth_event("m.room.topic"));
        assert!(EventAuthChain::is_auth_event("m.room.avatar"));

        assert!(!EventAuthChain::is_auth_event("m.room.message"));
        assert!(!EventAuthChain::is_auth_event("m.room.encrypted"));
        assert!(!EventAuthChain::is_auth_event("m.room.redaction"));
    }

    #[test]
    fn test_calculate_event_depth_basic() {
        let events = vec![
            EventInfo { event_id: "$1".to_string(), prev_events: None },
            EventInfo { event_id: "$2".to_string(), prev_events: Some(serde_json::json!([["$1", None::<bool>]])) },
            EventInfo { event_id: "$3".to_string(), prev_events: Some(serde_json::json!([["$2", None::<bool>]])) },
        ];

        let depth_map = EventAuthChain::new().calculate_event_depth(&events);

        assert_eq!(depth_map.get("$1"), Some(&1));
        assert_eq!(depth_map.get("$2"), Some(&2));
        assert_eq!(depth_map.get("$3"), Some(&3));
    }

    #[test]
    fn test_calculate_event_depth_empty() {
        let events: Vec<EventInfo> = vec![];

        let depth_map = EventAuthChain::new().calculate_event_depth(&events);

        assert!(depth_map.is_empty());
    }

    #[test]
    fn test_detect_conflicts_single_event() {
        let state_events = vec![json!({
            "event_id": "$1",
            "type": "m.room.name",
            "state_key": "",
            "origin_server_ts": 1000
        })];

        let conflicts = EventAuthChain::new().detect_conflicts(&state_events);

        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflicts_multiple_events() {
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 2000
            }),
        ];

        let conflicts = EventAuthChain::new().detect_conflicts(&state_events);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].state_key, "m.room.name:!room:test");
        assert_eq!(conflicts[0].winning_event, "$2");
        assert_eq!(conflicts[0].losing_events, vec!["$1"]);
    }

    #[test]
    fn test_build_auth_chain() {
        let mut events = HashMap::new();
        events.insert(
            "$create".to_string(),
            EventData {
                event_id: "$create".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );
        events.insert(
            "$member".to_string(),
            EventData {
                event_id: "$member".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.member".to_string(),
                auth_events: vec!["$create".to_string()],
                prev_events: vec!["$create".to_string()],
                state_key: Some(json!("@user:test")),
                content: None,
            },
        );

        let auth_chain = EventAuthChain::new().build_auth_chain_from_events(&events, "$member");

        assert!(auth_chain.contains(&"$create".to_string()));
        assert!(auth_chain.contains(&"$member".to_string()));
    }

    #[test]
    fn test_verify_auth_chain() {
        let mut events = HashMap::new();
        events.insert(
            "$create".to_string(),
            EventData {
                event_id: "$create".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );

        let result = EventAuthChain::new().verify_auth_chain(&events, "!room:test", &["$create".to_string()]);

        assert!(result);
    }

    #[test]
    fn test_verify_auth_chain_wrong_room() {
        let mut events = HashMap::new();
        events.insert(
            "$create".to_string(),
            EventData {
                event_id: "$create".to_string(),
                room_id: "!room:wrong".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );

        let result = EventAuthChain::new().verify_auth_chain(&events, "!room:test", &["$create".to_string()]);

        assert!(!result);
    }

    #[test]
    fn test_empty_auth_chain() {
        let events: HashMap<String, EventData> = HashMap::new();

        let result = EventAuthChain::new().verify_auth_chain(&events, "!room:test", &[]);

        assert!(!result);
    }

    #[test]
    fn test_event_auth_chain_constants() {
        assert_eq!(AUTH_CHAIN_CACHE_SIZE, 1000);
        assert_eq!(DEPTH_CACHE_SIZE, 2000);
        assert_eq!(STATE_RESOLUTION_MAX_HOPS, 100);
    }

    #[test]
    fn test_event_auth_chain_new() {
        let chain = EventAuthChain::new();
        assert_eq!(chain.auth_chain_cache.entry_count(), 0);
        assert_eq!(chain.depth_cache.entry_count(), 0);
    }

    #[test]
    fn test_cache_auth_chain() {
        let chain = EventAuthChain::new();

        let result = chain.get_cached_auth_chain("$test");
        assert!(result.is_none());

        chain.cache_auth_chain_result("$test", true);
        let result = chain.get_cached_auth_chain("$test");
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_cache_depth() {
        let chain = EventAuthChain::new();

        let result = chain.get_cached_depth("$test");
        assert!(result.is_none());

        chain.cache_depth("$test", 42);
        let result = chain.get_cached_depth("$test");
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_build_auth_chain_with_non_auth_events() {
        let mut events = HashMap::new();
        events.insert(
            "$msg1".to_string(),
            EventData {
                event_id: "$msg1".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.message".to_string(),
                auth_events: vec!["$create".to_string()],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );
        events.insert(
            "$create".to_string(),
            EventData {
                event_id: "$create".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );

        let auth_chain = EventAuthChain::new().build_auth_chain_from_events(&events, "$msg1");

        // Should include m.room.create but not m.room.message
        assert!(auth_chain.contains(&"$create".to_string()));
        assert!(!auth_chain.contains(&"$msg1".to_string()));
    }

    #[test]
    fn test_build_auth_chain_empty() {
        let events: HashMap<String, EventData> = HashMap::new();
        let auth_chain = EventAuthChain::new().build_auth_chain_from_events(&events, "$nonexistent");
        assert!(auth_chain.is_empty());
    }

    #[test]
    fn test_build_auth_chain_circular_refs() {
        let mut events = HashMap::new();
        events.insert(
            "$a".to_string(),
            EventData {
                event_id: "$a".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec!["$b".to_string()],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );
        events.insert(
            "$b".to_string(),
            EventData {
                event_id: "$b".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.member".to_string(),
                auth_events: vec!["$a".to_string()],
                prev_events: vec![],
                state_key: Some(json!("@user:test")),
                content: None,
            },
        );

        let auth_chain = EventAuthChain::new().build_auth_chain_from_events(&events, "$a");

        // Should handle circular references without infinite loop
        assert!(!auth_chain.is_empty());
    }

    #[test]
    fn test_verify_auth_chain_first_event_not_in_map() {
        let events: HashMap<String, EventData> = HashMap::new();

        // First event in chain not in events map - should still work
        let result = EventAuthChain::new().verify_auth_chain(&events, "!room:test", &["$create".to_string()]);
        assert!(result);
    }

    #[test]
    fn test_calculate_event_depth_multiple_roots() {
        let events = vec![
            EventInfo { event_id: "$1".to_string(), prev_events: None },
            EventInfo { event_id: "$2".to_string(), prev_events: None },
            EventInfo {
                event_id: "$3".to_string(),
                prev_events: Some(serde_json::json!([["$1", null], ["$2", null]])),
            },
        ];

        let depth_map = EventAuthChain::new().calculate_event_depth(&events);

        assert_eq!(depth_map.get("$1"), Some(&1));
        assert_eq!(depth_map.get("$2"), Some(&1));
        assert_eq!(depth_map.get("$3"), Some(&2));
    }

    #[test]
    fn test_calculate_event_depth_invalid_prev_format() {
        let events =
            vec![EventInfo { event_id: "$1".to_string(), prev_events: Some(serde_json::json!({"invalid": "format"})) }];

        let depth_map = EventAuthChain::new().calculate_event_depth(&events);

        // Should handle invalid format gracefully
        assert!(depth_map.contains_key("$1"));
    }

    #[test]
    fn test_detect_conflicts_no_state_key() {
        let state_events = vec![json!({
            "event_id": "$1",
            "type": "m.room.message",
            "state_key": "",
            "origin_server_ts": 1000
        })];

        let conflicts = EventAuthChain::new().detect_conflicts(&state_events);

        // Empty state_key should be skipped
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflicts_three_events() {
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 2000
            }),
            json!({
                "event_id": "$3",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 3000
            }),
        ];

        let conflicts = EventAuthChain::new().detect_conflicts(&state_events);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].winning_event, "$3");
        let mut losing = conflicts[0].losing_events.clone();
        losing.sort();
        assert_eq!(losing, vec!["$1", "$2"]);
    }

    #[test]
    fn test_detect_conflicts_different_types() {
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.topic",
                "state_key": "!room:test",
                "origin_server_ts": 2000
            }),
        ];

        let conflicts = EventAuthChain::new().detect_conflicts(&state_events);

        // Different event types with same state_key should not conflict
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_resolve_conflicts_power_based() {
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000,
                "sender": "@alice:test"
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 2000,
                "sender": "@bob:test"
            }),
        ];

        let mut power_levels = HashMap::new();
        power_levels.insert("@alice:test".to_string(), 100);
        power_levels.insert("@bob:test".to_string(), 50);

        let conflicts = EventAuthChain::new().resolve_conflicts_power_based(&state_events, &power_levels);

        assert_eq!(conflicts.len(), 1);
        // Alice has higher power, should win despite lower timestamp
        assert_eq!(conflicts[0].winning_event, "$1");
    }

    #[test]
    fn test_resolve_conflicts_power_equal_timestamps() {
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000,
                "sender": "@alice:test"
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 2000,
                "sender": "@bob:test"
            }),
        ];

        let mut power_levels = HashMap::new();
        power_levels.insert("@alice:test".to_string(), 50);
        power_levels.insert("@bob:test".to_string(), 50);

        let conflicts = EventAuthChain::new().resolve_conflicts_power_based(&state_events, &power_levels);

        assert_eq!(conflicts.len(), 1);
        // Equal power, higher timestamp should win
        assert_eq!(conflicts[0].winning_event, "$2");
    }

    #[test]
    fn test_resolve_conflicts_power_no_power_levels() {
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000,
                "sender": "@alice:test"
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 2000,
                "sender": "@bob:test"
            }),
        ];

        let power_levels: HashMap<String, i64> = HashMap::new();

        let conflicts = EventAuthChain::new().resolve_conflicts_power_based(&state_events, &power_levels);

        assert_eq!(conflicts.len(), 1);
        // No power levels, should use timestamp
        assert_eq!(conflicts[0].winning_event, "$2");
    }

    #[test]
    fn test_event_data_clone() {
        let data = EventData {
            event_id: "$1".to_string(),
            room_id: "!room:test".to_string(),
            event_type: "m.room.create".to_string(),
            auth_events: vec![],
            prev_events: vec![],
            state_key: None,
            content: None,
        };

        let cloned = data.clone();
        assert_eq!(data.event_id, cloned.event_id);
    }

    #[test]
    fn test_event_info_clone() {
        let info = EventInfo { event_id: "$1".to_string(), prev_events: None };

        let cloned = info.clone();
        assert_eq!(info.event_id, cloned.event_id);
    }

    #[test]
    fn test_conflict_info_clone() {
        let info = ConflictInfo {
            state_key: "m.room.name:!".to_string(),
            winning_event: "$1".to_string(),
            losing_events: vec!["$2".to_string()],
            resolution_reason: "test".to_string(),
        };

        let cloned = info.clone();
        assert_eq!(info.state_key, cloned.state_key);
    }

    #[test]
    fn test_calculate_event_depth_with_cache() {
        let chain = EventAuthChain::new();

        let events = vec![EventInfo { event_id: "$1".to_string(), prev_events: None }];

        let result = chain.calculate_event_depth_with_cache(&events, "$1");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn test_calculate_event_depth_with_cache_miss() {
        let chain = EventAuthChain::new();

        let events: Vec<EventInfo> = vec![];

        let result = chain.calculate_event_depth_with_cache(&events, "$nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_build_auth_chain_with_cache() {
        let chain = EventAuthChain::new();

        let mut events = HashMap::new();
        events.insert(
            "$create".to_string(),
            EventData {
                event_id: "$create".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );

        let result = chain.build_auth_chain_with_cache(&events, "$create");

        assert!(!result.is_empty());
    }

    #[test]
    fn test_verify_event_auth_chain_complete_empty_chain() {
        let chain = EventAuthChain::new();
        let events: HashMap<String, EventData> = HashMap::new();

        let result = chain.verify_event_auth_chain_complete(&events, "!room:test", "$1", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_event_auth_chain_complete_event_not_found() {
        let chain = EventAuthChain::new();
        let events: HashMap<String, EventData> = HashMap::new();

        let result = chain.verify_event_auth_chain_complete(&events, "!room:test", "$1", &["$1".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_event_auth_chain_complete_room_mismatch() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events.insert(
            "$1".to_string(),
            EventData {
                event_id: "$1".to_string(),
                room_id: "!room:other".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );

        let result = chain.verify_event_auth_chain_complete(&events, "!room:test", "$1", &["$1".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_event_auth_chain_complete_success() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events.insert(
            "$create".to_string(),
            EventData {
                event_id: "$create".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.create".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: None,
            },
        );
        events.insert(
            "$member".to_string(),
            EventData {
                event_id: "$member".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.member".to_string(),
                auth_events: vec!["$create".to_string()],
                prev_events: vec!["$create".to_string()],
                state_key: Some(json!("@user:test")),
                content: None,
            },
        );

        let result = chain.verify_event_auth_chain_complete(
            &events,
            "!room:test",
            "$member",
            &["$create".to_string(), "$member".to_string()],
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_state_with_auth_chain() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events.insert(
            "$1".to_string(),
            EventData {
                event_id: "$1".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.name".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: Some(json!("")),
                content: Some(json!({"name": "Test Room"})),
            },
        );

        let state = chain.resolve_state_with_auth_chain(&events, &["$1"]);

        assert!(state.contains_key("m.room.name:"));
    }

    #[test]
    fn test_resolve_state_with_auth_chain_empty_state_key() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events.insert(
            "$1".to_string(),
            EventData {
                event_id: "$1".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.message".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: None,
                content: Some(json!({"body": "hello"})),
            },
        );

        let state = chain.resolve_state_with_auth_chain(&events, &["$1"]);

        // No state_key means no state entry
        assert!(state.is_empty());
    }

    #[test]
    fn test_calculate_state_id() {
        let chain = EventAuthChain::new();
        let mut state: HashMap<String, &Value> = HashMap::new();
        let value = json!("value1");
        state.insert("key1".to_string(), &value);

        let id = chain.calculate_state_id("!room:test", &state);

        assert!(!id.is_empty());
    }

    #[test]
    fn test_calculate_state_id_empty() {
        let chain = EventAuthChain::new();
        let state: HashMap<String, &Value> = HashMap::new();

        let id = chain.calculate_state_id("!room:test", &state);

        assert!(!id.is_empty());
    }

    #[test]
    fn test_detect_state_conflicts_advanced_no_power_levels() {
        let chain = EventAuthChain::new();
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000,
                "sender": "@alice:test"
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 2000,
                "sender": "@bob:test"
            }),
        ];

        let conflicts = chain.detect_state_conflicts_advanced(&state_events, None);

        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn test_detect_state_conflicts_advanced_with_power_levels() {
        let chain = EventAuthChain::new();
        let state_events = vec![
            json!({
                "event_id": "$1",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 1000,
                "sender": "@alice:test"
            }),
            json!({
                "event_id": "$2",
                "type": "m.room.name",
                "state_key": "!room:test",
                "origin_server_ts": 2000,
                "sender": "@bob:test"
            }),
        ];

        let mut power_levels = HashMap::new();
        power_levels.insert("@alice:test".to_string(), 100);
        power_levels.insert("@bob:test".to_string(), 50);

        let conflicts = chain.detect_state_conflicts_advanced(&state_events, Some(&power_levels));

        assert_eq!(conflicts.len(), 1);
        // Alice has higher power
        assert_eq!(conflicts[0].winning_event, "$1");
    }

    #[test]
    fn test_detect_state_conflicts_advanced_no_conflicts() {
        let chain = EventAuthChain::new();
        let state_events = vec![json!({
            "event_id": "$1",
            "type": "m.room.name",
            "state_key": "!room:test",
            "origin_server_ts": 1000
        })];

        let conflicts = chain.detect_state_conflicts_advanced(&state_events, None);

        assert!(conflicts.is_empty());
    }
}
