use super::models::*;
use std::collections::{HashMap, HashSet, VecDeque};

/// Implementation of [`EventAuthChain`] methods.
impl EventAuthChain {
    /// See [`build_auth_chain_from_events`.
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

    /// See [`verify_auth_chain`.
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
                    // FED-04: 授权链中的事件必须是授权事件类型
                    if !Self::is_auth_event(&event.event_type) {
                        return false;
                    }
                    // FED-04: 非 create 根的事件必须被事件集合中某个事件的
                    // auth_events 引用，否则视为被伪造塞入链中
                    if event.event_type != "m.room.create" {
                        let referenced = events.values().any(|e| e.auth_events.iter().any(|id| id == event_id));
                        if !referenced {
                            return false;
                        }
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

    /// See [`build_auth_chain_with_cache`.
    pub fn build_auth_chain_with_cache(&self, events: &HashMap<String, EventData>, event_id: &str) -> Vec<String> {
        let cache_key = format!("auth_chain:{event_id}");

        // Return the cached chain directly — no recomputation needed.
        if let Some(cached_chain) = self.get_cached_auth_chain(&cache_key) {
            tracing::debug!("Auth chain cache hit for {}", event_id);
            return cached_chain;
        }

        let result = self.build_auth_chain_from_events(events, event_id);

        // Cache the full chain (not just a bool) so subsequent lookups avoid
        // the BFS recomputation entirely.
        self.cache_auth_chain_result(&cache_key, result.clone());

        result
    }

    /// See [`verify_event_auth_chain_complete`.
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

    /// See [`compute_mainline`.
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

    /// See [`get_mainline_depth`.
    pub fn get_mainline_depth(&self, mainline: &[String], event_id: &str) -> Option<usize> {
        mainline.iter().position(|e| e == event_id)
    }

    /// The **mainline depth** of `event_id`: the position, in `mainline`, of the
    /// **latest** mainline event reachable from it along `auth_events`.
    ///
    /// Spec (state resolution, "Definitions"): mainline ordering sorts by "the
    /// closest mainline event in the event's auth chain". "Closest" is the one
    /// with the greatest depth, so this walks the whole auth chain and takes the
    /// maximum. Events with no mainline ancestor report `0`, which sorts before
    /// the create event's own depth (`1`) — the same convention the spec uses.
    pub fn mainline_depth_of(&self, events: &HashMap<String, EventData>, mainline: &[String], event_id: &str) -> usize {
        let positions: HashMap<&str, usize> = mainline.iter().enumerate().map(|(i, eid)| (eid.as_str(), i)).collect();

        let mut best = 0usize;
        let mut visited: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = vec![event_id.to_string()];

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(position) = positions.get(current.as_str()) {
                best = best.max(position + 1);
            }
            if let Some(event) = events.get(&current) {
                for auth in &event.auth_events {
                    stack.push(auth.clone());
                }
            }
        }

        best
    }

    /// **Mainline ordering** (state resolution v2, also used by v2.1): order by
    /// mainline depth, then `origin_server_ts`, then `event_id` — each ascending.
    pub fn mainline_ordering(
        &self,
        events: &HashMap<String, EventData>,
        event_ids: &[String],
        mainline: &[String],
    ) -> Vec<String> {
        let depth_of = |eid: &String| self.mainline_depth_of(events, mainline, eid);
        let mut ordered = event_ids.to_vec();
        ordered.sort_by(|a, b| {
            depth_of(a).cmp(&depth_of(b)).then_with(|| ts_of(events, a).cmp(&ts_of(events, b))).then_with(|| a.cmp(b))
        });
        ordered
    }

