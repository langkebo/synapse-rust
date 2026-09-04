// Sync helpers unit tests — exercises the pure response-assembly helpers in
// `synapse_services::sync_helpers`.
//
// The two public functions (`room_event_to_json`, `state_event_to_json`) are
// pure transformations from storage-layer event structs to Client-format
// JSON. They have ZERO tests in the source module; this file covers:
//   * Required top-level fields (type, content, sender, event_id, room_id,
//     origin_server_ts, unsigned.age).
//   * state_key inclusion (present vs absent).
//   * age saturation when origin_server_ts > now (rare but valid clock skew).
//   * StateEvent sender fallback to `sender` field when `user_id` is None.
//   * StateEvent event_type fallback to "m.room.message" when None.

use serde_json::json;
use synapse_services::sync_helpers::{room_event_to_json, state_event_to_json};
use synapse_storage::event::{RoomEvent, StateEvent};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn sample_room_event() -> RoomEvent {
    RoomEvent {
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"body": "hello", "msgtype": "m.text"}),
        state_key: None,
        depth: 1,
        origin_server_ts: 1_700_000_000_000,
        processed_ts: 1_700_000_000_500,
        not_before: 0,
        status: Some("persisted".to_string()),
        origin: "self".to_string(),
        stream_ordering: Some(1),
        redacts: None,
    }
}

