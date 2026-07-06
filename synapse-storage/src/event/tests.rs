use super::*;
use serde_json::json;

/// Verify that the `ROOM_EVENT_COLS` constant contains every column
/// required to deserialize a `RoomEvent` via `sqlx::query_as`. This
/// guards against accidental column drift when refactoring SELECT
/// statements to use the constant (ARC-1..5).
#[test]
fn test_room_event_cols_contains_all_required_columns() {
    // The constant must exist (compile-time check) and be non-empty.
    assert!(!ROOM_EVENT_COLS.is_empty(), "ROOM_EVENT_COLS must not be empty");

    // Each column alias that sqlx maps to a RoomEvent field must appear.
    // The list mirrors the 15-column SELECT that was previously
    // duplicated across event/mod.rs and event/batch.rs.
    for required in [
        "event_id",
        "room_id",
        "user_id",
        "event_type",
        "content",
        "state_key",
        "depth",
        "origin_server_ts",
        "processed_at",
        "not_before",
        "status",
        "reference_image",
        "origin",
        "stream_ordering",
        "redacts",
    ] {
        assert!(
            ROOM_EVENT_COLS.contains(required),
            "ROOM_EVENT_COLS must contain column '{required}'; got: {ROOM_EVENT_COLS}"
        );
    }
}

#[test]
fn test_room_event_struct() {
    let event = RoomEvent {
        event_id: "$event123:example.com".to_string(),
        room_id: "!room123:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"msgtype": "m.text", "body": "Hello"}),
        state_key: None,
        depth: 1,
        origin_server_ts: 1234567890,
        processed_ts: 1234567891,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "self".to_string(),
        stream_ordering: Some(1),
        redacts: None,
    };

    assert_eq!(event.event_id, "$event123:example.com");
    assert_eq!(event.room_id, "!room123:example.com");
    assert_eq!(event.event_type, "m.room.message");
    assert!(event.state_key.is_none());
}

#[test]
fn test_state_event_struct() {
    let event = StateEvent {
        event_id: "$state123:example.com".to_string(),
        room_id: "!room123:example.com".to_string(),
        sender: "@alice:example.com".to_string(),
        event_type: Some("m.room.member".to_string()),
        content: json!({"membership": "join"}),
        state_key: Some("@bob:example.com".to_string()),
        unsigned: None,
        is_redacted: Some(false),
        origin_server_ts: 1234567890,
        depth: Some(1),
        processed_ts: Some(1234567891),
        not_before: Some(0),
        status: None,
        reference_image: None,
        origin: Some("self".to_string()),
        user_id: Some("@alice:example.com".to_string()),
        stream_ordering: Some(1),
    };

    assert_eq!(event.event_type, Some("m.room.member".to_string()));
    assert!(event.state_key.is_some());
    assert_eq!(event.state_key.unwrap(), "@bob:example.com");
}

#[test]
fn test_create_event_params() {
    let params = CreateEventParams {
        event_id: "$new_event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({"msgtype": "m.text", "body": "Test"}),
        state_key: None,
        origin_server_ts: 1234567890,
        redacts: None,
    };

    assert_eq!(params.event_id, "$new_event:example.com");
    assert_eq!(params.event_type, "m.room.message");
    assert!(params.state_key.is_none());
}

#[test]
fn test_create_event_params_with_state_key() {
    let params = CreateEventParams {
        event_id: "$state_event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        event_type: "m.room.member".to_string(),
        content: json!({"membership": "join"}),
        state_key: Some("@user:example.com".to_string()),
        origin_server_ts: 1234567890,
        redacts: None,
    };

    assert_eq!(params.event_type, "m.room.member");
    assert!(params.state_key.is_some());
}

#[test]
fn test_event_report_struct() {
    let report = EventReport {
        id: 1,
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        reporter_user_id: "@reporter:example.com".to_string(),
        reason: Some("Spam".to_string()),
        score: -50,
        received_ts: 1234567890,
        resolved_ts: None,
        resolved_by: None,
    };

    assert_eq!(report.id, 1);
    assert_eq!(report.reason, Some("Spam".to_string()));
    assert!(report.resolved_ts.is_none());
}

#[test]
fn test_event_report_id_struct() {
    let report_id = EventReportId { id: 42 };
    assert_eq!(report_id.id, 42);
}

#[test]
fn test_event_content_serialization() {
    let content = json!({
        "msgtype": "m.text",
        "body": "Hello, World!",
        "format": "org.matrix.custom.html",
        "formatted_body": "<b>Hello, World!</b>"
    });

    let event = RoomEvent {
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content,
        state_key: None,
        depth: 0,
        origin_server_ts: 0,
        processed_ts: 0,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "self".to_string(),
        stream_ordering: Some(0),
        redacts: None,
    };

    assert_eq!(event.content["msgtype"], "m.text");
    assert_eq!(event.content["body"], "Hello, World!");
}

#[test]
fn test_event_types() {
    let message_type = "m.room.message";
    let member_type = "m.room.member";
    let create_type = "m.room.create";
    let power_levels_type = "m.room.power_levels";

    assert!(message_type.starts_with("m.room."));
    assert!(member_type.starts_with("m.room."));
    assert!(create_type.starts_with("m.room."));
    assert!(power_levels_type.starts_with("m.room."));
}

#[test]
fn test_state_event_with_is_redacted() {
    let event = StateEvent {
        event_id: "$redacted:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        sender: "@alice:example.com".to_string(),
        event_type: Some("m.room.message".to_string()),
        content: json!({}),
        state_key: None,
        unsigned: Some(json!({"redacted_because": {}})),
        is_redacted: Some(true),
        origin_server_ts: 1234567890,
        depth: None,
        processed_ts: None,
        not_before: None,
        status: None,
        reference_image: None,
        origin: None,
        user_id: None,
        stream_ordering: None,
    };

    assert!(event.is_redacted.unwrap_or(false));
    assert!(event.unsigned.is_some());
}