    /// **Reverse topological power ordering** (state resolution v2, also used by
    /// v2.1): the lexicographically smallest topological ordering of `event_ids`
    /// over the DAG formed by their `auth_events`, with ties broken by the
    /// sender's power level (higher first), then `origin_server_ts` (earlier
    /// first), then `event_id` (smaller first).
    ///
    /// Implemented as Kahn's algorithm: repeatedly take the smallest ready node by
    /// that comparator. `power_of` is injected because a sender's power level is
    /// resolved from the room state, not carried on the event.
    pub fn reverse_topological_power_ordering<P>(
        &self,
        events: &HashMap<String, EventData>,
        event_ids: &[String],
        power_of: P,
    ) -> Vec<String>
    where
        P: Fn(&EventData) -> i64,
    {
        let in_set: HashSet<&str> = event_ids.iter().map(String::as_str).collect();

        // Edges restricted to `event_ids`: an auth event *within the set* must be
        // ordered before the event that references it.
        let mut indegree: HashMap<&str, usize> = event_ids.iter().map(|eid| (eid.as_str(), 0)).collect();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for eid in event_ids {
            if let Some(event) = events.get(eid) {
                for auth in &event.auth_events {
                    if in_set.contains(auth.as_str()) {
                        *indegree.entry(eid.as_str()).or_insert(0) += 1;
                        dependents.entry(auth.as_str()).or_default().push(eid.as_str());
                    }
                }
            }
        }

        let comparator = |a: &str, b: &str| -> std::cmp::Ordering {
            let power_a = events.get(a).map(&power_of).unwrap_or(0);
            let power_b = events.get(b).map(&power_of).unwrap_or(0);
            power_b
                .cmp(&power_a)
                .then_with(|| {
                    let ts_a = events.get(a).map(|e| e.origin_server_ts).unwrap_or(0);
                    let ts_b = events.get(b).map(|e| e.origin_server_ts).unwrap_or(0);
                    ts_a.cmp(&ts_b)
                })
                .then_with(|| a.cmp(b))
        };

        let mut ready: Vec<&str> =
            event_ids.iter().map(String::as_str).filter(|eid| indegree.get(eid).copied().unwrap_or(0) == 0).collect();
        let mut ordered: Vec<String> = Vec::with_capacity(event_ids.len());
        let mut emitted: HashSet<&str> = HashSet::new();

        while !ready.is_empty() {
            ready.sort_by(|a, b| comparator(a, b));
            let next = ready.remove(0);
            if !emitted.insert(next) {
                continue;
            }
            ordered.push(next.to_string());
            if let Some(children) = dependents.get(next) {
                for child in children {
                    if let Some(degree) = indegree.get_mut(child) {
                        *degree = degree.saturating_sub(1);
                        if *degree == 0 && !emitted.contains(child) {
                            ready.push(child);
                        }
                    }
                }
            }
        }

        // An auth cycle would leave nodes unordered; append them deterministically
        // rather than dropping them (the auth graph is acyclic by construction, so
        // this is a safety net).
        let mut rest: Vec<&str> = event_ids.iter().map(String::as_str).filter(|e| !emitted.contains(e)).collect();
        rest.sort_by(|a, b| comparator(a, b));
        ordered.extend(rest.into_iter().map(str::to_string));

        ordered
    }
}

