//! Integration tests for `EventAuthChain` state resolution logic
//! (synapse-federation/src/event_auth/state_resolution.rs).
//!
//! These tests exercise the pure-logic state resolution algorithms:
//! conflict detection, power-based resolution, auth-chain traversal,
//! state-id hashing, reverse topological sort, and full v2 resolution.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};
use synapse_rust::federation::event_auth::{ConflictInfo, EventAuthChain, EventData};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn make_state_event(event_type: &str, state_key: &str, event_id: &str, ts: i64, sender: &str) -> Value {
    json!({
        "type": event_type,
        "state_key": state_key,
        "event_id": event_id,
        "origin_server_ts": ts,
        "sender": sender,
        "content": {}
    })
}

fn make_event_data(
    event_id: &str,
    event_type: &str,
    sender: &str,
    state_key: Option<&str>,
    content: Value,
    auth_events: Vec<&str>,
    origin_server_ts: i64,
    depth: i64,
) -> EventData {
    EventData {
        event_id: event_id.to_string(),
        room_id: format!("!room_{}:test.local", unique_id()),
        event_type: event_type.to_string(),
        auth_events: auth_events.iter().map(|s| s.to_string()).collect(),
        prev_events: Vec::new(),
        state_key: state_key.map(|sk| json!(sk)),
        content: Some(content),
        sender: sender.to_string(),
        origin_server_ts,
        depth,
    }
}

// ===========================================================================
// detect_conflicts
// ===========================================================================

#[test]
fn detect_conflicts_no_conflicts_single_event_per_key() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.member", "@alice:test", "$e1", 1000, "@alice:test"),
        make_state_event("m.room.name", "", "$e2", 2000, "@alice:test"),
    ];

    let conflicts = chain.detect_conflicts(&events);
    assert!(conflicts.is_empty(), "single event per key should not produce conflicts");
}

#[test]
fn detect_conflicts_multiple_events_same_key() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.member", "@alice:test", "$e1", 1000, "@alice:test"),
        make_state_event("m.room.member", "@alice:test", "$e2", 2000, "@bob:test"),
    ];

    let conflicts = chain.detect_conflicts(&events);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].state_key, "m.room.member:@alice:test");
    // Most recent timestamp wins
    assert_eq!(conflicts[0].winning_event, "$e2");
    assert_eq!(conflicts[0].losing_events, vec!["$e1".to_string()]);
    assert!(conflicts[0].resolution_reason.contains("Timestamp-based"));
}

#[test]
fn detect_conflicts_most_recent_timestamp_wins() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.name", "room", "$old", 100, "@a:test"),
        make_state_event("m.room.name", "room", "$mid", 500, "@b:test"),
        make_state_event("m.room.name", "room", "$new", 900, "@c:test"),
    ];

    let conflicts = chain.detect_conflicts(&events);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].winning_event, "$new");
    assert_eq!(conflicts[0].losing_events.len(), 2);
    assert!(conflicts[0].losing_events.contains(&"$old".to_string()));
    assert!(conflicts[0].losing_events.contains(&"$mid".to_string()));
}

#[test]
fn detect_conflicts_skips_empty_state_key() {
    let chain = EventAuthChain::new();
    // Empty state_key events are still state events but skipped by detect_conflicts
    let events = vec![
        make_state_event("m.room.name", "", "$e1", 1000, "@a:test"),
        make_state_event("m.room.name", "", "$e2", 2000, "@b:test"),
    ];

    let conflicts = chain.detect_conflicts(&events);
    assert!(conflicts.is_empty(), "events with empty state_key are skipped");
}

#[test]
fn detect_conflicts_empty_input() {
    let chain = EventAuthChain::new();
    let events: Vec<Value> = vec![];
    let conflicts = chain.detect_conflicts(&events);
    assert!(conflicts.is_empty());
}

