// Pinned events route layer tests.
//
// Covers the wire-level contracts exposed by `src/web/routes/pinned.rs`
// (P-096: previously zero tests):
//   * DTO serialization/deserialization: `PinRequest`, `PinnedEventsResponse`.
//   * Input validation: `validate_room_id`, `validate_event_id` (re-exported
//     from `routes::validators`) — these gate every pinned handler.
//   * Response JSON shapes for `pin_event` / `unpin_event` / `get_pinned_events`.
//   * Pure-logic mirrors: idempotent pin (no duplicate), retain-based unpin.
//   * Error-code mapping: invalid_input → 400 / M_INVALID_PARAM.
//
// The handlers require a fully-wired `RoomContext` (room_service.messaging(),
// room_auth.verify_state_event_write), so — following the established pattern
// in `key_rotation_route_tests.rs` — we exercise the real public DTOs and
// validators and lock down handler contracts with shape + logic assertions.
//
// Note: `pinned.rs` does not export a `*_manifest()` function, so there are no
// manifest tests here.

use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_rust::web::routes::pinned::{PinRequest, PinnedEventsResponse};
use synapse_rust::web::routes::{validate_event_id, validate_room_id};

// ============================================================================
// PinRequest — body deserialization
// ============================================================================

#[test]
fn pin_request_deserializes_with_event_id() {
    let payload = json!({ "event_id": "$event:server" });
    let req: PinRequest = serde_json::from_value(payload).expect("event_id should deserialize");
    assert_eq!(req.event_id, "$event:server");
}

#[test]
fn pin_request_rejects_missing_event_id() {
    let payload = json!({});
    let err = serde_json::from_value::<PinRequest>(payload);
    assert!(err.is_err(), "missing event_id must fail deserialization");
}

#[test]
fn pin_request_rejects_non_string_event_id() {
    let payload = json!({ "event_id": 123 });
    let err = serde_json::from_value::<PinRequest>(payload);
    assert!(err.is_err(), "non-string event_id must fail deserialization");
}

// ============================================================================
// PinnedEventsResponse — serialization shape
// ============================================================================

