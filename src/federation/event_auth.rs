use lru::LruCache;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;

const AUTH_CHAIN_CACHE_SIZE: usize = 1000;
const DEPTH_CACHE_SIZE: usize = 2000;
const STATE_RESOLUTION_MAX_HOPS: usize = 100;

type StateKey = String;
type StateEntry = (i64, String, i64, Option<String>);
type StateByKey = HashMap<StateKey, Vec<StateEntry>>;

#[derive(Debug, Clone)]
pub struct EventAuthChain {
    auth_chain_cache: Arc<RwLock<LruCache<String, bool>>>,
    depth_cache: Arc<RwLock<LruCache<String, i64>>>,
}

impl Default for EventAuthChain {
    fn default() -> Self {
        Self::new()
    }
}

impl EventAuthChain {
    pub fn new() -> Self {
        Self {
            auth_chain_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(AUTH_CHAIN_CACHE_SIZE)
                    .expect("AUTH_CHAIN_CACHE_SIZE is non-zero"),
            ))),
            depth_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(DEPTH_CACHE_SIZE).expect("DEPTH_CACHE_SIZE is non-zero"),
            ))),
        }
    }

    pub async fn get_cached_auth_chain(&self, event_id: &str) -> Option<bool> {
        let mut cache = self.auth_chain_cache.write().await;
        cache.get(event_id).copied()
    }

    pub async fn cache_auth_chain_result(&self, event_id: &str, result: bool) {
        let mut cache = self.auth_chain_cache.write().await;
        cache.put(event_id.to_string(), result);
    }

    pub async fn get_cached_depth(&self, event_id: &str) -> Option<i64> {
        let mut cache = self.depth_cache.write().await;
        cache.get(event_id).copied()
    }

    pub async fn cache_depth(&self, event_id: &str, depth: i64) {
        let mut cache = self.depth_cache.write().await;
        cache.put(event_id.to_string(), depth);
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

    pub fn build_auth_chain_from_events(
        &self,
        events: &HashMap<String, EventData>,
        event_id: &str,
    ) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut auth_chain = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back(event_id.to_string());

        while let Some(current_event_id) = queue.pop_front() {
            if visited.contains(&current_event_id) {
                continue;
            }
            visited.insert(current_event_id.clone());

            if let Some(event) = events.get(&current_event_id) {
                if Self::is_auth_event(&event.event_type) {
                    auth_chain.push(current_event_id.clone());
                }

                for auth_event_id in &event.auth_events {
                    if !visited.contains(auth_event_id) {
                        queue.push_back(auth_event_id.clone());
                    }
                }
            }
        }

        auth_chain.sort();
        auth_chain
    }

    pub fn verify_auth_chain(
        &self,
        events: &HashMap<String, EventData>,
        room_id: &str,
        auth_chain: &[String],
    ) -> bool {
        if auth_chain.is_empty() {
            return false;
        }

        let mut seen_events = HashSet::new();

        for event_id in auth_chain {
            match events.get(event_id) {
                Some(event) => {
                    if event.room_id != room_id {
                        return false;
                    }
                    seen_events.insert(event_id.clone());
                }
                None => {
                    if auth_chain[0] != *event_id {
                        return false;
                    }
                }
            }
        }

        true
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
                                    graph
                                        .entry(prev_id.to_string())
                                        .or_default()
                                        .push(event.event_id.clone());

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

    pub fn detect_conflicts(&self, state_events: &[Value]) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();
        let mut state_by_key: HashMap<String, Vec<(i64, String)>> = HashMap::new();

        for event in state_events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let state_key = event
                .get("state_key")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let event_id = event.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let origin_server_ts = event
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            if state_key.is_empty() {
                continue;
            }

            let key = format!("{}:{}", event_type, state_key);
            state_by_key
                .entry(key.clone())
                .or_default()
                .push((origin_server_ts, event_id.to_string()));
        }

        for (key, events) in &state_by_key {
            if events.len() > 1 {
                let mut sorted_events = events.clone();
                // Sort by timestamp descending, then by event_id ascending for stable ordering
                sorted_events.sort_by(|a, b| {
                    let cmp = b.0.cmp(&a.0); // timestamp descending
                    if cmp == std::cmp::Ordering::Equal {
                        a.1.cmp(&b.1) // event_id ascending
                    } else {
                        cmp
                    }
                });

                let winner = &sorted_events[0];
                let losers: Vec<String> = sorted_events[1..]
                    .iter()
                    .map(|(_, eid)| eid.clone())
                    .collect();

                conflicts.push(ConflictInfo {
                    state_key: key.clone(),
                    winning_event: winner.1.clone(),
                    losing_events: losers,
                    resolution_reason: "Timestamp-based resolution: selected most recent event"
                        .to_string(),
                });
            }
        }

        conflicts
    }

    pub fn resolve_conflicts_power_based(
        &self,
        state_events: &[Value],
        power_levels: &HashMap<String, i64>,
    ) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();
        let mut state_by_key: HashMap<String, Vec<(i64, String, i64)>> = HashMap::new();

        for event in state_events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let state_key = event
                .get("state_key")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let event_id = event.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let origin_server_ts = event
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");

            if state_key.is_empty() {
                continue;
            }

            let sender_power = power_levels.get(sender).copied().unwrap_or(0);
            let key = format!("{}:{}", event_type, state_key);
            state_by_key.entry(key.clone()).or_default().push((
                origin_server_ts,
                event_id.to_string(),
                sender_power,
            ));
        }

        for (key, events) in &state_by_key {
            if events.len() > 1 {
                let mut sorted_events = events.clone();
                sorted_events.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)));

                let winner = &sorted_events[0];
                let losers: Vec<String> = sorted_events[1..]
                    .iter()
                    .map(|(_, eid, _)| eid.clone())
                    .collect();

                let reason = if winner.2 > 0 {
                    format!("Power-based resolution: sender power={}", winner.2)
                } else {
                    "Timestamp-based resolution: equal power levels".to_string()
                };

                conflicts.push(ConflictInfo {
                    state_key: key.clone(),
                    winning_event: winner.1.clone(),
                    losing_events: losers,
                    resolution_reason: reason,
                });
            }
        }

        conflicts
    }

    pub async fn calculate_event_depth_with_cache(
        &self,
        events: &[EventInfo],
        event_id: &str,
    ) -> Option<i64> {
        let cache_key = format!("depth:{}", event_id);

        if let Some(cached) = self.get_cached_depth(&cache_key).await {
            return Some(cached);
        }

        let depth_map = self.calculate_event_depth(events);

        if let Some(&depth) = depth_map.get(event_id) {
            self.cache_depth(&cache_key, depth).await;
            Some(depth)
        } else {
            None
        }
    }

    pub async fn build_auth_chain_with_cache(
        &self,
        events: &HashMap<String, EventData>,
        event_id: &str,
    ) -> Vec<String> {
        let cache_key = format!("auth_chain:{}", event_id);

        let cached_result: Option<bool> = self.get_cached_auth_chain(&cache_key).await;

        if cached_result.is_some() {
            tracing::debug!("Auth chain cache hit for {}", event_id);
        }

        let result = self.build_auth_chain_from_events(events, event_id);

        self.cache_auth_chain_result(&cache_key, !result.is_empty())
            .await;

        result
    }

    pub async fn verify_event_auth_chain_complete(
        &self,
        events: &HashMap<String, EventData>,
        room_id: &str,
        event_id: &str,
        auth_chain: &[String],
    ) -> Result<bool, &'static str> {
        if auth_chain.is_empty() {
            return Err("Empty auth chain");
        }

        let mut expected_auth_events = HashSet::new();
        for eid in auth_chain {
            expected_auth_events.insert(eid.as_str());
        }

        if let Some(event) = events.get(event_id) {
            if event.room_id != room_id {
                return Err("Event room_id mismatch");
            }

            let mut auth_set: HashSet<String> = HashSet::new();
            let mut queue: VecDeque<String> = VecDeque::new();
            queue.push_back(event_id.to_string());

            let mut hops = 0;
            while let Some(current_id) = queue.pop_front() {
                if hops > STATE_RESOLUTION_MAX_HOPS {
                    return Err("Auth chain verification exceeded max hops");
                }

                if let Some(current_event) = events.get(&current_id) {
                    if Self::is_auth_event(&current_event.event_type) {
                        auth_set.insert(current_id.clone());
                    }

                    for auth_eid in &current_event.auth_events {
                        if expected_auth_events.contains(&auth_eid.as_str())
                            && !auth_set.contains(auth_eid.as_str())
                        {
                            auth_set.insert(auth_eid.clone());
                            queue.push_back(auth_eid.clone());
                        }
                    }
                }
                hops += 1;
            }

            let missing: Vec<String> = expected_auth_events
                .iter()
                .filter(|&&eid| !auth_set.contains(eid))
                .map(|&eid| eid.to_string())
                .collect();

            if !missing.is_empty() {
                tracing::warn!("Missing auth events in chain: {:?}", missing);
                return Err("Auth chain verification failed: missing events");
            }

            Ok(true)
        } else {
            Err("Event not found")
        }
    }

    pub fn resolve_state_with_auth_chain<'a>(
        &'a self,
        events: &'a HashMap<String, EventData>,
        event_ids: &[&'a str],
    ) -> HashMap<String, &'a Value> {
        let mut state: HashMap<String, &Value> = HashMap::new();
        let mut processed = HashSet::new();
        let mut queue: VecDeque<&str> = event_ids.iter().copied().collect();
        let mut hops = 0;

        while let Some(event_id) = queue.pop_front() {
            if hops > STATE_RESOLUTION_MAX_HOPS * 10 {
                tracing::warn!("State resolution exceeded max hops, stopping");
                break;
            }

            if processed.contains(event_id) {
                continue;
            }
            processed.insert(event_id);

            if let Some(event) = events.get(event_id) {
                if let Some(state_key) = event.state_key.as_ref() {
                    let state_key_str = state_key.as_str().unwrap_or("");
                    // Empty state_key is valid for events like m.room.name
                    if let Some(content) = event.content.as_ref() {
                        state.insert(format!("{}:{}", event.event_type, state_key_str), content);
                    }
                }

                for auth_eid in &event.auth_events {
                    if !processed.contains(auth_eid.as_str()) {
                        queue.push_back(auth_eid);
                    }
                }
            }
            hops += 1;
        }

        state
    }

    pub fn calculate_state_id(&self, room_id: &str, state: &HashMap<String, &Value>) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();

        let mut state_entries: Vec<_> = state.iter().collect();
        state_entries.sort_by_key(|&(k, _)| k);

        for (key, value) in state_entries {
            hasher.update(key.as_bytes());
            if let Ok(json_str) = serde_json::to_string(value) {
                hasher.update(json_str.as_bytes());
            }
        }

        let room_id_bytes = room_id.as_bytes();
        hasher.update(room_id_bytes);

        let result = hasher.finalize();
        format!(
            "{:032x}:{}",
            u128::from_le_bytes(result[..16].try_into().unwrap_or([0u8; 16])),
            u128::from_le_bytes(result[16..].try_into().unwrap_or([0u8; 16]))
        )
    }

    pub async fn detect_state_conflicts_advanced(
        &self,
        state_events: &[Value],
        power_levels: Option<&HashMap<String, i64>>,
    ) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();
        let mut state_by_key: StateByKey = HashMap::new();

        for event in state_events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let state_key = event
                .get("state_key")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let event_id = event.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let origin_server_ts = event
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");

            if state_key.is_empty() {
                continue;
            }

            let sender_power = power_levels
                .and_then(|pl| pl.get(sender).copied())
                .unwrap_or(0);
            let content_json = serde_json::to_string(&event).ok();

            let key = format!("{}:{}", event_type, state_key);
            state_by_key.entry(key.clone()).or_default().push((
                origin_server_ts,
                event_id.to_string(),
                sender_power,
                content_json,
            ));
        }

        for (key, events) in &state_by_key {
            if events.len() > 1 {
                let mut sorted_events = events.clone();
                sorted_events.sort_by(|a, b| {
                    b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)).then_with(|| {
                        let content_a = &a.3;
                        let content_b = &b.3;
                        content_b.cmp(content_a)
                    })
                });

                let winner = &sorted_events[0];
                let winners_clone = winner.1.clone();
                let losers: Vec<String> = sorted_events[1..]
                    .iter()
                    .map(|(_, eid, _, _)| eid.clone())
                    .collect();

                let reason = if winner.2 > 0 {
                    format!(
                        "Power-based resolution: sender={}, power={}, ts={}",
                        winner.1, winner.2, winner.0
                    )
                } else if winner.0 > 0 {
                    format!("Timestamp-based resolution: ts={}", winner.0)
                } else {
                    "Default resolution: first event selected".to_string()
                };

                let reason_clone = reason.clone();
                let _resolution_details: HashMap<String, Value> = sorted_events
                    .iter()
                    .enumerate()
                    .map(|(i, (_, eid, power, content))| {
                        let mut detail = serde_json::Map::new();
                        detail.insert("event_id".to_string(), json!(eid));
                        detail.insert("power".to_string(), json!(power));
                        detail.insert(
                            "timestamp".to_string(),
                            json!(winner.0 == sorted_events[i].0),
                        );
                        if let Some(c) = content {
                            if let Ok(v) = serde_json::from_str(c) {
                                detail.insert("content".to_string(), v);
                            }
                        }
                        (format!("rank_{}", i), Value::Object(detail))
                    })
                    .collect();

                let losers_clone = losers.clone();
                conflicts.push(ConflictInfo {
                    state_key: key.clone(),
                    winning_event: winner.1.clone(),
                    losing_events: losers,
                    resolution_reason: reason,
                });

                tracing::debug!(
                    "State conflict resolved for {}: winner={}, losers={:?}, reason={}",
                    key,
                    winners_clone,
                    losers_clone,
                    reason_clone
                );
            }
        }

        conflicts
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
    use serde_json::json;

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
            EventInfo {
                event_id: "$1".to_string(),
                prev_events: None,
            },
            EventInfo {
                event_id: "$2".to_string(),
                prev_events: Some(serde_json::json!([["$1", None::<bool>]])),
            },
            EventInfo {
                event_id: "$3".to_string(),
                prev_events: Some(serde_json::json!([["$2", None::<bool>]])),
            },
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

        let result = EventAuthChain::new().verify_auth_chain(
            &events,
            "!room:test",
            &["$create".to_string()],
        );

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

        let result = EventAuthChain::new().verify_auth_chain(
            &events,
            "!room:test",
            &["$create".to_string()],
        );

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
        assert!(chain.auth_chain_cache.try_read().is_ok());
        assert!(chain.depth_cache.try_read().is_ok());
    }

    #[tokio::test]
    async fn test_cache_auth_chain() {
        let chain = EventAuthChain::new();

        // Test cache miss
        let result = chain.get_cached_auth_chain("$test").await;
        assert!(result.is_none());

        // Test cache set and hit
        chain.cache_auth_chain_result("$test", true).await;
        let result = chain.get_cached_auth_chain("$test").await;
        assert_eq!(result, Some(true));
    }

    #[tokio::test]
    async fn test_cache_depth() {
        let chain = EventAuthChain::new();

        // Test cache miss
        let result = chain.get_cached_depth("$test").await;
        assert!(result.is_none());

        // Test cache set and hit
        chain.cache_depth("$test", 42).await;
        let result = chain.get_cached_depth("$test").await;
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
        let auth_chain =
            EventAuthChain::new().build_auth_chain_from_events(&events, "$nonexistent");
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
        let result = EventAuthChain::new().verify_auth_chain(
            &events,
            "!room:test",
            &["$create".to_string()],
        );
        assert!(result);
    }

    #[test]
    fn test_calculate_event_depth_multiple_roots() {
        let events = vec![
            EventInfo {
                event_id: "$1".to_string(),
                prev_events: None,
            },
            EventInfo {
                event_id: "$2".to_string(),
                prev_events: None,
            },
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
        let events = vec![EventInfo {
            event_id: "$1".to_string(),
            prev_events: Some(serde_json::json!({"invalid": "format"})),
        }];

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

        let conflicts =
            EventAuthChain::new().resolve_conflicts_power_based(&state_events, &power_levels);

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

        let conflicts =
            EventAuthChain::new().resolve_conflicts_power_based(&state_events, &power_levels);

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

        let conflicts =
            EventAuthChain::new().resolve_conflicts_power_based(&state_events, &power_levels);

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
        let info = EventInfo {
            event_id: "$1".to_string(),
            prev_events: None,
        };

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

    #[tokio::test]
    async fn test_calculate_event_depth_with_cache() {
        let chain = EventAuthChain::new();

        let events = vec![EventInfo {
            event_id: "$1".to_string(),
            prev_events: None,
        }];

        let result = chain.calculate_event_depth_with_cache(&events, "$1").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_calculate_event_depth_with_cache_miss() {
        let chain = EventAuthChain::new();

        let events: Vec<EventInfo> = vec![];

        let result = chain
            .calculate_event_depth_with_cache(&events, "$nonexistent")
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_build_auth_chain_with_cache() {
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

        let result = chain.build_auth_chain_with_cache(&events, "$create").await;

        // Should return auth chain (includes $create as it's auth event)
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_verify_event_auth_chain_complete_empty_chain() {
        let chain = EventAuthChain::new();
        let events: HashMap<String, EventData> = HashMap::new();

        let result = chain
            .verify_event_auth_chain_complete(&events, "!room:test", "$1", &[])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_event_auth_chain_complete_event_not_found() {
        let chain = EventAuthChain::new();
        let events: HashMap<String, EventData> = HashMap::new();

        let result = chain
            .verify_event_auth_chain_complete(&events, "!room:test", "$1", &["$1".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_event_auth_chain_complete_room_mismatch() {
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

        let result = chain
            .verify_event_auth_chain_complete(&events, "!room:test", "$1", &["$1".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_event_auth_chain_complete_success() {
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

        let result = chain
            .verify_event_auth_chain_complete(
                &events,
                "!room:test",
                "$member",
                &["$create".to_string(), "$member".to_string()],
            )
            .await;

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

    #[tokio::test]
    async fn test_detect_state_conflicts_advanced_no_power_levels() {
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

        let conflicts = chain
            .detect_state_conflicts_advanced(&state_events, None)
            .await;

        assert_eq!(conflicts.len(), 1);
    }

    #[tokio::test]
    async fn test_detect_state_conflicts_advanced_with_power_levels() {
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

        let conflicts = chain
            .detect_state_conflicts_advanced(&state_events, Some(&power_levels))
            .await;

        assert_eq!(conflicts.len(), 1);
        // Alice has higher power
        assert_eq!(conflicts[0].winning_event, "$1");
    }

    #[tokio::test]
    async fn test_detect_state_conflicts_advanced_no_conflicts() {
        let chain = EventAuthChain::new();
        let state_events = vec![json!({
            "event_id": "$1",
            "type": "m.room.name",
            "state_key": "!room:test",
            "origin_server_ts": 1000
        })];

        let conflicts = chain
            .detect_state_conflicts_advanced(&state_events, None)
            .await;

        assert!(conflicts.is_empty());
    }
}