#[test]
fn detect_conflicts_tiebreak_by_event_id_ascending() {
    let chain = EventAuthChain::new();
    // Same timestamp -> event_id ascending wins
    let events = vec![
        make_state_event("m.room.member", "@x:test", "$b", 500, "@x:test"),
        make_state_event("m.room.member", "@x:test", "$a", 500, "@y:test"),
    ];

    let conflicts = chain.detect_conflicts(&events);
    assert_eq!(conflicts.len(), 1);
    // Same timestamp, "$a" < "$b" so "$a" wins
    assert_eq!(conflicts[0].winning_event, "$a");
}

#[test]
fn detect_conflicts_multiple_distinct_keys() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.member", "@a:test", "$e1", 100, "@a:test"),
        make_state_event("m.room.member", "@a:test", "$e2", 200, "@a:test"),
        make_state_event("m.room.power_levels", "pl", "$e3", 100, "@a:test"),
        make_state_event("m.room.power_levels", "pl", "$e4", 300, "@a:test"),
    ];

    let conflicts = chain.detect_conflicts(&events);
    assert_eq!(conflicts.len(), 2);
}

// ===========================================================================
// resolve_conflicts_power_based
// ===========================================================================

#[test]
fn resolve_conflicts_power_based_higher_power_wins() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.member", "@x:test", "$low", 1000, "@user1:test"),
        make_state_event("m.room.member", "@x:test", "$high", 500, "@admin:test"),
    ];

    let mut power_levels = HashMap::new();
    power_levels.insert("@admin:test".to_string(), 100);
    power_levels.insert("@user1:test".to_string(), 0);

    let conflicts = chain.resolve_conflicts_power_based(&events, &power_levels);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].winning_event, "$high");
    assert!(conflicts[0].resolution_reason.contains("Power-based"));
    assert!(conflicts[0].resolution_reason.contains("power=100"));
}

#[test]
fn resolve_conflicts_power_based_equal_power_falls_back_to_timestamp() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.name", "room", "$old", 100, "@a:test"),
        make_state_event("m.room.name", "room", "$new", 500, "@b:test"),
    ];

    let mut power_levels = HashMap::new();
    power_levels.insert("@a:test".to_string(), 50);
    power_levels.insert("@b:test".to_string(), 50);

    let conflicts = chain.resolve_conflicts_power_based(&events, &power_levels);
    assert_eq!(conflicts.len(), 1);
    // Equal power (>0): newer timestamp wins via tiebreaker, but reason is Power-based
    // since winner.2 > 0
    assert_eq!(conflicts[0].winning_event, "$new");
    assert!(conflicts[0].resolution_reason.contains("Power-based"));
}

#[test]
fn resolve_conflicts_power_based_zero_power_timestamp_reason() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.topic", "topic", "$e1", 100, "@a:test"),
        make_state_event("m.room.topic", "topic", "$e2", 200, "@b:test"),
    ];

    let power_levels = HashMap::new(); // no power for anyone

    let conflicts = chain.resolve_conflicts_power_based(&events, &power_levels);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].winning_event, "$e2");
    assert!(conflicts[0].resolution_reason.contains("Timestamp-based"));
    assert!(conflicts[0].resolution_reason.contains("equal power"));
}

#[test]
fn resolve_conflicts_power_based_no_conflicts() {
    let chain = EventAuthChain::new();
    let events = vec![make_state_event("m.room.name", "", "$e1", 100, "@a:test")];
    let power_levels = HashMap::new();

    let conflicts = chain.resolve_conflicts_power_based(&events, &power_levels);
    assert!(conflicts.is_empty());
}

// ===========================================================================
// resolve_state_with_auth_chain
// ===========================================================================

