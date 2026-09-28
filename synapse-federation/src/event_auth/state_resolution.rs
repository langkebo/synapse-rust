use super::models::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};

/// Implementation of [`EventAuthChain`] methods.
impl EventAuthChain {
    /// See [`detect_conflicts`.
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

    /// See [`resolve_conflicts_power_based`.
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

    /// See [`resolve_state_with_auth_chain`
    pub fn resolve_state_with_auth_chain<'a>(
        &'a self,
        events: &'a HashMap<String, EventData>,
        event_ids: &[&'a str],
    ) -> HashMap<String, &'a Value> {
        let mut state: HashMap<String, &Value> = HashMap::new();
        // 每个状态槽位当前胜者的 (origin_server_ts, event_id)，用于确定性裁决：
        // 与 detect_conflicts 同一约定 —— 时间戳大者胜，平票时 event_id 小者胜。
        // 不带该裁决时，胜者由 HashMap 迭代/BFS 顺序决定，同输入可能产出不同结果。
        let mut slot_winners: HashMap<String, (i64, &str)> = HashMap::new();
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
                        let slot = format!("{}:{}", event.event_type, state_key_str);
                        let wins = match slot_winners.get(&slot) {
                            None => true,
                            Some(&(ts, eid)) => {
                                event.origin_server_ts > ts
                                    || (event.origin_server_ts == ts && event.event_id.as_str() < eid)
                            }
                        };
                        if wins {
                            slot_winners.insert(slot.clone(), (event.origin_server_ts, event.event_id.as_str()));
                            state.insert(slot, content);
                        }
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

    /// See [`calculate_state_id`.
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

    /// See [`detect_state_conflicts_advanced`.
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

    /// The **auth difference** of two full auth chains.
    ///
    /// Spec definition (state resolution, "Definitions"): *"The auth difference
    /// is calculated by first calculating the full auth chain for each state set
    /// `S_i` ... and then taking every event that doesn't appear in every auth
    /// chain. If `C_i` is the full auth chain of `S_i`, then the auth difference
    /// is `∪C_i − ∩C_i`."* For exactly two chains that is their symmetric
    /// difference — nothing more.
    ///
    /// This previously also inserted the `auth_events` of every differing event,
    /// which is **not** the definition: an event's auth events are already in
    /// that event's full auth chain, so the extra step could only add events that
    /// both chains share. Fixed here because F-2's `full_conflicted_set` unions
    /// this set and would otherwise replay events the spec does not select.
    pub fn calculate_auth_difference(
        &self,
        _events: &HashMap<String, EventData>,
        chain_a: &[String],
        chain_b: &[String],
    ) -> HashSet<String> {
        let set_a: HashSet<&str> = chain_a.iter().map(|s| s.as_str()).collect();
        let set_b: HashSet<&str> = chain_b.iter().map(|s| s.as_str()).collect();
        set_a.symmetric_difference(&set_b).map(|s| (*s).to_string()).collect()
    }

    /// The **conflicted state subgraph** (MSC4297 / state resolution v2.1).
    ///
    /// MSC4297: *"Starting from an event in the conflicted state set and
    /// following `auth_events` edges may lead to another event in the conflicted
    /// state set. The union of all such paths between any pair of events in the
    /// conflicted state set (including endpoints) forms a subgraph of the
    /// original `auth_event` graph, called the conflicted state subgraph."*
    ///
    /// Implemented literally: from every conflicted event, walk `auth_events`
    /// transitively carrying the path, and when a step lands on a conflicted
    /// event, every event on that path (endpoints included) joins the subgraph.
    /// `visited` is per start and only prevents re-expanding a node, which cannot
    /// lose a path: reaching a conflicted event is a property of the suffix from a
    /// node, and the walk continues *through* such a node after recording the path
    /// that reached it.
    ///
    /// The subgraph is what v2 adds to the conflicted set (see
    /// [`Self::full_conflicted_set`]); unused until the resolver replays events,
    /// which is the remaining half of F-2.
    pub fn conflicted_state_subgraph(
        &self,
        conflicted: &HashSet<String>,
        events: &HashMap<String, EventData>,
    ) -> HashSet<String> {
        // Endpoints are part of the subgraph by definition.
        let mut subgraph: HashSet<String> = conflicted.clone();

        for start in conflicted {
            let mut visited: HashSet<String> = HashSet::new();
            let mut stack: Vec<(String, Vec<String>)> = vec![(start.clone(), vec![start.clone()])];

            while let Some((current, path)) = stack.pop() {
                if !visited.insert(current.clone()) {
                    continue;
                }
                let Some(event) = events.get(&current) else { continue };

                for auth in &event.auth_events {
                    let mut next_path = path.clone();
                    next_path.push(auth.clone());
                    if conflicted.contains(auth) {
                        subgraph.extend(next_path.iter().cloned());
                    }
                    stack.push((auth.clone(), next_path));
                }
            }
        }

        subgraph
    }

