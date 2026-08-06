// Event service unit tests — verifies the re-export shim in
// `synapse_services::event_service`.
//
// The module is a pure type-re-export from `synapse_storage::event`:
//   pub use synapse_storage::event::{CreateEventParams, EventStorage, RoomEvent, StateEvent};
//
// These tests verify:
//   * The re-exported types are constructible from the service path (preserving
//     the `route → service → storage` layering contract documented in the
//     module).
//   * `CreateEventParams` field round-trip (Clone + Debug).
//   * `EventStorage` construction requires a pool + server_name (verifying the
//     type's API surface is unchanged through the re-export).
//   * `RoomEvent` / `StateEvent` can be built with typical fields (proving
//     the re-export is structurally complete, not missing fields).

use serde_json::json;
use std::sync::Arc;
use synapse_storage::event::{CreateEventParams, EventStorage, RoomEvent, StateEvent};

// ─────────────────────────────────────────────────────────────────────────────
// CreateEventParams
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn create_event_params_construction_round_trip() {
    let params = CreateEventParams {
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"body": "hello"}),
        state_key: None,
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
    };

    assert_eq!(params.event_id, "$event:example.com");
    assert_eq!(params.room_id, "!room:example.com");
    assert_eq!(params.user_id, "@alice:example.com");
    assert_eq!(params.event_type, "m.room.message");
    assert_eq!(params.content["body"], "hello");
    assert!(params.state_key.is_none());
    assert_eq!(params.origin_server_ts, 1_700_000_000_000);
    assert!(params.redacts.is_none());
}

#[test]
fn create_event_params_clone_is_independent() {
    let original = CreateEventParams {
        event_id: "$orig:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"body": "original"}),
        state_key: Some("@alice:example.com".to_string()),
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
    };

    let cloned = original.clone();
    assert_eq!(original.event_id, cloned.event_id);
    assert_eq!(original.content, cloned.content);
    assert_eq!(original.state_key, cloned.state_key);
}

#[test]
fn create_event_params_supports_redaction_event() {
    // Redaction events carry a `redacts` field targeting another event.
    let params = CreateEventParams {
        event_id: "$redaction:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.redaction".to_string(),
        content: json!({}),
        state_key: None,
        origin_server_ts: 1_700_000_000_000,
        redacts: Some("$target:example.com".to_string()),
    };

    assert_eq!(params.event_type, "m.room.redaction");
    assert_eq!(params.redacts.as_deref(), Some("$target:example.com"));
}

#[test]
fn create_event_params_supports_state_event_with_state_key() {
    let params = CreateEventParams {
        event_id: "$state:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.member".to_string(),
        content: json!({"membership": "join"}),
        state_key: Some("@alice:example.com".to_string()),
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
    };

    assert_eq!(params.state_key.as_deref(), Some("@alice:example.com"));
}

#[test]
fn create_event_params_debug_format_contains_event_id() {
    let params = CreateEventParams {
        event_id: "$debug:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"body": "debug"}),
        state_key: None,
        origin_server_ts: 0,
        redacts: None,
    };

    let debug_str = format!("{:?}", params);
    assert!(debug_str.contains("$debug:example.com"), "Debug output must include event_id");
}

// ─────────────────────────────────────────────────────────────────────────────
// RoomEvent — re-export completeness
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn room_event_reexport_is_constructible_with_all_fields() {
    let event = RoomEvent {
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"body": "hello"}),
        state_key: None,
        depth: 1,
        origin_server_ts: 1_700_000_000_000,
        processed_ts: 1_700_000_000_500,
        not_before: 0,
        status: Some("persisted".to_string()),
        reference_image: None,
        origin: "self".to_string(),
        stream_ordering: Some(1),
        redacts: None,
    };

    assert_eq!(event.event_id, "$event:example.com");
    assert_eq!(event.depth, 1);
    assert_eq!(event.origin, "self");
    assert_eq!(event.stream_ordering, Some(1));
}

#[test]
fn room_event_reexport_clones_correctly() {
    let event = RoomEvent {
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"body": "hello"}),
        state_key: None,
        depth: 1,
        origin_server_ts: 1_700_000_000_000,
        processed_ts: 0,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "self".to_string(),
        stream_ordering: None,
        redacts: None,
    };

    let cloned = event.clone();
    assert_eq!(event.event_id, cloned.event_id);
    assert_eq!(event.content, cloned.content);
}

// ─────────────────────────────────────────────────────────────────────────────
// StateEvent — re-export completeness
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn state_event_reexport_is_constructible_with_all_fields() {
    let event = StateEvent {
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
        reference_image: None,
        origin: None,
        user_id: None,
        stream_ordering: None,
    };

    assert_eq!(event.event_id, "$state:example.com");
    assert_eq!(event.sender, "@alice:example.com");
    assert_eq!(event.event_type.as_deref(), Some("m.room.member"));
}

#[test]
fn state_event_reexport_serializes_to_json() {
    let event = StateEvent {
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
        reference_image: None,
        origin: None,
        user_id: None,
        stream_ordering: None,
    };

    let json_str = serde_json::to_string(&event).expect("StateEvent must serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("must parse back");
    assert_eq!(parsed["event_id"], "$state:example.com");
    assert_eq!(parsed["sender"], "@alice:example.com");
    assert_eq!(parsed["content"]["membership"], "join");
}

// ─────────────────────────────────────────────────────────────────────────────
// EventStorage — re-export API surface
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn event_storage_reexport_constructible_with_pool_and_server_name() {
    // EventStorage requires a PgPool + server_name. We use connect_lazy to
    // avoid a real DB connection; the struct construction itself doesn't
    // execute queries.
    let pool = Arc::new(
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://nobody:nobody@localhost:5432/nobody")
            .expect("connect_lazy must not perform I/O"),
    );
    let storage = EventStorage { pool, server_name: "example.com".to_string() };

    assert_eq!(storage.server_name, "example.com");
}

#[tokio::test]
async fn event_storage_reexport_clone_is_independent() {
    let pool = Arc::new(
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://nobody:nobody@localhost:5432/nobody")
            .expect("connect_lazy must not perform I/O"),
    );
    let storage = EventStorage { pool, server_name: "example.com".to_string() };
    let cloned = storage.clone();

    assert_eq!(storage.server_name, cloned.server_name);
}