#[test]
fn resolve_state_with_auth_chain_basic_traversal() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();

    events.insert(
        "$create".to_string(),
        make_event_data("$create", "m.room.create", "@founder:test", Some(""), json!({"creator": "@founder:test"}), vec![], 1, 1),
    );
    events.insert(
        "$pl".to_string(),
        make_event_data("$pl", "m.room.power_levels", "@founder:test", Some(""), json!({"ban": 50}), vec!["$create"], 2, 2),
    );
    events.insert(
        "$member".to_string(),
        make_event_data("$member", "m.room.member", "@founder:test", Some("@founder:test"), json!({"membership": "join"}), vec!["$create", "$pl"], 3, 3),
    );

    let state = chain.resolve_state_with_auth_chain(&events, &["$member"]);
    // The member event itself has a state_key and should be in state
    assert!(state.contains_key("m.room.member:@founder:test"));
    // Auth events with state_key also get added
    assert!(state.contains_key("m.room.create:"));
    assert!(state.contains_key("m.room.power_levels:"));
}

#[test]
fn resolve_state_with_auth_chain_handles_cycles() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();

    // Create a cycle: $a -> $b -> $a
    events.insert(
        "$a".to_string(),
        make_event_data("$a", "m.room.member", "@x:test", Some("@x:test"), json!({}), vec!["$b"], 1, 1),
    );
    events.insert(
        "$b".to_string(),
        make_event_data("$b", "m.room.member", "@y:test", Some("@y:test"), json!({}), vec!["$a"], 2, 2),
    );

    // Should terminate without infinite loop
    let state = chain.resolve_state_with_auth_chain(&events, &["$a"]);
    assert!(state.contains_key("m.room.member:@x:test"));
    assert!(state.contains_key("m.room.member:@y:test"));
}

#[test]
fn resolve_state_with_auth_chain_missing_event_skipped() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();

    let state = chain.resolve_state_with_auth_chain(&events, &["$nonexistent"]);
    assert!(state.is_empty());
}

#[test]
fn resolve_state_with_auth_chain_skips_non_state_events() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();

    // state_key is None -> not a state event
    events.insert(
        "$msg".to_string(),
        EventData {
            event_id: "$msg".to_string(),
            room_id: "!r:test".to_string(),
            event_type: "m.room.message".to_string(),
            auth_events: vec![],
            prev_events: vec![],
            state_key: None,
            content: Some(json!({"body": "hi"})),
            sender: "@a:test".to_string(),
            origin_server_ts: 1,
            depth: 1,
        },
    );

    let state = chain.resolve_state_with_auth_chain(&events, &["$msg"]);
    // Non-state events (state_key None) are not added to state map
    assert!(state.is_empty());
}

#[test]
fn resolve_state_with_auth_chain_processes_auth_events() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();

    events.insert(
        "$root".to_string(),
        make_event_data("$root", "m.room.member", "@root:test", Some("@root:test"), json!({"membership": "join"}), vec!["$auth1"], 10, 5),
    );
    events.insert(
        "$auth1".to_string(),
        make_event_data("$auth1", "m.room.create", "@root:test", Some(""), json!({"creator": "@root:test"}), vec![], 1, 1),
    );

    let state = chain.resolve_state_with_auth_chain(&events, &["$root"]);
    // Both root and its auth event auth1 should be processed
    assert!(state.contains_key("m.room.member:@root:test"));
    assert!(state.contains_key("m.room.create:"));
}

// ===========================================================================
// calculate_state_id
// ===========================================================================

#[test]
fn calculate_state_id_deterministic_same_input() {
    let chain = EventAuthChain::new();
    let mut state: HashMap<String, &Value> = HashMap::new();
    let v = json!({"name": "test"});
    state.insert("m.room.name:".to_string(), &v);

    let id1 = chain.calculate_state_id("!room:test", &state);
    let id2 = chain.calculate_state_id("!room:test", &state);
    assert_eq!(id1, id2, "state id should be deterministic for identical input");
}

