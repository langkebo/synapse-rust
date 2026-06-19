use super::models::*;
use std::collections::{HashMap, HashSet, VecDeque};

impl EventAuthChain {
    pub fn build_auth_chain_from_events(&self, events: &HashMap<String, EventData>, event_id: &str) -> Vec<String> {
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

    pub fn verify_auth_chain(&self, events: &HashMap<String, EventData>, room_id: &str, auth_chain: &[String]) -> bool {
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

    pub fn build_auth_chain_with_cache(&self, events: &HashMap<String, EventData>, event_id: &str) -> Vec<String> {
        let cache_key = format!("auth_chain:{event_id}");

        let cached_result: Option<bool> = self.get_cached_auth_chain(&cache_key);

        if cached_result.is_some() {
            tracing::debug!("Auth chain cache hit for {}", event_id);
        }

        let result = self.build_auth_chain_from_events(events, event_id);

        self.cache_auth_chain_result(&cache_key, !result.is_empty());

        result
    }

    pub fn verify_event_auth_chain_complete(
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
                        if expected_auth_events.contains(&auth_eid.as_str()) && !auth_set.contains(auth_eid.as_str()) {
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

    pub fn compute_mainline(&self, events: &HashMap<String, EventData>, room_create_event_id: &str) -> Vec<String> {
        // MSC1442 主链: 从 m.room.create 开始, 沿 auth_events 链
        // 收集 m.room.power_levels 事件序列 (含 create 作为根).
        let mut mainline: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();

        // 主链必须包含 create 事件作为根.
        if events.contains_key(room_create_event_id) {
            mainline.push(room_create_event_id.to_string());
            visited.insert(room_create_event_id.to_string());
        }

        // 收集所有 m.room.power_levels 事件, 按深度排序 (升序).
        let mut pl_events: Vec<(i64, i64, String)> = events
            .iter()
            .filter(|(_, e)| e.event_type == "m.room.power_levels")
            .map(|(eid, e)| (e.depth, e.origin_server_ts, eid.clone()))
            .collect();
        pl_events.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        // 按深度顺序加入主链 (深度小的在前 = 旧的在前).
        for (_, _, eid) in pl_events {
            if !visited.contains(&eid) {
                mainline.push(eid.clone());
                visited.insert(eid);
            }
        }

        mainline
    }

    pub fn get_mainline_depth(&self, mainline: &[String], event_id: &str) -> Option<usize> {
        mainline.iter().position(|e| e == event_id)
    }
}