    /// The `"type:state_key"` key an event occupies in a state map, when it is a
    /// state event.
    fn state_map_key(event: &EventData) -> Option<String> {
        let state_key = event.state_key.as_ref()?.as_str()?;
        Some(format!("{}:{}", event.event_type, state_key))
    }

    /// **Iterative auth checks** — the replay step shared by state resolution
    /// v2 and v2.1.
    ///
    /// Spec algorithm: walk `ordered_event_ids` in order; for each event, ensure
    /// the state it is checked against contains the keys its `auth_events` name
    /// — *"If a (event_type, state_key) key that is required for checking the
    /// authorization rules is not present in the state, then the appropriate
    /// state event from the event's `auth_events` is used if the auth event is
    /// not rejected"* — then, if the event is authorised, insert it into the
    /// state.
    ///
    /// # The v2 / v2.1 difference lives entirely in `start_state`
    ///
    /// MSC4297 Modification 1 changes only what this function is *seeded* with:
    /// v2 passes the **unconflicted** state map, v2.1 passes an **empty** map, so
    /// that the replay is driven by each event's own `auth_events` history rather
    /// than by a possibly-stale unconflicted state (the MSC's "Problem A"). This
    /// function therefore takes the start map as a parameter and makes no
    /// assumption about which one it gets — the caller owns that decision, which
    /// is exactly where v2.1 must be distinguishable from v2.
    ///
    /// # The authorisation predicate is injected
    ///
    /// `is_authorised` is the `_check_event_auth` half. This crate has no
    /// spec auth-rules engine that operates on a state *map* (`event_auth::rules`
    /// authorises a single inbound event against its own `auth_events`), so it is
    /// a parameter rather than a duplicated rule set: the resolver supplies the
    /// rules, this function owns the replay mechanics.
    ///
    /// `rejected` holds event IDs the caller already rejected; they are never
    /// used to fill a missing key, per the spec's "if the auth event is not
    /// rejected".
    pub fn iterative_auth_checks<F>(
        &self,
        ordered_event_ids: &[String],
        start_state: &HashMap<String, String>,
        events: &HashMap<String, EventData>,
        rejected: &HashSet<String>,
        is_authorised: F,
    ) -> HashMap<String, String>
    where
        F: Fn(&EventData, &HashMap<String, String>) -> bool,
    {
        let mut resolved = start_state.clone();

        for event_id in ordered_event_ids {
            let Some(event) = events.get(event_id) else { continue };

            // Fill every key the event's auth chain names, walking it
            // transitively: the auth event that carries a key may itself be
            // reached through another auth event.
            let mut stack: Vec<&String> = event.auth_events.iter().collect();
            let mut visited: HashSet<&str> = HashSet::new();
            while let Some(auth_id) = stack.pop() {
                if !visited.insert(auth_id.as_str()) || rejected.contains(auth_id) {
                    continue;
                }
                let Some(auth_event) = events.get(auth_id) else { continue };
                if let Some(key) = Self::state_map_key(auth_event) {
                    resolved.entry(key).or_insert_with(|| auth_id.clone());
                }
                for next in &auth_event.auth_events {
                    stack.push(next);
                }
            }

            if is_authorised(event, &resolved) {
                if let Some(key) = Self::state_map_key(event) {
                    resolved.insert(key, event_id.clone());
                }
            }
        }

        resolved
    }