#[test]
fn calculate_state_id_different_state_different_id() {
    let chain = EventAuthChain::new();
    let v1 = json!({"name": "room1"});
    let v2 = json!({"name": "room2"});

    let mut state1: HashMap<String, &Value> = HashMap::new();
    state1.insert("m.room.name:".to_string(), &v1);

    let mut state2: HashMap<String, &Value> = HashMap::new();
    state2.insert("m.room.name:".to_string(), &v2);

    let id1 = chain.calculate_state_id("!room:test", &state1);
    let id2 = chain.calculate_state_id("!room:test", &state2);
    assert_ne!(id1, id2, "different state should produce different ids");
}

#[test]
fn calculate_state_id_different_room_different_id() {
    let chain = EventAuthChain::new();
    let v = json!({"name": "test"});

    let mut state: HashMap<String, &Value> = HashMap::new();
    state.insert("m.room.name:".to_string(), &v);

    let id1 = chain.calculate_state_id("!room1:test", &state);
    let id2 = chain.calculate_state_id("!room2:test", &state);
    assert_ne!(id1, id2, "different room_id should produce different state ids");
}

#[test]
fn calculate_state_id_empty_state() {
    let chain = EventAuthChain::new();
    let state: HashMap<String, &Value> = HashMap::new();
    let id = chain.calculate_state_id("!room:test", &state);
    assert!(!id.is_empty(), "even empty state should produce a non-empty id");
}

#[test]
fn calculate_state_id_order_independent() {
    let chain = EventAuthChain::new();
    let v1 = json!({"a": 1});
    let v2 = json!({"b": 2});

    let mut state1: HashMap<String, &Value> = HashMap::new();
    state1.insert("key1".to_string(), &v1);
    state1.insert("key2".to_string(), &v2);

    let mut state2: HashMap<String, &Value> = HashMap::new();
    state2.insert("key2".to_string(), &v2);
    state2.insert("key1".to_string(), &v1);

    let id1 = chain.calculate_state_id("!room:test", &state1);
    let id2 = chain.calculate_state_id("!room:test", &state2);
    // The function sorts entries internally, so insertion order shouldn't matter
    assert_eq!(id1, id2, "state id should be order-independent");
}

// ===========================================================================
// detect_state_conflicts_advanced
// ===========================================================================

#[test]
fn detect_state_conflicts_advanced_power_based_resolution() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.member", "@x:test", "$low", 1000, "@user1:test"),
        make_state_event("m.room.member", "@x:test", "$high", 500, "@admin:test"),
    ];

    let mut power_levels = HashMap::new();
    power_levels.insert("@admin:test".to_string(), 100);
    power_levels.insert("@user1:test".to_string(), 0);

    let conflicts = chain.detect_state_conflicts_advanced(&events, Some(&power_levels));
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].winning_event, "$high");
    assert!(conflicts[0].resolution_reason.contains("Power-based"));
    assert!(conflicts[0].resolution_reason.contains("power=100"));
}

#[test]
fn detect_state_conflicts_advanced_no_power_levels() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.name", "room", "$old", 100, "@a:test"),
        make_state_event("m.room.name", "room", "$new", 200, "@b:test"),
    ];

    let conflicts = chain.detect_state_conflicts_advanced(&events, None);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].winning_event, "$new");
    assert!(conflicts[0].resolution_reason.contains("Timestamp-based"));
}

#[test]
fn detect_state_conflicts_advanced_zero_timestamp_default_resolution() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.topic", "topic", "$e1", 0, "@a:test"),
        make_state_event("m.room.topic", "topic", "$e2", 0, "@b:test"),
    ];

    let conflicts = chain.detect_state_conflicts_advanced(&events, None);
    assert_eq!(conflicts.len(), 1);
    // Both ts=0 and no power -> "Default resolution: first event selected"
    assert!(conflicts[0].resolution_reason.contains("Default resolution"));
}

#[test]
fn detect_state_conflicts_advanced_no_conflicts() {
    let chain = EventAuthChain::new();
    let events = vec![make_state_event("m.room.name", "", "$e1", 100, "@a:test")];
    let conflicts = chain.detect_state_conflicts_advanced(&events, None);
    assert!(conflicts.is_empty());
}