/// `origin_server_ts` of `event_id`, or `0` when unknown.
fn ts_of(events: &HashMap<String, EventData>, event_id: &String) -> i64 {
    events.get(event_id).map(|event| event.origin_server_ts).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event_data(
        event_id: &str,
        room_id: &str,
        event_type: &str,
        auth_events: Vec<&str>,
        sender: &str,
        depth: i64,
    ) -> EventData {
        EventData {
            event_id: event_id.into(),
            room_id: room_id.into(),
            event_type: event_type.into(),
            auth_events: auth_events.iter().map(|s| s.to_string()).collect(),
            prev_events: Vec::new(),
            state_key: Some(serde_json::Value::String("".into())),
            content: Some(serde_json::json!({})),
            sender: sender.into(),
            origin_server_ts: depth * 1000,
            depth,
        }
    }

    // ── get_mainline_depth ────────────────────────────────────────────

    #[test]
    fn mainline_depth_finds_position() {
        let chain = EventAuthChain::new();
        let mainline: Vec<String> = vec!["$a".into(), "$b".into(), "$c".into()];
        assert_eq!(chain.get_mainline_depth(&mainline, "$a"), Some(0));
        assert_eq!(chain.get_mainline_depth(&mainline, "$b"), Some(1));
        assert_eq!(chain.get_mainline_depth(&mainline, "$c"), Some(2));
    }

    #[test]
    fn mainline_depth_missing_event_returns_none() {
        let chain = EventAuthChain::new();
        let mainline: Vec<String> = vec!["$a".into()];
        assert_eq!(chain.get_mainline_depth(&mainline, "$x"), None);
    }

    #[test]
    fn mainline_depth_empty_returns_none() {
        let chain = EventAuthChain::new();
        let mainline: Vec<String> = vec![];
        assert_eq!(chain.get_mainline_depth(&mainline, "$a"), None);
    }

    // ── compute_mainline ──────────────────────────────────────────────

    #[test]
    fn compute_mainline_starts_with_create() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events
            .insert("$create".into(), make_event_data("$create", "!r:ex.com", "m.room.create", vec![], "@a:ex.com", 1));
        let mainline = chain.compute_mainline(&events, "$create");
        assert_eq!(mainline[0], "$create");
    }

    #[test]
    fn compute_mainline_includes_power_levels_in_depth_order() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events
            .insert("$create".into(), make_event_data("$create", "!r:ex.com", "m.room.create", vec![], "@a:ex.com", 1));
        events.insert(
            "$pl1".into(),
            make_event_data("$pl1", "!r:ex.com", "m.room.power_levels", vec!["$create"], "@a:ex.com", 2),
        );
        events.insert(
            "$pl2".into(),
            make_event_data("$pl2", "!r:ex.com", "m.room.power_levels", vec!["$pl1"], "@a:ex.com", 3),
        );
        let mainline = chain.compute_mainline(&events, "$create");
        assert_eq!(mainline.len(), 3);
        assert_eq!(mainline[0], "$create");
        assert_eq!(mainline[1], "$pl1");
        assert_eq!(mainline[2], "$pl2");
    }

    #[test]
    fn compute_mainline_ignores_non_pl_and_non_create() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events
            .insert("$create".into(), make_event_data("$create", "!r:ex.com", "m.room.create", vec![], "@a:ex.com", 1));
        events.insert(
            "$msg".into(),
            make_event_data("$msg", "!r:ex.com", "m.room.message", vec!["$create"], "@a:ex.com", 2),
        );
        let mainline = chain.compute_mainline(&events, "$create");
        assert_eq!(mainline.len(), 1);
        assert_eq!(mainline[0], "$create");
    }

    // ── build_auth_chain_from_events ──────────────────────────────────

    #[test]
    fn build_auth_chain_collects_auth_events_only() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events
            .insert("$create".into(), make_event_data("$create", "!r:ex.com", "m.room.create", vec![], "@a:ex.com", 1));
        events.insert(
            "$msg".into(),
            make_event_data("$msg", "!r:ex.com", "m.room.message", vec!["$create"], "@a:ex.com", 2),
        );
        // build_auth_chain_from_events follows auth_events BFS and collects auth events
        let auth_chain = chain.build_auth_chain_from_events(&events, "$msg");
        // $msg has auth_event $create, which is an auth event type
        assert!(auth_chain.contains(&"$create".to_string()));
    }

    #[test]
    fn build_auth_chain_empty_for_nonexistent_event() {
        let chain = EventAuthChain::new();
        let events = HashMap::new();
        let auth_chain = chain.build_auth_chain_from_events(&events, "$nonexistent");
        assert!(auth_chain.is_empty());
    }

    // ── verify_auth_chain ─────────────────────────────────────────────

    #[test]
    fn verify_auth_chain_empty_returns_false() {
        let chain = EventAuthChain::new();
        let events = HashMap::new();
        assert!(!chain.verify_auth_chain(&events, "!r:ex.com", &[]));
    }

    #[test]
    fn verify_auth_chain_room_id_mismatch_returns_false() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events.insert(
            "$create".into(),
            make_event_data("$create", "!other:ex.com", "m.room.create", vec![], "@a:ex.com", 1),
        );
        assert!(!chain.verify_auth_chain(&events, "!r:ex.com", &["$create".into()]));
    }

    #[test]
    fn verify_auth_chain_valid_returns_true() {
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events
            .insert("$create".into(), make_event_data("$create", "!r:ex.com", "m.room.create", vec![], "@a:ex.com", 1));
        assert!(chain.verify_auth_chain(&events, "!r:ex.com", &["$create".into()]));
    }

    // ------------------------------------------------------------------
    // S3 / FED-04: 授权链必须校验事件类型与 auth_events 引用有效性
    // ------------------------------------------------------------------

    #[test]
    fn verify_auth_chain_rejects_non_auth_event_type() {
        // 攻击者把普通消息事件塞进授权链 —— 必须拒绝
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events.insert("$msg".into(), make_event_data("$msg", "!r:ex.com", "m.room.message", vec![], "@a:ex.com", 2));
        assert!(!chain.verify_auth_chain(&events, "!r:ex.com", &["$msg".into()]));
    }

    #[test]
    fn verify_auth_chain_rejects_unreferenced_auth_event() {
        // 授权事件不被任何事件的 auth_events 引用（且非 create 根）—— 必须拒绝
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events
            .insert("$create".into(), make_event_data("$create", "!r:ex.com", "m.room.create", vec![], "@a:ex.com", 1));
        events.insert(
            "$orphan_pl".into(),
            make_event_data("$orphan_pl", "!r:ex.com", "m.room.power_levels", vec![], "@attacker:evil.com", 5),
        );
        assert!(!chain.verify_auth_chain(&events, "!r:ex.com", &["$create".into(), "$orphan_pl".into()]));
    }

    #[test]
    fn verify_auth_chain_accepts_referenced_auth_events() {
        // create + 被引用的 power_levels/member（目标事件 $msg 引用整条链）—— 合法链必须接受
        let chain = EventAuthChain::new();
        let mut events = HashMap::new();
        events
            .insert("$create".into(), make_event_data("$create", "!r:ex.com", "m.room.create", vec![], "@a:ex.com", 1));
        events.insert(
            "$pl".into(),
            make_event_data("$pl", "!r:ex.com", "m.room.power_levels", vec!["$create"], "@a:ex.com", 2),
        );
        events.insert(
            "$member".into(),
            make_event_data("$member", "!r:ex.com", "m.room.member", vec!["$create", "$pl"], "@a:ex.com", 3),
        );
        events.insert(
            "$msg".into(),
            make_event_data("$msg", "!r:ex.com", "m.room.message", vec!["$create", "$pl", "$member"], "@a:ex.com", 4),
        );
        assert!(chain.verify_auth_chain(&events, "!r:ex.com", &["$create".into(), "$pl".into(), "$member".into()]));
    }

    // ── reverse topological power ordering ────────────────────────────────

    #[test]
    fn reverse_topological_power_ordering_respects_auth_edges() {
        let chain = EventAuthChain::new();
        // `$b_promoted` is authorised by `$a_promotes_b`, so it must be ordered
        // after it even though it has the later timestamp and higher sender power.
        let events: HashMap<String, EventData> = vec![
            make_event_data("$a_promotes_b", "!r:e", "m.room.power_levels", vec![], "@a:e", 100),
            make_event_data("$b_promoted", "!r:e", "m.room.member", vec!["$a_promotes_b"], "@b:e", 200),
            make_event_data("$b_promotes_c", "!r:e", "m.room.power_levels", vec!["$b_promoted"], "@b:e", 300),
        ]
        .into_iter()
        .map(|e| (e.event_id.clone(), e))
        .collect();

        let ids: Vec<String> = events.keys().cloned().collect();
        let ordered = chain.reverse_topological_power_ordering(&events, &ids, |e| {
            // b outranks a, so power alone would put b's events first.
            if e.sender == "@b:e" {
                100
            } else {
                50
            }
        });

        let pos = |id: &str| ordered.iter().position(|e| e == id).unwrap_or(usize::MAX);
        assert!(pos("$a_promotes_b") < pos("$b_promoted"), "auth edge must be respected: {ordered:?}");
        assert!(pos("$b_promoted") < pos("$b_promotes_c"), "auth edge must be respected: {ordered:?}");
    }

    #[test]
    fn reverse_topological_power_ordering_breaks_ties_by_power_then_ts_then_id() {
        let chain = EventAuthChain::new();
        let mut events: HashMap<String, EventData> = vec![
            make_event_data("$low_power", "!r:e", "m.room.member", vec![], "@low:e", 1),
            make_event_data("$high_power", "!r:e", "m.room.member", vec![], "@high:e", 2),
            make_event_data("$b_ts", "!r:e", "m.room.member", vec![], "@low:e", 3),
        ]
        .into_iter()
        .map(|e| (e.event_id.clone(), e))
        .collect();
        // `$a_ts` matches `$b_ts` on power and timestamp, so event id decides.
        events.insert("$a_ts".to_string(), make_event_data("$a_ts", "!r:e", "m.room.member", vec![], "@low:e", 3));

        let ids: Vec<String> = events.keys().cloned().collect();
        let ordered =
            chain.reverse_topological_power_ordering(&events, &ids, |e| if e.sender == "@high:e" { 100 } else { 0 });

        assert_eq!(ordered[0], "$high_power", "higher sender power first: {ordered:?}");
        let pos = |id: &str| ordered.iter().position(|e| e == id).unwrap_or(usize::MAX);
        assert!(pos("$low_power") < pos("$a_ts"), "earlier timestamp breaks the next tie: {ordered:?}");
        assert!(pos("$a_ts") < pos("$b_ts"), "event id breaks the final tie: {ordered:?}");
    }

    // ── mainline ordering ────────────────────────────────────────────────

    #[test]
    fn mainline_ordering_uses_the_closest_mainline_ancestor() {
        let chain = EventAuthChain::new();
        // Mainline: $create -> $pl1 -> $pl2.
        // `$deep` reaches $pl2 (depth 3), `$shallow` only $pl1 (depth 2), and
        // `$none` has no mainline ancestor (depth 0, so it sorts first).
        let events: HashMap<String, EventData> = vec![
            make_event_data("$create", "!r:e", "m.room.create", vec![], "@a:e", 1),
            make_event_data("$pl1", "!r:e", "m.room.power_levels", vec!["$create"], "@a:e", 2),
            make_event_data("$pl2", "!r:e", "m.room.power_levels", vec!["$pl1"], "@a:e", 3),
            make_event_data("$deep", "!r:e", "m.room.name", vec!["$pl2"], "@a:e", 4),
            make_event_data("$shallow", "!r:e", "m.room.name", vec!["$pl1"], "@a:e", 5),
            make_event_data("$none", "!r:e", "m.room.name", vec![], "@a:e", 6),
        ]
        .into_iter()
        .map(|e| (e.event_id.clone(), e))
        .collect();

        let mainline = vec!["$create".to_string(), "$pl1".to_string(), "$pl2".to_string()];
        let ids: Vec<String> = events.keys().cloned().collect();
        let ordered = chain.mainline_ordering(&events, &ids, &mainline);

        let pos = |id: &str| ordered.iter().position(|e| e == id).unwrap_or(usize::MAX);
        assert!(pos("$none") < pos("$shallow"), "no mainline ancestor sorts first: {ordered:?}");
        assert!(pos("$shallow") < pos("$deep"), "the closest (deepest) mainline ancestor sorts later: {ordered:?}");

        assert_eq!(chain.mainline_depth_of(&events, &mainline, "$none"), 0);
        assert_eq!(chain.mainline_depth_of(&events, &mainline, "$shallow"), 2);
        assert_eq!(chain.mainline_depth_of(&events, &mainline, "$deep"), 3);
    }
}