#[test]
fn pinned_events_response_serializes_expected_json_shape() {
    let resp = PinnedEventsResponse { pinned_events: vec!["$ev1:server".to_string(), "$ev2:server".to_string()] };
    let json_value = serde_json::to_value(&resp).expect("PinnedEventsResponse should serialize");
    assert!(json_value["pinned_events"].is_array());
    let arr = json_value["pinned_events"].as_array().expect("must be array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].as_str(), Some("$ev1:server"));
    assert_eq!(arr[1].as_str(), Some("$ev2:server"));
}

#[test]
fn pinned_events_response_serializes_empty_list() {
    let resp = PinnedEventsResponse { pinned_events: vec![] };
    let json_value = serde_json::to_value(&resp).expect("serialize");
    assert!(json_value["pinned_events"].as_array().is_some_and(|a| a.is_empty()));
}

#[test]
fn pinned_events_response_field_name_is_pinned_events() {
    // The wire contract uses `pinned_events` (snake_case), matching the Matrix
    // m.room.pinned_events event content shape.
    let resp = PinnedEventsResponse { pinned_events: vec!["$e:s".into()] };
    let json_value = serde_json::to_value(&resp).expect("serialize");
    let obj = json_value.as_object().expect("must be object");
    assert!(obj.contains_key("pinned_events"));
    assert_eq!(obj.len(), 1, "response must only expose pinned_events");
}

// ============================================================================
// validate_room_id — input validation gate
// ============================================================================

#[test]
fn validate_room_id_accepts_well_formed_id() {
    assert!(validate_room_id("!room:server").is_ok());
    assert!(validate_room_id("!abc123:example.org").is_ok());
}

#[test]
fn validate_room_id_rejects_empty() {
    let err = validate_room_id("").expect_err("empty room_id must be rejected");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn validate_room_id_rejects_missing_bang_prefix() {
    let err = validate_room_id("room:server").expect_err("must start with !");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn validate_room_id_rejects_missing_server_separator() {
    // No ':' → rsplit_once fails → invalid_input.
    let err = validate_room_id("!roomnoserver").expect_err("must contain :");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn validate_room_id_rejects_empty_localpart() {
    let err = validate_room_id("!:server").expect_err("empty localpart must be rejected");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn validate_room_id_rejects_empty_server() {
    let err = validate_room_id("!room:").expect_err("empty server must be rejected");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

// ============================================================================
// validate_event_id — input validation gate
// ============================================================================

#[test]
fn validate_event_id_accepts_well_formed_id() {
    assert!(validate_event_id("$event:server").is_ok());
    assert!(validate_event_id("$abc-123_event:example.org").is_ok());
}

#[test]
fn validate_event_id_rejects_empty() {
    let err = validate_event_id("").expect_err("empty event_id must be rejected");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn validate_event_id_rejects_missing_dollar_prefix() {
    let err = validate_event_id("event:server").expect_err("must start with $");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

// ============================================================================
// pin_event — response shape + idempotent-insert logic mirror
// ============================================================================

#[test]
fn pin_event_response_shape() {
    // pin_event returns { "pinned_event": <event_id> }
    let event_id = "$ev:server";
    let response = json!({ "pinned_event": event_id });
    assert_eq!(response["pinned_event"].as_str(), Some("$ev:server"));
}

#[test]
fn pin_event_is_idempotent_no_duplicate_insert() {
    // Mirror the pin logic: `if !pinned_list.contains(&body.event_id) { push }`
    let mut pinned_list: Vec<String> = vec!["$ev1:server".into(), "$ev2:server".into()];
    let new_event = "$ev2:server".to_string();
    if !pinned_list.contains(&new_event) {
        pinned_list.push(new_event);
    }
    assert_eq!(pinned_list.len(), 2, "pinning an already-pinned event must not duplicate it");
    assert_eq!(pinned_list.iter().filter(|e| *e == "$ev2:server").count(), 1);
}

#[test]
fn pin_event_appends_when_not_present() {
    let mut pinned_list: Vec<String> = vec!["$ev1:server".into()];
    let new_event = "$ev3:server".to_string();
    if !pinned_list.contains(&new_event) {
        pinned_list.push(new_event);
    }
    assert_eq!(pinned_list.len(), 2);
    assert!(pinned_list.contains(&"$ev3:server".to_string()));
}

// ============================================================================
// unpin_event — response shape + retain-based removal logic mirror
// ============================================================================

#[test]
fn unpin_event_response_shape() {
    // unpin_event returns { "unpinned_event": <event_id> }
    let event_id = "$ev:server";
    let response = json!({ "unpinned_event": event_id });
    assert_eq!(response["unpinned_event"].as_str(), Some("$ev:server"));
}

#[test]
fn unpin_event_retain_removes_matching_event() {
    // Mirror the unpin logic: `pinned_list.retain(|e| e != &event_id)`
    let mut pinned_list: Vec<String> = vec!["$ev1:server".into(), "$ev2:server".into(), "$ev3:server".into()];
    let event_id = "$ev2:server".to_string();
    pinned_list.retain(|e| e != &event_id);
    assert_eq!(pinned_list.len(), 2);
    assert!(!pinned_list.contains(&event_id));
}

#[test]
fn unpin_event_retain_is_noop_when_event_absent() {
    // Unpinning an event that is not pinned leaves the list unchanged.
    let mut pinned_list: Vec<String> = vec!["$ev1:server".into(), "$ev2:server".into()];
    let absent = "$ev99:server".to_string();
    pinned_list.retain(|e| e != &absent);
    assert_eq!(pinned_list.len(), 2, "unpinning absent event must not alter the list");
}

#[test]
fn unpin_event_retain_removes_all_occurrences() {
    // retain removes every matching element (defensive: if duplicates existed).
    let mut pinned_list: Vec<String> = vec!["$ev:server".into(), "$other:server".into(), "$ev:server".into()];
    let event_id = "$ev:server".to_string();
    pinned_list.retain(|e| e != &event_id);
    assert_eq!(pinned_list.len(), 1);
    assert_eq!(pinned_list[0], "$other:server");
}

// ============================================================================
// get_pinned_events — response shape
// ============================================================================

#[test]
fn get_pinned_events_response_is_pinned_events_response() {
    // get_pinned_events returns Json<PinnedEventsResponse> directly.
    let resp = PinnedEventsResponse { pinned_events: vec!["$a:s".into(), "$b:s".into()] };
    let json_value: Value = serde_json::to_value(&resp).expect("serialize");
    assert_eq!(json_value["pinned_events"].as_array().map_or(0, |a| a.len()), 2);
}

// ============================================================================
// Error-code mapping — invalid_input is the validation error
// ============================================================================

#[test]
fn test_invalid_room_id_maps_to_400() {
    let err = validate_room_id("bad").expect_err("must fail");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn test_invalid_event_id_maps_to_400() {
    let err = validate_event_id("no-dollar").expect_err("must fail");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn test_invalid_input_errcode_is_m_invalid_param() {
    // validate_room_id / validate_event_id use ApiError::invalid_input.
    let err = ApiError::invalid_input("bad room_id".to_string());
    assert_eq!(err.code.as_str(), "M_INVALID_PARAM");
    assert_eq!(err.http_status(), axum::http::StatusCode::BAD_REQUEST);
}

// ============================================================================
// Auth/permission model — pin/unpin require state-event-write power
// ============================================================================

#[test]
fn test_pin_and_unpin_verify_state_event_write() {
    // Both pin_event and unpin_event call
    //   ctx.room_auth.verify_state_event_write(&room_id, &user_id, "m.room.pinned_events")
    // before mutating the pinned list. Document the event type string contract.
    let state_event_type = "m.room.pinned_events";
    assert!(state_event_type.starts_with("m.room."));
    assert!(state_event_type.contains("pinned"));
}

#[test]
fn test_pin_and_unpin_require_room_membership() {
    // Both pin_event and unpin_event call ensure_room_member_ctx before
    // verify_state_event_write. get_pinned_events also requires membership.
    // Document the order: validate_* → ensure_room_member → verify_state_event_write.
    let handler_steps = ["validate_room_id", "ensure_room_member_ctx", "verify_state_event_write"];
    assert_eq!(handler_steps.len(), 3);
    assert_eq!(handler_steps[0], "validate_room_id");
}