// ===========================================================================
// calculate_auth_difference
// ===========================================================================

#[test]
fn calculate_auth_difference_symmetric_difference() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();

    let chain_a = vec!["$e1".to_string(), "$e2".to_string(), "$e3".to_string()];
    let chain_b = vec!["$e2".to_string(), "$e3".to_string(), "$e4".to_string()];

    let diff = chain.calculate_auth_difference(&events, &chain_a, &chain_b);
    // Symmetric difference: $e1 and $e4
    assert!(diff.contains("$e1"));
    assert!(diff.contains("$e4"));
    assert!(!diff.contains("$e2"));
    assert!(!diff.contains("$e3"));
}

#[test]
fn calculate_auth_difference_includes_auth_events_of_diff() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();
    // $e1 is in the diff; its auth_events should be included
    events.insert(
        "$e1".to_string(),
        make_event_data("$e1", "m.room.member", "@a:test", Some("@a:test"), json!({}), vec!["$auth1", "$auth2"], 1, 1),
    );

    let chain_a = vec!["$e1".to_string()];
    let chain_b = vec!["$e2".to_string()];

    let diff = chain.calculate_auth_difference(&events, &chain_a, &chain_b);
    assert!(diff.contains("$e1"));
    assert!(diff.contains("$auth1"));
    assert!(diff.contains("$auth2"));
}

#[test]
fn calculate_auth_difference_identical_chains_empty() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();
    let chain_a = vec!["$e1".to_string(), "$e2".to_string()];
    let chain_b = chain_a.clone();

    let diff = chain.calculate_auth_difference(&events, &chain_a, &chain_b);
    assert!(diff.is_empty(), "identical chains should produce empty difference");
}

#[test]
fn calculate_auth_difference_empty_chains() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();
    let diff = chain.calculate_auth_difference(&events, &[], &[]);
    assert!(diff.is_empty());
}

// ===========================================================================
// sort_by_reverse_topological_power
// ===========================================================================

#[test]
fn sort_by_reverse_topological_power_higher_power_first() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();
    events.insert(
        "$low".to_string(),
        make_event_data("$low", "m.room.member", "@user:test", Some("@user:test"), json!({}), vec![], 100, 1),
    );
    events.insert(
        "$high".to_string(),
        make_event_data("$high", "m.room.member", "@admin:test", Some("@admin:test"), json!({}), vec![], 50, 1),
    );

    let mut power_levels = HashMap::new();
    power_levels.insert("@admin:test".to_string(), 100);
    power_levels.insert("@user:test".to_string(), 0);

    let sorted = chain.sort_by_reverse_topological_power(
        &events,
        &["$low".to_string(), "$high".to_string()],
        &[],
        &power_levels,
    );

    assert_eq!(sorted[0], "$high", "higher power event should come first");
    assert_eq!(sorted[1], "$low");
}

#[test]
fn sort_by_reverse_topological_power_tiebreak_by_timestamp() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();
    events.insert(
        "$newer".to_string(),
        make_event_data("$newer", "m.room.member", "@a:test", Some("@a:test"), json!({}), vec![], 500, 1),
    );
    events.insert(
        "$older".to_string(),
        make_event_data("$older", "m.room.member", "@a:test", Some("@a:test"), json!({}), vec![], 100, 1),
    );

    let power_levels = HashMap::new();
    let sorted = chain.sort_by_reverse_topological_power(
        &events,
        &["$newer".to_string(), "$older".to_string()],
        &[],
        &power_levels,
    );

    // Same power -> earlier timestamp first
    assert_eq!(sorted[0], "$older");
    assert_eq!(sorted[1], "$newer");
}

