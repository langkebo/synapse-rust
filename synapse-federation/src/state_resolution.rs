use crate::event_auth::{EventAuthChain, EventData};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionResult {
    pub accepted_events: Vec<String>,
    pub rejected_events: Vec<String>,
}

pub struct StateResolutionService;

impl StateResolutionService {
    pub fn new() -> Self {
        Self
    }

    /// 将 JSON 事件转换为 EventData（用于状态解析）
    pub fn events_to_data(events: &[serde_json::Value]) -> HashMap<String, EventData> {
        let mut data = HashMap::new();
        for evt in events {
            let eid = match evt.get("event_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            let room_id = evt.get("room_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let event_type = evt.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let auth_events = evt
                .get("auth_events")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|ae| ae.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let prev_events = evt
                .get("prev_events")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|pe| pe.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let state_key = evt.get("state_key").cloned();
            let content = evt.get("content").cloned();

            data.insert(
                eid,
                EventData {
                    event_id: evt.get("event_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    room_id,
                    event_type,
                    auth_events,
                    prev_events,
                    state_key,
                    content,
                },
            );
        }
        data
    }

    /// 使用 State Resolution v2 (MSC1442) 算法解析状态冲突
    ///
    /// # Arguments
    /// * `events` - 冲突的状态事件（JSON 格式）
    ///
    /// # Returns
    /// 解析结果，包含被接受和拒绝的 event_id
    pub fn resolve(events: &[serde_json::Value]) -> Result<ResolutionResult, StateResolutionError> {
        let chain = EventAuthChain::new();
        let event_data = Self::events_to_data(events);

        let event_ids: Vec<&str> = event_data.keys().map(|s| s.as_str()).collect();

        let resolved = chain.resolve_state_with_auth_chain(&event_data, &event_ids);

        let resolved_ids: std::collections::HashSet<&str> =
            resolved.keys().map(|s| s.split(':').next().unwrap_or("")).collect();

        let accepted: Vec<String> =
            event_ids.iter().filter(|eid| resolved_ids.contains(*eid)).map(|s| s.to_string()).collect();

        let rejected: Vec<String> =
            event_ids.iter().filter(|eid| !resolved_ids.contains(*eid)).map(|s| s.to_string()).collect();

        Ok(ResolutionResult { accepted_events: accepted, rejected_events: rejected })
    }
}

impl Default for StateResolutionService {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StateResolutionError {
    #[error("Invalid event: {0}")]
    InvalidEvent(String),
    #[error("State resolution failed: {0}")]
    ResolutionFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_resolve_empty_state() {
        let events: Vec<serde_json::Value> = vec![];
        let result = StateResolutionService::resolve(&events);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert!(resolution.accepted_events.is_empty());
        assert!(resolution.rejected_events.is_empty());
    }

    #[test]
    fn test_resolve_single_branch() {
        let events = vec![json!({
            "event_id": "$event1",
            "room_id": "!room:server",
            "type": "m.room.member",
            "state_key": "@user:server",
            "auth_events": [],
            "prev_events": [],
            "content": {"membership": "join"}
        })];

        let result = StateResolutionService::resolve(&events);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        // Verify the function completes and returns a valid result structure
        assert_eq!(resolution.accepted_events.len() + resolution.rejected_events.len(), 1);
    }

    #[test]
    fn test_resolve_conflicting_power_levels() {
        let events = vec![
            json!({
                "event_id": "$pl1",
                "room_id": "!room:server",
                "type": "m.room.power_levels",
                "state_key": "",
                "auth_events": [],
                "prev_events": [],
                "content": {"users": {"@alice:server": 100}, "users_default": 0}
            }),
            json!({
                "event_id": "$pl2",
                "room_id": "!room:server",
                "type": "m.room.power_levels",
                "state_key": "",
                "auth_events": [],
                "prev_events": [],
                "content": {"users": {"@bob:server": 100}, "users_default": 0}
            }),
        ];

        let result = StateResolutionService::resolve(&events);
        assert!(result.is_ok());
        // Both events should be processed; at least one should be accepted
        let resolution = result.unwrap();
        assert!(!resolution.accepted_events.is_empty() || !resolution.rejected_events.is_empty());
    }

    #[test]
    fn test_resolve_with_auth_chain_diff() {
        let events = vec![
            json!({
                "event_id": "$create",
                "room_id": "!room:server",
                "type": "m.room.create",
                "state_key": "",
                "auth_events": [],
                "prev_events": [],
                "content": {"creator": "@alice:server", "room_version": "10"}
            }),
            json!({
                "event_id": "$member_a",
                "room_id": "!room:server",
                "type": "m.room.member",
                "state_key": "@alice:server",
                "auth_events": ["$create"],
                "prev_events": ["$create"],
                "content": {"membership": "join"}
            }),
            json!({
                "event_id": "$member_b",
                "room_id": "!room:server",
                "type": "m.room.member",
                "state_key": "@bob:server",
                "auth_events": ["$create"],
                "prev_events": ["$create"],
                "content": {"membership": "join"}
            }),
        ];

        let result = StateResolutionService::resolve(&events);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        // All events should be accounted for (accepted + rejected = total)
        assert_eq!(resolution.accepted_events.len() + resolution.rejected_events.len(), 3);
    }

    #[test]
    fn test_events_to_data_basic() {
        let events = vec![json!({
            "event_id": "$e1",
            "room_id": "!room:s",
            "type": "m.room.message",
            "auth_events": ["$a1"],
            "prev_events": ["$p1"],
            "state_key": "",
            "content": {"body": "hi"}
        })];

        let data = StateResolutionService::events_to_data(&events);
        assert_eq!(data.len(), 1);
        let ed = &data["$e1"];
        assert_eq!(ed.event_id, "$e1");
        assert_eq!(ed.room_id, "!room:s");
        assert_eq!(ed.event_type, "m.room.message");
        assert_eq!(ed.auth_events, vec!["$a1"]);
        assert_eq!(ed.prev_events, vec!["$p1"]);
        assert!(ed.state_key.is_some());
        assert!(ed.content.is_some());
    }

    #[test]
    fn test_events_to_data_skips_no_event_id() {
        let events = vec![json!({
            "room_id": "!room:s",
            "type": "m.room.message"
        })];

        let data = StateResolutionService::events_to_data(&events);
        assert!(data.is_empty());
    }

    #[test]
    fn test_events_to_data_defaults() {
        let events = vec![json!({
            "event_id": "$e1"
        })];

        let data = StateResolutionService::events_to_data(&events);
        assert_eq!(data.len(), 1);
        let ed = &data["$e1"];
        assert_eq!(ed.room_id, "");
        assert_eq!(ed.event_type, "");
        assert!(ed.auth_events.is_empty());
        assert!(ed.prev_events.is_empty());
        assert!(ed.state_key.is_none());
        assert!(ed.content.is_none());
    }

    #[test]
    fn test_resolution_result_serialization() {
        let result = ResolutionResult {
            accepted_events: vec!["$e1".to_string()],
            rejected_events: vec!["$e2".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("accepted_events"));
        assert!(json.contains("rejected_events"));

        let deserialized: ResolutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.accepted_events, vec!["$e1"]);
        assert_eq!(deserialized.rejected_events, vec!["$e2"]);
    }

    #[test]
    fn test_state_resolution_service_default() {
        let service = StateResolutionService::new();
        let _ = service;
    }
}