fn sample_state_event() -> StateEvent {
    StateEvent {
        event_id: "$state:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        sender: "@alice:example.com".to_string(),
        event_type: Some("m.room.member".to_string()),
        content: json!({"membership": "join"}),
        state_key: Some("@alice:example.com".to_string()),
        unsigned: None,
        is_redacted: None,
        origin_server_ts: 1_700_000_000_000,
        depth: Some(1),
        processed_ts: None,
        not_before: None,
        status: None,
        origin: None,
        user_id: None,
        stream_ordering: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// room_event_to_json — required fields
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn room_event_to_json_includes_required_top_level_fields() {
    let event = sample_room_event();
    let value = room_event_to_json(&event);

    assert_eq!(value["type"], "m.room.message");
    assert_eq!(value["content"]["body"], "hello");
    assert_eq!(value["sender"], "@alice:example.com");
    assert_eq!(value["event_id"], "$event:example.com");
    assert_eq!(value["room_id"], "!room:example.com");
    assert_eq!(value["origin_server_ts"], 1_700_000_000_000_i64);
}

#[test]
fn room_event_to_json_includes_unsigned_age() {
    let event = sample_room_event();
    let value = room_event_to_json(&event);

    // unsigned.age must be present and a non-negative integer.
    let age = value["unsigned"]["age"].as_i64().expect("age must be present as i64");
    // age = now - origin_server_ts; now is >= origin_server_ts (test runs now).
    assert!(age >= 0, "age must be non-negative (got {age})");
}

#[test]
fn room_event_to_json_omits_state_key_when_none() {
    let event = sample_room_event(); // state_key = None
    let value = room_event_to_json(&event);

    assert!(value.get("state_key").is_none(), "state_key must be absent when None");
}

#[test]
fn room_event_to_json_includes_state_key_when_some() {
    let mut event = sample_room_event();
    event.state_key = Some("@alice:example.com".to_string());
    let value = room_event_to_json(&event);

    assert_eq!(value["state_key"], "@alice:example.com");
}

#[test]
fn room_event_to_json_age_is_zero_when_event_is_now() {
    let now = synapse_common::current_timestamp_millis();
    let mut event = sample_room_event();
    event.origin_server_ts = now;
    let value = room_event_to_json(&event);

    // `room_event_to_json` recomputes `now` internally, so up to ~1 ms may
    // have elapsed between the two reads. Age for a "now" event must be
    // 0 or 1 ms — matching the tolerance used by `calculate_age` tests.
    let age = value["unsigned"]["age"].as_i64().expect("age must be present");
    assert!(age <= 1, "age for a now-event must be near zero, got {age}");
}

#[test]
fn room_event_to_json_age_saturates_when_event_in_future() {
    // origin_server_ts in the future: age = now.saturating_sub(future_ts) = 0.
    let mut event = sample_room_event();
    event.origin_server_ts = i64::MAX;
    let value = room_event_to_json(&event);

    assert_eq!(value["unsigned"]["age"].as_i64(), Some(0), "age must saturate at 0 for future timestamps");
}

#[test]
fn room_event_to_json_preserves_arbitrary_content() {
    let mut event = sample_room_event();
    event.content = json!({
        "body": "complex",
        "msgtype": "m.text",
        "format": "org.matrix.custom.html",
        "formatted_body": "<b>complex</b>",
        "m.relates_to": { "rel_type": "m.replace", "event_id": "$orig:example.com" }
    });
    let value = room_event_to_json(&event);

    assert_eq!(value["content"]["body"], "complex");
    assert_eq!(value["content"]["formatted_body"], "<b>complex</b>");
    assert_eq!(value["content"]["m.relates_to"]["rel_type"], "m.replace");
}

// ─────────────────────────────────────────────────────────────────────────────
// state_event_to_json — required fields + fallbacks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn state_event_to_json_includes_required_top_level_fields() {
    let event = sample_state_event();
    let value = state_event_to_json(&event);

    assert_eq!(value["type"], "m.room.member");
    assert_eq!(value["content"]["membership"], "join");
    assert_eq!(value["sender"], "@alice:example.com");
    assert_eq!(value["event_id"], "$state:example.com");
    assert_eq!(value["room_id"], "!room:example.com");
    assert_eq!(value["origin_server_ts"], 1_700_000_000_000_i64);
    assert_eq!(value["state_key"], "@alice:example.com");
}

#[test]
fn state_event_to_json_includes_unsigned_age() {
    let event = sample_state_event();
    let value = state_event_to_json(&event);

    let age = value["unsigned"]["age"].as_i64().expect("age must be present");
    assert!(age >= 0);
}

#[test]
fn state_event_to_json_sender_falls_back_to_sender_field_when_user_id_none() {
    let mut event = sample_state_event();
    event.user_id = None;
    event.sender = "@bob:example.com".to_string();
    let value = state_event_to_json(&event);

    // When user_id is None, the helper must use the `sender` field.
    assert_eq!(value["sender"], "@bob:example.com");
}

#[test]
fn state_event_to_json_sender_prefers_user_id_when_present() {
    let mut event = sample_state_event();
    event.user_id = Some("@charlie:example.com".to_string());
    event.sender = "@bob:example.com".to_string();
    let value = state_event_to_json(&event);

    // When user_id is Some, it takes precedence over `sender`.
    assert_eq!(value["sender"], "@charlie:example.com");
}

#[test]
fn state_event_to_json_event_type_falls_back_to_default_when_none() {
    let mut event = sample_state_event();
    event.event_type = None;
    let value = state_event_to_json(&event);

    // When event_type is None, the helper must default to "m.room.message".
    assert_eq!(value["type"], "m.room.message");
}

#[test]
fn state_event_to_json_omits_state_key_when_none() {
    let mut event = sample_state_event();
    event.state_key = None;
    let value = state_event_to_json(&event);

    assert!(value.get("state_key").is_none());
}

#[test]
fn state_event_to_json_age_saturates_when_event_in_future() {
    let mut event = sample_state_event();
    event.origin_server_ts = i64::MAX;
    let value = state_event_to_json(&event);

    assert_eq!(value["unsigned"]["age"].as_i64(), Some(0));
}

#[test]
fn state_event_to_json_preserves_arbitrary_content() {
    let mut event = sample_state_event();
    event.content = json!({"membership": "join", "displayname": "Alice"});
    let value = state_event_to_json(&event);

    assert_eq!(value["content"]["membership"], "join");
    assert_eq!(value["content"]["displayname"], "Alice");
}