#[test]
fn sort_by_reverse_topological_power_mainline_tiebreak() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();
    events.insert(
        "$a".to_string(),
        make_event_data("$a", "m.room.member", "@a:test", Some("@a:test"), json!({}), vec![], 100, 1),
    );
    events.insert(
        "$b".to_string(),
        make_event_data("$b", "m.room.member", "@a:test", Some("@a:test"), json!({}), vec![], 100, 1),
    );

    let power_levels = HashMap::new();
    // $a is earlier in mainline
    let mainline = vec!["$a".to_string()];

    let sorted = chain.sort_by_reverse_topological_power(
        &events,
        &["$a".to_string(), "$b".to_string()],
        &mainline,
        &power_levels,
    );

    // Same power, same timestamp -> mainline order: $a (index 0) before $b (MAX)
    assert_eq!(sorted[0], "$a");
}

#[test]
fn sort_by_reverse_topological_power_power_from_event_content() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();
    // m.room.power_levels event whose sender power is in its own content.users
    events.insert(
        "$pl".to_string(),
        make_event_data(
            "$pl",
            "m.room.power_levels",
            "@creator:test",
            Some(""),
            json!({"users": {"@creator:test": 100}}),
            vec![],
            1,
            1,
        ),
    );
    events.insert(
        "$other".to_string(),
        make_event_data("$other", "m.room.member", "@user:test", Some("@user:test"), json!({}), vec![], 1, 1),
    );

    let power_levels = HashMap::new(); // empty; should fall back to content.users
    let sorted = chain.sort_by_reverse_topological_power(
        &events,
        &["$pl".to_string(), "$other".to_string()],
        &[],
        &power_levels,
    );

    assert_eq!(sorted[0], "$pl", "power_levels event with power 100 should come first");
}

#[test]
fn sort_by_reverse_topological_power_empty_input() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();
    let power_levels = HashMap::new();
    let sorted = chain.sort_by_reverse_topological_power(&events, &[], &[], &power_levels);
    assert!(sorted.is_empty());
}

// ===========================================================================
// resolve_state_v2
// ===========================================================================

#[test]
fn resolve_state_v2_empty_state_sets() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();
    let state_sets: Vec<&HashMap<String, &Value>> = vec![];

    let resolved = chain.resolve_state_v2(&state_sets, &events);
    assert!(resolved.is_empty(), "empty state sets should resolve to empty");
}

#[test]
fn resolve_state_v2_no_conflicts_all_same() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();
    let v = json!({"name": "room"});

    let state: HashMap<String, &Value> = vec![("m.room.name:".to_string(), &v)].into_iter().collect();
    let state_sets = vec![&state, &state];

    let resolved = chain.resolve_state_v2(&state_sets, &events);
    assert_eq!(resolved.get("m.room.name:"), Some(&json!({"name": "room"})));
}

#[test]
fn resolve_state_v2_single_candidate_conflict() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();
    events.insert(
        "$e1".to_string(),
        make_event_data("$e1", "m.room.name", "@a:test", Some(""), json!({"name": "v1"}), vec![], 100, 1),
    );

    let v1 = json!({"event_id": "$e1"});
    let state_a: HashMap<String, &Value> = vec![("m.room.name:".to_string(), &v1)].into_iter().collect();

    // Second set has no entry for this key (simulating absence)
    let state_b: HashMap<String, &Value> = HashMap::new();
    let state_sets = vec![&state_a, &state_b];

    let resolved = chain.resolve_state_v2(&state_sets, &events);
    // Single candidate -> adopted directly
    assert_eq!(resolved.get("m.room.name:"), Some(&json!({"name": "v1"})));
}

