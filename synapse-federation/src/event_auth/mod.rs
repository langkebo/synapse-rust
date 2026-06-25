pub mod chain;
pub mod models;
pub mod state_resolution;

pub use models::*;

use std::collections::{HashMap, VecDeque};

impl EventAuthChain {
    pub fn get_cached_auth_chain(&self, event_id: &str) -> Option<Vec<String>> {
        self.auth_chain_cache.get(event_id)
    }

    pub fn cache_auth_chain_result(&self, event_id: &str, chain: Vec<String>) {
        self.auth_chain_cache.insert(event_id.to_string(), chain);
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

        let test_chain = vec!["$event1".to_string(), "$event2".to_string()];
        chain.cache_auth_chain_result("$test", test_chain.clone());
        let result = chain.get_cached_auth_chain("$test");
        assert_eq!(result, Some(test_chain));
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
}
