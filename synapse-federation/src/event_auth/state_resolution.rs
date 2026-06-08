use super::models::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};

impl EventAuthChain {
    pub fn detect_conflicts(&self, state_events: &[Value]) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();
        let mut state_by_key: HashMap<String, Vec<(i64, String)>> = HashMap::new();

        for event in state_events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let state_key = event.get("state_key").and_then(|v| v.as_str()).unwrap_or("");
            let event_id = event.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let origin_server_ts = event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);

            if state_key.is_empty() {
                continue;
            }

            let key = format!("{event_type}:{state_key}");
            state_by_key.entry(key.clone()).or_default().push((origin_server_ts, event_id.to_string()));
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
                let losers: Vec<String> = sorted_events[1..].iter().map(|(_, eid)| eid.clone()).collect();

                conflicts.push(ConflictInfo {
                    state_key: key.clone(),
                    winning_event: winner.1.clone(),
                    losing_events: losers,
                    resolution_reason: "Timestamp-based resolution: selected most recent event".to_string(),
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
            let state_key = event.get("state_key").and_then(|v| v.as_str()).unwrap_or("");
            let event_id = event.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let origin_server_ts = event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);
            let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");

            if state_key.is_empty() {
                continue;
            }

            let sender_power = power_levels.get(sender).copied().unwrap_or(0);
            let key = format!("{event_type}:{state_key}");
            state_by_key.entry(key.clone()).or_default().push((origin_server_ts, event_id.to_string(), sender_power));
        }

        for (key, events) in &state_by_key {
            if events.len() > 1 {
                let mut sorted_events = events.clone();
                sorted_events.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)));

                let winner = &sorted_events[0];
                let losers: Vec<String> = sorted_events[1..].iter().map(|(_, eid, _)| eid.clone()).collect();

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

    pub fn calculate_state_id(&self, _room_id: &str, state: &HashMap<String, &Value>) -> String {
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

        let room_id_bytes = _room_id.as_bytes();
        hasher.update(room_id_bytes);

        let result = hasher.finalize();
        format!(
            "{:032x}:{}",
            u128::from_le_bytes(result[..16].try_into().unwrap_or([0u8; 16])),
            u128::from_le_bytes(result[16..].try_into().unwrap_or([0u8; 16]))
        )
    }

    pub fn detect_state_conflicts_advanced(
        &self,
        state_events: &[Value],
        power_levels: Option<&HashMap<String, i64>>,
    ) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();
        let mut state_by_key: StateByKey = HashMap::new();

        for event in state_events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let state_key = event.get("state_key").and_then(|v| v.as_str()).unwrap_or("");
            let event_id = event.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let origin_server_ts = event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);
            let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");

            if state_key.is_empty() {
                continue;
            }

            let sender_power = power_levels.and_then(|pl| pl.get(sender).copied()).unwrap_or(0);
            let content_json = serde_json::to_string(&event).ok();

            let key = format!("{event_type}:{state_key}");
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
                let losers: Vec<String> = sorted_events[1..].iter().map(|(_, eid, _, _)| eid.clone()).collect();

                let reason = if winner.2 > 0 {
                    format!("Power-based resolution: sender={}, power={}, ts={}", winner.1, winner.2, winner.0)
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
                        detail.insert("timestamp".to_string(), json!(winner.0 == sorted_events[i].0));
                        if let Some(c) = content {
                            if let Ok(v) = serde_json::from_str(c) {
                                detail.insert("content".to_string(), v);
                            }
                        }
                        (format!("rank_{i}"), Value::Object(detail))
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

    pub fn calculate_auth_difference(
        &self,
        _events: &HashMap<String, EventData>,
        chain_a: &[String],
        chain_b: &[String],
    ) -> HashSet<String> {
        let set_a: HashSet<&str> = chain_a.iter().map(|s| s.as_str()).collect();
        let set_b: HashSet<&str> = chain_b.iter().map(|s| s.as_str()).collect();

        let diff_events: Vec<String> = set_a.symmetric_difference(&set_b).map(|s| s.to_string()).collect();
        let mut auth_diff: HashSet<String> = diff_events.into_iter().collect();

        let additional: Vec<String> = auth_diff
            .iter()
            .flat_map(|diff_id| match _events.get(diff_id.as_str()) {
                Some(event) => event.auth_events.clone(),
                None => Vec::new(),
            })
            .collect();

        for eid in additional {
            auth_diff.insert(eid);
        }

        auth_diff
    }

    pub fn sort_by_reverse_topological_power(
        &self,
        events: &HashMap<String, EventData>,
        event_ids: &[String],
        mainline: &[String],
        power_levels: &HashMap<String, i64>,
    ) -> Vec<String> {
        let mut sorted = event_ids.to_vec();
        let mainline_map: HashMap<&str, usize> =
            mainline.iter().enumerate().map(|(i, eid)| (eid.as_str(), i)).collect();

        let power_event = |eid: &str| -> i64 {
            if let Some(event) = events.get(eid) {
                if let Some(content) = &event.content {
                    if let Some(users) = content.get("users") {
                        if let Some(user_power) =
                            users.get(event.state_key.as_ref().and_then(|v| v.as_str()).unwrap_or(""))
                        {
                            return user_power.as_i64().unwrap_or(0);
                        }
                    }
                }
                power_levels.get(eid).copied().unwrap_or(0)
            } else {
                0
            }
        };

        sorted.sort_by(|a, b| {
            let power_a = power_event(a);
            let power_b = power_event(b);

            power_b
                .cmp(&power_a)
                .then_with(|| {
                    let ts_a =
                        events.get(a).and_then(|e| e.content.as_ref()?.get("origin_server_ts")?.as_i64()).unwrap_or(0);
                    let ts_b =
                        events.get(b).and_then(|e| e.content.as_ref()?.get("origin_server_ts")?.as_i64()).unwrap_or(0);
                    ts_a.cmp(&ts_b)
                })
                .then_with(|| {
                    let mainline_a = mainline_map.get(a.as_str()).copied().unwrap_or(usize::MAX);
                    let mainline_b = mainline_map.get(b.as_str()).copied().unwrap_or(usize::MAX);
                    mainline_a.cmp(&mainline_b)
                })
                .then_with(|| a.cmp(b))
        });

        sorted
    }

    pub fn resolve_state_v2(
        &self,
        state_sets: &[&HashMap<String, &Value>],
        events: &HashMap<String, EventData>,
    ) -> HashMap<String, Value> {
        let mut resolved: HashMap<String, Value> = HashMap::new();
        let mut unconflicted: HashMap<String, &Value> = HashMap::new();
        let mut conflicted_keys: HashSet<String> = HashSet::new();

        if state_sets.is_empty() {
            return resolved;
        }

        let first_set = state_sets[0];
        for key in first_set.keys() {
            let first_val = first_set.get(key).copied();
            let all_same = state_sets.iter().all(|s| {
                let a = s.get(key).copied();
                let b = first_val;
                a == b
            });

            if all_same {
                if let Some(val) = first_val {
                    unconflicted.insert(key.clone(), val);
                }
            } else {
                conflicted_keys.insert(key.clone());
            }
        }

        for (key, val) in &unconflicted {
            resolved.insert(key.clone(), (*val).clone());
        }

        for key in &conflicted_keys {
            let mut candidates: Vec<String> = Vec::new();
            for state_set in state_sets {
                if let Some(val) = state_set.get(key) {
                    if let Some(event_id) = val.get("event_id").and_then(|v| v.as_str()) {
                        candidates.push(event_id.to_string());
                    }
                }
            }

            if candidates.len() == 1 {
                if let Some(val) = state_sets[0].get(key) {
                    resolved.insert(key.clone(), (*val).clone());
                }
                continue;
            }

            let power_levels: HashMap<String, i64> = events
                .iter()
                .filter(|(_, e)| e.event_type == "m.room.power_levels")
                .map(|(eid, _)| (eid.clone(), 0))
                .collect();

            let room_create = events.iter().find(|(_, e)| e.event_type == "m.room.create").map(|(eid, _)| eid.clone());

            let mainline =
                if let Some(create_id) = &room_create { self.compute_mainline(events, create_id) } else { Vec::new() };

            let sorted = self.sort_by_reverse_topological_power(events, &candidates, &mainline, &power_levels);

            if let Some(winner) = sorted.first() {
                if let Some(event) = events.get(winner) {
                    if let Some(content) = &event.content {
                        resolved.insert(key.clone(), content.clone());
                    }
                }
            }
        }

        resolved
    }
}