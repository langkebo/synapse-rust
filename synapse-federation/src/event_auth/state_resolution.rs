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

        // 返回事件发送者的 power level: 优先用 power_levels 映射 (user_id -> power),
        // 否则尝试从事件 content.users 读取 (针对 m.room.power_levels 事件自身).
        let power_of = |eid: &str| -> i64 {
            if let Some(event) = events.get(eid) {
                if let Some(pl) = power_levels.get(&event.sender).copied() {
                    return pl;
                }
                // 对于 m.room.power_levels 事件自身, 其 sender 的 power 可能在自己的 content.users 中.
                if event.event_type == "m.room.power_levels" {
                    if let Some(content) = &event.content {
                        if let Some(users) = content.get("users").and_then(|u| u.as_object()) {
                            if let Some(user_power) = users.get(&event.sender) {
                                return user_power.as_i64().unwrap_or(0);
                            }
                        }
                    }
                }
            }
            0
        };

        sorted.sort_by(|a, b| {
            let power_a = power_of(a);
            let power_b = power_of(b);

            power_b
                .cmp(&power_a)
                .then_with(|| {
                    let ts_a = events.get(a).map(|e| e.origin_server_ts).unwrap_or(0);
                    let ts_b = events.get(b).map(|e| e.origin_server_ts).unwrap_or(0);
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

        if conflicted_keys.is_empty() {
            return resolved;
        }

        // 收集所有冲突事件 (event_id), 按状态键分组.
        let mut conflicted_events_by_key: HashMap<String, Vec<String>> = HashMap::new();
        for key in &conflicted_keys {
            let mut candidates: Vec<String> = Vec::new();
            for state_set in state_sets {
                if let Some(val) = state_set.get(key) {
                    if let Some(event_id) = val.get("event_id").and_then(|v| v.as_str()) {
                        if !candidates.contains(&event_id.to_string()) {
                            candidates.push(event_id.to_string());
                        }
                    }
                }
            }
            conflicted_events_by_key.insert(key.clone(), candidates);
        }

        // 单候选的键直接采用.
        let mut multi_conflict_keys: Vec<String> = Vec::new();
        for (key, candidates) in &conflicted_events_by_key {
            if candidates.len() == 1 {
                if let Some(event) = events.get(&candidates[0]) {
                    if let Some(content) = &event.content {
                        resolved.insert(key.clone(), content.clone());
                    }
                }
            } else {
                multi_conflict_keys.push(key.clone());
            }
        }

        if multi_conflict_keys.is_empty() {
            return resolved;
        }

        // P0-11: power_levels 映射必须是 user_id -> power_level,
        // 从冲突集合中最新的 m.room.power_levels 事件 content.users 提取.
        let power_levels: HashMap<String, i64> = {
            let mut pl_events: Vec<&EventData> =
                events.values().filter(|e| e.event_type == "m.room.power_levels").collect();
            // 按深度排序, 取最深 (最新) 的 power_levels 事件作为基准.
            pl_events.sort_by(|a, b| b.depth.cmp(&a.depth).then_with(|| b.origin_server_ts.cmp(&a.origin_server_ts)));
            let mut map: HashMap<String, i64> = HashMap::new();
            if let Some(pl_event) = pl_events.first() {
                if let Some(content) = &pl_event.content {
                    if let Some(users) = content.get("users").and_then(|u| u.as_object()) {
                        for (user_id, power) in users {
                            if let Some(p) = power.as_i64() {
                                map.insert(user_id.clone(), p);
                            }
                        }
                    }
                }
            }
            map
        };

        // 构建主链 (mainline): 从 m.room.create 开始, 沿 auth_events 链
        // 收集 m.room.power_levels 事件序列.
        let room_create = events.iter().find(|(_, e)| e.event_type == "m.room.create").map(|(eid, _)| eid.clone());
        let mainline =
            if let Some(create_id) = &room_create { self.compute_mainline(events, create_id) } else { Vec::new() };

        // 将冲突事件分为 auth 事件和非 auth 事件.
        let auth_event_types: &[&str] = &[
            "m.room.create",
            "m.room.member",
            "m.room.power_levels",
            "m.room.join_rules",
            "m.room.history_visibility",
        ];

        let is_auth_event = |eid: &str| -> bool {
            events.get(eid).map(|e| auth_event_types.contains(&e.event_type.as_str())).unwrap_or(false)
        };

        // 收集所有多候选冲突事件的 event_id.
        let all_conflicted_eids: Vec<String> = multi_conflict_keys
            .iter()
            .flat_map(|k| conflicted_events_by_key.get(k).cloned().unwrap_or_default())
            .collect();

        let auth_eids: Vec<String> = all_conflicted_eids.iter().filter(|e| is_auth_event(e)).cloned().collect();
        let non_auth_eids: Vec<String> = all_conflicted_eids.iter().filter(|e| !is_auth_event(e)).cloned().collect();

        // 解析 auth 事件: 使用 reverse topological power ordering.
        let sorted_auth = self.sort_by_reverse_topological_power(events, &auth_eids, &mainline, &power_levels);

        // 解析非 auth 事件: 使用 mainline ordering.
        let sorted_non_auth = self.sort_by_reverse_topological_power(events, &non_auth_eids, &mainline, &power_levels);

        // 合并排序结果, auth 事件优先, 然后非 auth 事件.
        // 对每个状态键, 第一个出现的事件获胜.
        let mut ordered_all: Vec<String> = sorted_auth;
        ordered_all.extend(sorted_non_auth);

        // 按排序顺序填充 resolved: 对每个状态键, 第一个匹配的事件获胜.
        for key in &multi_conflict_keys {
            let candidates = conflicted_events_by_key.get(key).cloned().unwrap_or_default();
            // 在 ordered_all 中找到第一个属于该 key 候选集的事件.
            if let Some(winner_id) = ordered_all.iter().find(|eid| candidates.contains(eid)) {
                if let Some(event) = events.get(winner_id) {
                    if let Some(content) = &event.content {
                        resolved.insert(key.clone(), content.clone());
                    }
                }
            }
        }

        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state_event(event_type: &str, state_key: &str, event_id: &str, origin_server_ts: i64) -> Value {
        json!({
            "type": event_type,
            "state_key": state_key,
            "event_id": event_id,
            "origin_server_ts": origin_server_ts,
            "sender": "@alice:ex.com",
            "content": {"body": "test"},
        })
    }

    // ── detect_conflicts ──────────────────────────────────────────────

    #[test]
    fn detect_conflicts_single_event_no_conflict() {
        let chain = EventAuthChain::new();
        let events = vec![make_state_event("m.room.name", "key1", "$e1", 1000)];
        let conflicts = chain.detect_conflicts(&events);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_two_events_same_key_conflict() {
        let chain = EventAuthChain::new();
        let events = vec![
            make_state_event("m.room.name", "key1", "$old", 1000),
            make_state_event("m.room.name", "key1", "$new", 2000),
        ];
        let conflicts = chain.detect_conflicts(&events);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].winning_event, "$new"); // higher timestamp wins
        assert_eq!(conflicts[0].losing_events, vec!["$old"]);
    }

    #[test]
    fn detect_conflicts_different_keys_no_conflict() {
        let chain = EventAuthChain::new();
        let events = vec![
            make_state_event("m.room.name", "key1", "$e1", 1000),
            make_state_event("m.room.topic", "key2", "$e2", 1000),
        ];
        let conflicts = chain.detect_conflicts(&events);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_empty_state_key_skipped() {
        let chain = EventAuthChain::new();
        let mut event = make_state_event("m.room.name", "key1", "$e1", 1000);
        event["state_key"] = json!("");
        let events = vec![event];
        let conflicts = chain.detect_conflicts(&events);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_timestamp_tiebreaker_by_event_id() {
        let chain = EventAuthChain::new();
        let events = vec![
            make_state_event("m.room.name", "key1", "$b", 1000),
            make_state_event("m.room.name", "key1", "$a", 1000),
        ];
        let conflicts = chain.detect_conflicts(&events);
        assert_eq!(conflicts.len(), 1);
        // Same timestamp, sorted by timestamp desc then event_id ascending => $a wins
        assert_eq!(conflicts[0].winning_event, "$a");
        assert_eq!(conflicts[0].losing_events, vec!["$b"]);
    }

    // ── resolve_conflicts_power_based ─────────────────────────────────

    #[test]
    fn power_based_resolution_higher_power_wins() {
        let chain = EventAuthChain::new();
        let events = vec![
            {
                let mut e = make_state_event("m.room.name", "key1", "$low_power", 2000);
                e["sender"] = json!("@low:ex.com");
                e
            },
            {
                let mut e = make_state_event("m.room.name", "key1", "$high_power", 1000);
                e["sender"] = json!("@admin:ex.com");
                e
            },
        ];
        let mut power_levels = HashMap::new();
        power_levels.insert("@low:ex.com".into(), 0);
        power_levels.insert("@admin:ex.com".into(), 100);
        let conflicts = chain.resolve_conflicts_power_based(&events, &power_levels);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].winning_event, "$high_power");
    }

    #[test]
    fn power_based_resolution_equal_power_uses_timestamp() {
        let chain = EventAuthChain::new();
        let events = vec![
            {
                let mut e = make_state_event("m.room.name", "key1", "$old", 1000);
                e["sender"] = json!("@a:ex.com");
                e
            },
            {
                let mut e = make_state_event("m.room.name", "key1", "$new", 2000);
                e["sender"] = json!("@b:ex.com");
                e
            },
        ];
        let mut power_levels = HashMap::new();
        power_levels.insert("@a:ex.com".into(), 0);
        power_levels.insert("@b:ex.com".into(), 0);
        let conflicts = chain.resolve_conflicts_power_based(&events, &power_levels);
        assert_eq!(conflicts[0].winning_event, "$new"); // equal power, higher ts wins
    }

    // ── calculate_state_id ─────────────────────────────────────────────

    #[test]
    fn calculate_state_id_is_deterministic() {
        let chain = EventAuthChain::new();
        let mut state: HashMap<String, &Value> = HashMap::new();
        let content = json!({"body": "hello"});
        state.insert("m.room.name:".into(), &content);
        let id1 = chain.calculate_state_id("!r:ex.com", &state);
        let id2 = chain.calculate_state_id("!r:ex.com", &state);
        assert_eq!(id1, id2);
    }

    #[test]
    fn calculate_state_id_differs_for_different_content() {
        let chain = EventAuthChain::new();
        let content_a = json!({"body": "a"});
        let content_b = json!({"body": "b"});
        let mut state_a: HashMap<String, &Value> = HashMap::new();
        state_a.insert("m.room.name:".into(), &content_a);
        let mut state_b: HashMap<String, &Value> = HashMap::new();
        state_b.insert("m.room.name:".into(), &content_b);
        let id1 = chain.calculate_state_id("!r:ex.com", &state_a);
        let id2 = chain.calculate_state_id("!r:ex.com", &state_b);
        assert_ne!(id1, id2);
    }

    // ── calculate_auth_difference ─────────────────────────────────────

    #[test]
    fn auth_difference_symmetric_returns_difference() {
        let chain = EventAuthChain::new();
        let events = HashMap::new();
        let chain_a: Vec<String> = vec!["$a".into(), "$b".into()];
        let chain_b: Vec<String> = vec!["$b".into(), "$c".into()];
        let diff = chain.calculate_auth_difference(&events, &chain_a, &chain_b);
        assert!(diff.contains("$a"));
        assert!(diff.contains("$c"));
        assert!(!diff.contains("$b"));
    }

    #[test]
    fn auth_difference_identical_chains_empty_diff() {
        let chain = EventAuthChain::new();
        let events = HashMap::new();
        let chain_a: Vec<String> = vec!["$a".into(), "$b".into()];
        let diff = chain.calculate_auth_difference(&events, &chain_a, &chain_a);
        assert!(diff.is_empty());
    }
}
