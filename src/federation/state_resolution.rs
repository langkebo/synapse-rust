use crate::federation::event_auth::{EventAuthChain, EventData};
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