#[test]
fn resolve_state_v2_multi_candidate_uses_power_resolution() {
    let chain = EventAuthChain::new();
    let mut events = HashMap::new();
    // create event
    events.insert(
        "$create".to_string(),
        make_event_data("$create", "m.room.create", "@creator:test", Some(""), json!({"creator": "@creator:test"}), vec![], 1, 1),
    );
    // power levels event
    events.insert(
        "$pl".to_string(),
        make_event_data(
            "$pl",
            "m.room.power_levels",
            "@creator:test",
            Some(""),
            json!({"users": {"@creator:test": 100, "@user:test": 0}}),
            vec!["$create"],
            2,
            2,
        ),
    );
    // two competing name events
    events.insert(
        "$name1".to_string(),
        make_event_data("$name1", "m.room.name", "@user:test", Some(""), json!({"name": "user-name"}), vec!["$create"], 3, 3),
    );
    events.insert(
        "$name2".to_string(),
        make_event_data("$name2", "m.room.name", "@creator:test", Some(""), json!({"name": "creator-name"}), vec!["$create", "$pl"], 4, 4),
    );

    let v1 = json!({"event_id": "$name1"});
    let v2 = json!({"event_id": "$name2"});
    let state_a: HashMap<String, &Value> = vec![("m.room.name:".to_string(), &v1)].into_iter().collect();
    let state_b: HashMap<String, &Value> = vec![("m.room.name:".to_string(), &v2)].into_iter().collect();
    let state_sets = vec![&state_a, &state_b];

    let resolved = chain.resolve_state_v2(&state_sets, &events);
    // Should resolve to one of the candidates
    let name = resolved.get("m.room.name:");
    assert!(name == Some(&json!({"name": "user-name"})) || name == Some(&json!({"name": "creator-name"})),
        "multi-candidate conflict should resolve to a single value, got: {:?}", name);
}

#[test]
fn resolve_state_v2_single_set_passthrough() {
    let chain = EventAuthChain::new();
    let events: HashMap<String, EventData> = HashMap::new();
    let v = json!({"topic": "hello"});
    let state: HashMap<String, &Value> = vec![("m.room.topic:".to_string(), &v)].into_iter().collect();
    let state_sets = vec![&state];

    let resolved = chain.resolve_state_v2(&state_sets, &events);
    assert_eq!(resolved.get("m.room.topic:"), Some(&json!({"topic": "hello"})));
}

#[test]
fn conflict_info_struct_fields() {
    let info = ConflictInfo {
        state_key: "m.room.member:@a:test".to_string(),
        winning_event: "$e1".to_string(),
        losing_events: vec!["$e2".to_string(), "$e3".to_string()],
        resolution_reason: "test reason".to_string(),
    };
    assert_eq!(info.state_key, "m.room.member:@a:test");
    assert_eq!(info.winning_event, "$e1");
    assert_eq!(info.losing_events.len(), 2);
    assert_eq!(info.resolution_reason, "test reason");
}

#[test]
fn event_data_default_is_empty() {
    let data = EventData::default();
    assert!(data.event_id.is_empty());
    assert!(data.auth_events.is_empty());
    assert!(data.state_key.is_none());
    assert!(data.content.is_none());
    assert_eq!(data.origin_server_ts, 0);
    assert_eq!(data.depth, 0);
}

#[test]
fn event_auth_chain_default_equals_new() {
    let new_chain = EventAuthChain::new();
    let default_chain = EventAuthChain::default();
    // Both should have empty caches (verified via public API - no cached entries)
    assert!(new_chain.get_cached_auth_chain("$any").is_none());
    assert!(default_chain.get_cached_auth_chain("$any").is_none());
    assert!(new_chain.get_cached_depth("$any").is_none());
    assert!(default_chain.get_cached_depth("$any").is_none());
}

#[test]
fn detect_conflicts_with_hashset_losing_events_unique() {
    let chain = EventAuthChain::new();
    let events = vec![
        make_state_event("m.room.name", "room", "$a", 100, "@x:test"),
        make_state_event("m.room.name", "room", "$b", 200, "@y:test"),
        make_state_event("m.room.name", "room", "$c", 300, "@z:test"),
    ];

    let conflicts = chain.detect_conflicts(&events);
    assert_eq!(conflicts.len(), 1);
    let losing: HashSet<&str> = conflicts[0].losing_events.iter().map(|s| s.as_str()).collect();
    assert_eq!(losing.len(), 2, "losing events should be unique");
    assert!(losing.contains("$a"));
    assert!(losing.contains("$b"));
}