    /// The **full conflicted set** (MSC4297 / state resolution v2.1).
    ///
    /// MSC4297 amends the v2 definition to: *"the union of the conflicted state
    /// set, **the conflicted state subgraph** and the auth difference."* The
    /// subgraph is the only part v2.1 adds; the other two terms are unchanged
    /// from v2.
    pub fn full_conflicted_set(
        &self,
        conflicted: &HashSet<String>,
        auth_difference: &HashSet<String>,
        events: &HashMap<String, EventData>,
    ) -> HashSet<String> {
        let mut full = self.conflicted_state_subgraph(conflicted, events);
        full.extend(auth_difference.iter().cloned());
        full
    }

    /// See [`sort_by_reverse_topological_power`.
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

    /// See [`resolve_state_v2`.
    /// Resolve conflicting state (state resolution v2.1, MSC4297).
    ///
    /// `is_authorised` is the `_check_event_auth` half injected by the caller —
    /// see [`Self::iterative_auth_checks`] for why it is a parameter.
    ///
    /// Mechanics: split the state sets into unconflicted and conflicted keys,
    /// build the **full conflicted set** (conflicted ∪ conflicted state subgraph
    /// ∪ auth difference), order it, **replay** it with iterative auth checks
    /// starting from an **empty** state map (v2.1 Modification 1), then overlay
    /// the unconflicted state (spec step 5).
    pub fn resolve_state_v2<F>(
        &self,
        state_sets: &[&HashMap<String, &Value>],
        events: &HashMap<String, EventData>,
        is_authorised: F,
    ) -> HashMap<String, Value>
    where
        F: Fn(&EventData, &HashMap<String, String>) -> bool,
    {
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

        // MSC4297: every candidate named by a conflicted key is conflicted —
        // including keys only one state set carries (present in one, absent in
        // another is a conflict too, and v2 replays it).
        let conflicted_set: HashSet<String> = conflicted_events_by_key.values().flatten().cloned().collect();
        if conflicted_set.is_empty() {
            return resolved;
        }

        // The **full conflicted set** (v2.1): conflicted ∪ conflicted state
        // subgraph ∪ auth difference. A state set's full auth chain is the union
        // of the auth chains of the events it contains.
        let full_auth_chain = |state_set: &HashMap<String, &Value>| -> Vec<String> {
            let mut chain: Vec<String> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for value in state_set.values() {
                let Some(event_id) = value.get("event_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                for id in self.build_auth_chain_from_events(events, event_id) {
                    if seen.insert(id.clone()) {
                        chain.push(id);
                    }
                }
            }
            chain
        };

        let auth_difference = if state_sets.len() >= 2 {
            self.calculate_auth_difference(events, &full_auth_chain(state_sets[0]), &full_auth_chain(state_sets[1]))
        } else {
            HashSet::new()
        };

        let all_conflicted_eids: Vec<String> =
            self.full_conflicted_set(&conflicted_set, &auth_difference, events).into_iter().collect();

        // P0-11: the power levels map (user_id -> level) from the deepest
        // power_levels event available.
        let power_levels: HashMap<String, i64> = {
            let mut pl_events: Vec<&EventData> =
                events.values().filter(|e| e.event_type == "m.room.power_levels").collect();
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

        // The mainline: the `m.room.power_levels` sequence reachable from
        // `m.room.create` along `auth_events`.
        let room_create = events.iter().find(|(_, e)| e.event_type == "m.room.create").map(|(eid, _)| eid.clone());
        let mainline =
            if let Some(create_id) = &room_create { self.compute_mainline(events, create_id) } else { Vec::new() };

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

        let auth_eids: Vec<String> = all_conflicted_eids.iter().filter(|e| is_auth_event(e)).cloned().collect();
        let non_auth_eids: Vec<String> = all_conflicted_eids.iter().filter(|e| !is_auth_event(e)).cloned().collect();

        // Conflicted power/auth events are ordered first (reverse topological
        // power ordering), then the rest (mainline ordering). Both currently use
        // the same comparator — sender power, timestamp, mainline position — which
        // approximates the two spec orderings; see the status doc §4.8.
        let sorted_auth = self.sort_by_reverse_topological_power(events, &auth_eids, &mainline, &power_levels);
        let sorted_non_auth = self.sort_by_reverse_topological_power(events, &non_auth_eids, &mainline, &power_levels);

        let mut ordered_all: Vec<String> = sorted_auth;
        ordered_all.extend(sorted_non_auth);

        // MSC4297 Modification 1: replay from an **empty** state map, so each
        // event is authorised from its own `auth_events` history rather than from
        // the (possibly stale) unconflicted state — "Problem A".
        let replayed =
            self.iterative_auth_checks(&ordered_all, &HashMap::new(), events, &HashSet::new(), is_authorised);

        // Spec step 5: the unconflicted state wins for the keys it covers.
        for (key, val) in &unconflicted {
            resolved.insert(key.clone(), (*val).clone());
        }

        // Materialise the replayed state, skipping the keys step 5 settled.
        for (key, event_id) in &replayed {
            if resolved.contains_key(key) {
                continue;
            }
            if let Some(content) = events.get(event_id).and_then(|event| event.content.as_ref()) {
                resolved.insert(key.clone(), content.clone());
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

    // ── MSC4297 (state resolution v2.1) set selection ─────────────────────

    /// `EventData` fixture: `id` authorised by `auth`, all in one room.
    fn event_data(id: &str, auth: &[&str]) -> EventData {
        EventData {
            event_id: id.to_string(),
            room_id: "!r:ex.com".to_string(),
            event_type: "m.room.member".to_string(),
            auth_events: auth.iter().map(|s| s.to_string()).collect(),
            prev_events: Vec::new(),
            state_key: Some(Value::String("@a:ex.com".to_string())),
            content: None,
            sender: "@a:ex.com".to_string(),
            origin_server_ts: 1,
            depth: 1,
        }
    }

    fn events_of(list: Vec<EventData>) -> HashMap<String, EventData> {
        list.into_iter().map(|e| (e.event_id.clone(), e)).collect()
    }

    /// MSC4297: "the union of all such paths between any pair of events in the
    /// conflicted state set (including endpoints)".
    ///
    /// Graph: `$c1 -> $x -> $c2` along `auth_events`, with `$c1`/`$c2` conflicted
    /// and `$x` neither. `$x` must join the subgraph — it is *between* two
    /// conflicted events — and the unrelated `$other` must not.
    #[test]
    fn conflicted_state_subgraph_includes_the_events_between_conflicted_events() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            event_data("$c1", &["$x"]),
            event_data("$x", &["$c2", "$other"]),
            event_data("$c2", &[]),
            event_data("$other", &[]),
        ]);
        let conflicted: HashSet<String> = ["$c1".to_string(), "$c2".to_string()].into_iter().collect();

        let subgraph = chain.conflicted_state_subgraph(&conflicted, &events);
        assert!(subgraph.contains("$c1") && subgraph.contains("$c2"), "endpoints are included: {subgraph:?}");
        assert!(subgraph.contains("$x"), "the event between two conflicted events is included: {subgraph:?}");
        assert!(
            !subgraph.contains("$other"),
            "an event on no path between conflicted events is excluded: {subgraph:?}"
        );
    }

    /// With a single conflicted event there is no *pair*, so the subgraph is just
    /// that event. This is the v2.1 boundary: the new term adds nothing.
    #[test]
    fn conflicted_state_subgraph_of_one_event_is_that_event() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![event_data("$c1", &["$x"]), event_data("$x", &[])]);
        let conflicted: HashSet<String> = ["$c1".to_string()].into_iter().collect();
        assert_eq!(chain.conflicted_state_subgraph(&conflicted, &events), conflicted);
    }

    /// The auth difference is exactly `(A ∪ B) − (A ∩ B)` — nothing more. The
    /// previous implementation also inserted the `auth_events` of differing
    /// events, which can only add events *both* chains already share.
    #[test]
    fn auth_difference_is_the_symmetric_difference_of_the_two_chains() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            event_data("$shared", &["$common"]),
            event_data("$common", &[]),
            // `$only_a` is authorised by `$common`, which both chains share: the
            // superseded implementation added exactly these `auth_events` to the
            // difference, so this fixture makes that regression visible.
            event_data("$only_a", &["$common"]),
            event_data("$only_b", &[]),
        ]);
        let a = vec!["$shared".to_string(), "$only_a".to_string(), "$common".to_string()];
        let b = vec!["$shared".to_string(), "$only_b".to_string(), "$common".to_string()];

        let diff = chain.calculate_auth_difference(&events, &a, &b);
        assert_eq!(diff, HashSet::from(["$only_a".to_string(), "$only_b".to_string()]));
        assert!(!diff.contains("$common"), "an event in both chains is not part of the difference");
        assert!(!diff.contains("$shared"));
    }

    /// Full conflicted set = conflicted ∪ subgraph ∪ auth difference (MSC4297).
    #[test]
    fn full_conflicted_set_is_the_union_of_the_three_terms() {
        let chain = EventAuthChain::new();
        let events =
            events_of(vec![event_data("$c1", &["$mid"]), event_data("$mid", &["$c2"]), event_data("$c2", &[])]);
        let conflicted: HashSet<String> = ["$c1".to_string(), "$c2".to_string()].into_iter().collect();
        let auth_difference: HashSet<String> = ["$auth_only".to_string()].into_iter().collect();

        let full = chain.full_conflicted_set(&conflicted, &auth_difference, &events);
        for expected in ["$c1", "$c2", "$mid", "$auth_only"] {
            assert!(full.contains(expected), "{expected} must be in the full conflicted set: {full:?}");
        }
        assert_eq!(full.len(), 4);
    }

    // ── MSC4297 Modification 1: iterative auth checks ─────────────────────

    fn member_event(id: &str, user: &str, membership: &str, auth: &[&str]) -> EventData {
        EventData {
            event_id: id.to_string(),
            room_id: "!r:ex.com".to_string(),
            event_type: "m.room.member".to_string(),
            auth_events: auth.iter().map(|s| s.to_string()).collect(),
            prev_events: Vec::new(),
            state_key: Some(Value::String(user.to_string())),
            content: Some(json!({ "membership": membership })),
            sender: user.to_string(),
            origin_server_ts: 1,
            depth: 1,
        }
    }

    fn join_rules_event(id: &str, sender: &str, auth: &[&str]) -> EventData {
        EventData {
            event_id: id.to_string(),
            room_id: "!r:ex.com".to_string(),
            event_type: "m.room.join_rules".to_string(),
            auth_events: auth.iter().map(|s| s.to_string()).collect(),
            prev_events: Vec::new(),
            state_key: Some(Value::String(String::new())),
            content: Some(json!({ "join_rule": "public" })),
            sender: sender.to_string(),
            origin_server_ts: 2,
            depth: 2,
        }
    }

    /// The injected predicate these tests use: an event is authorised when its
    /// sender is a **joined** member in the state it is checked against.
    fn sender_is_joined(
        events: &HashMap<String, EventData>,
    ) -> impl Fn(&EventData, &HashMap<String, String>) -> bool + '_ {
        move |event: &EventData, state: &HashMap<String, String>| {
            let key = format!("m.room.member:{}", event.sender);
            state
                .get(&key)
                .and_then(|id| events.get(id))
                .and_then(|m| m.content.as_ref())
                .and_then(|c| c.get("membership"))
                .and_then(|v| v.as_str())
                == Some("join")
        }
    }

    #[test]
    fn iterative_auth_checks_inserts_authorised_and_skips_unauthorised() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            member_event("$a_join", "@a:ex.com", "join", &[]),
            member_event("$b_leave", "@b:ex.com", "leave", &[]),
            join_rules_event("$rules_ok", "@a:ex.com", &["$a_join"]),
            join_rules_event("$rules_bad", "@b:ex.com", &["$b_leave"]),
        ]);
        let ordered = vec!["$rules_ok".to_string(), "$rules_bad".to_string()];
        let rejects: HashSet<String> = HashSet::new();

        let resolved =
            chain.iterative_auth_checks(&ordered, &HashMap::new(), &events, &rejects, sender_is_joined(&events));

        assert_eq!(resolved.get("m.room.join_rules:"), Some(&"$rules_ok".to_string()));
        assert!(
            !resolved.values().any(|id| id == "$rules_bad"),
            "an event whose sender is not joined must not be inserted: {resolved:?}"
        );
    }

    /// A missing `(type, state_key)` is filled from the event's own `auth_events`
    /// — the spec clause MSC4297 Modification 1 leans on.
    #[test]
    fn iterative_auth_checks_fills_missing_keys_from_auth_events() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            member_event("$a_join", "@a:ex.com", "join", &[]),
            join_rules_event("$rules", "@a:ex.com", &["$a_join"]),
        ]);
        let ordered = vec!["$rules".to_string()];
        let rejects: HashSet<String> = HashSet::new();

        let v21 = chain.iterative_auth_checks(&ordered, &HashMap::new(), &events, &rejects, sender_is_joined(&events));
        assert_eq!(v21.get("m.room.join_rules:"), Some(&"$rules".to_string()), "v2.1 empty start: {v21:?}");
    }

    /// **MSC4297 Problem A**, reproduced: the unconflicted state says the sender
    /// left, while both forks' `auth_events` agree they were joined. v2 seeds the
    /// replay with the unconflicted state and therefore unauthorises the event;
    /// v2.1 seeds it empty and the event's own auth history authorises it.
    #[test]
    fn empty_start_map_differs_from_the_unconflicted_start_map_problem_a() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            member_event("$a_join", "@a:ex.com", "join", &[]),
            member_event("$a_leave", "@a:ex.com", "leave", &["$a_join"]),
            join_rules_event("$rules", "@a:ex.com", &["$a_join"]),
        ]);
        let ordered = vec!["$rules".to_string()];
        let rejects: HashSet<String> = HashSet::new();

        // v2: start from the unconflicted state, which carries the stale leave.
        let mut unconflicted = HashMap::new();
        unconflicted.insert("m.room.member:@a:ex.com".to_string(), "$a_leave".to_string());
        let v2 = chain.iterative_auth_checks(&ordered, &unconflicted, &events, &rejects, sender_is_joined(&events));
        assert!(
            !v2.values().any(|id| id == "$rules"),
            "v2 unauthorises the event when the unconflicted state says the sender left: {v2:?}"
        );

        // v2.1: start from an empty map — the event's auth_events decide.
        let v21 = chain.iterative_auth_checks(&ordered, &HashMap::new(), &events, &rejects, sender_is_joined(&events));
        assert_eq!(
            v21.get("m.room.join_rules:"),
            Some(&"$rules".to_string()),
            "v2.1 authorises it from its own auth history: {v21:?}"
        );
    }

    /// A rejected auth event is never used to fill a missing key.
    #[test]
    fn iterative_auth_checks_never_fills_from_a_rejected_event() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            member_event("$a_join", "@a:ex.com", "join", &[]),
            join_rules_event("$rules", "@a:ex.com", &["$a_join"]),
        ]);
        let ordered = vec!["$rules".to_string()];
        let rejects: HashSet<String> = ["$a_join".to_string()].into_iter().collect();

        let resolved =
            chain.iterative_auth_checks(&ordered, &HashMap::new(), &events, &rejects, sender_is_joined(&events));
        assert!(
            !resolved.values().any(|id| id == "$rules"),
            "with the only authorising member event rejected, nothing is authorised: {resolved:?}"
        );
    }

    // ── reassembled resolver: replay semantics ───────────────────────────

    /// `resolve_state_v2` input: a state set mapping `"type:state_key"` to the
    /// projected event value (the resolver only reads `event_id` from it).
    fn state_set(entries: Vec<(&str, &str)>) -> HashMap<String, &'static Value> {
        // Leaked: the map borrows the values, and a test binary leaking a few
        // small JSON values is harmless.
        entries.into_iter().map(|(k, id)| (k.to_string(), &*Box::leak(Box::new(json!({ "event_id": id }))))).collect()
    }

    /// The replay decides the winner: the candidate whose sender is not a joined
    /// member is skipped, even when it sorts first. Under the previous
    /// "first candidate in order wins" logic the loser here would have won.
    #[test]
    fn replay_decides_the_conflicted_winner_not_the_ordering() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            member_event("$a_join", "@a:ex.com", "join", &[]),
            member_event("$b_leave", "@b:ex.com", "leave", &[]),
            join_rules_event("$rules_ok", "@a:ex.com", &["$a_join"]),
            join_rules_event("$rules_bad", "@b:ex.com", &["$b_leave"]),
        ]);

        let set_a = state_set(vec![("m.room.join_rules:", "$rules_ok")]);
        let set_b = state_set(vec![("m.room.join_rules:", "$rules_bad")]);
        let sets: Vec<&HashMap<String, &Value>> = vec![&set_a, &set_b];

        let resolved = chain.resolve_state_v2(&sets, &events, sender_is_joined(&events));
        assert_eq!(
            resolved.get("m.room.join_rules:").and_then(|v| v.get("join_rule")).and_then(|v| v.as_str()),
            Some("public"),
            "the replay must keep the authorised candidate: {resolved:?}"
        );
    }

    /// Spec step 5: the unconflicted state overrides whatever the replay produced
    /// for the same key.
    #[test]
    fn unconflicted_state_overrides_the_replayed_result() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![
            member_event("$a_join", "@a:ex.com", "join", &[]),
            join_rules_event("$rules_a", "@a:ex.com", &["$a_join"]),
            join_rules_event("$rules_b", "@a:ex.com", &["$a_join"]),
        ]);

        // `m.room.name:` is identical in both sets → unconflicted. (The name
        // events themselves are not in `events`, which is fine: the overlay
        // carries the value through.)
        let dir_a: &'static Value = Box::leak(Box::new(json!({ "event_id": "$dir_a" })));
        let dir_b: &'static Value = Box::leak(Box::new(json!({ "event_id": "$dir_a" })));
        let mut set_a = state_set(vec![("m.room.join_rules:", "$rules_a")]);
        let mut set_b = state_set(vec![("m.room.join_rules:", "$rules_b")]);
        set_a.insert("m.room.name:".to_string(), dir_a);
        set_b.insert("m.room.name:".to_string(), dir_b);
        let sets: Vec<&HashMap<String, &Value>> = vec![&set_a, &set_b];

        let resolved = chain.resolve_state_v2(&sets, &events, sender_is_joined(&events));
        assert_eq!(
            resolved.get("m.room.name:").and_then(|v| v.get("event_id")).and_then(|v| v.as_str()),
            Some("$dir_a"),
            "the unconflicted key must survive resolution: {resolved:?}"
        );
    }

    /// No conflict at all → the unconflicted state is returned unchanged. The
    /// predicate denies everything, so a result can only come from the
    /// unconflicted overlay.
    #[test]
    fn no_conflict_returns_the_unconflicted_state_without_replay() {
        let chain = EventAuthChain::new();
        let events = events_of(vec![]);
        let set_a = state_set(vec![("m.room.name:", "$x")]);
        let set_b = state_set(vec![("m.room.name:", "$x")]);
        let sets: Vec<&HashMap<String, &Value>> = vec![&set_a, &set_b];

        let resolved = chain.resolve_state_v2(&sets, &events, |_, _| false);
        assert_eq!(resolved.get("m.room.name:").and_then(|v| v.get("event_id")).and_then(|v| v.as_str()), Some("$x"));
    }
}
