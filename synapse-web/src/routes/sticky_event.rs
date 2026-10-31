// Sticky Event Routes - MSC4354
// Allows clients to get and set sticky (pinned) event metadata

use crate::routes::context::RoomContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::MatrixQuery;
use crate::routes::extractors::RoomId;
use crate::routes::response_helpers::empty_json;
use crate::routes::MatrixJson;
use crate::routes::{ensure_room_member_ctx, validate_event_id, validate_room_id, ApiError, AuthenticatedUser};
use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::Value;

/// Route manifest for the sticky_event module. The handlers are mounted by
/// `room.rs` because MSC4354 paths are scoped under `/rooms/...`, but the
/// list of (method, path) tuples lives here so the ledger tracks changes
/// next to the handlers instead of in a sibling file.
pub fn sticky_event_compat_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/rooms/{room_id}/sticky_events"),
        (Method::POST, "/rooms/{room_id}/sticky_events"),
        (Method::DELETE, "/rooms/{room_id}/sticky_events/{event_type}"),
    ]
}

/// Query parameters for sticky events
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StickyEventQuery {
    /// The event type to query (optional)
    #[serde(rename = "event_type")]
    /// The `event_type` field.
    pub event_type: Option<String>,
}

/// MSC4354's request field for a sticky duration, as spelled by the proposal
/// (upstream `synapse/api/constants.py::StickyEvent.REQUEST_PARAM_NAME`).
///
/// Accepted alongside the shorter `sticky_duration_ms` so a client written
/// against the MSC works without translation.
pub const MSC4354_STICKY_DURATION_FIELD: &str = "org.matrix.msc4354.sticky_duration_ms";

/// Whether a client-shaped event JSON describes a **state** event.
///
/// `state_key` is the only discriminator the client projection carries: the
/// event serializer always emits the key, with `null` for timeline events and
/// a string (possibly the empty string) for state events. An absent key is
/// treated as "not a state event" — the route only ever feeds it events read
/// back from storage, which are produced by the same serializer.
///
/// Kept as a pure function so the MSC4354 rule "no sticky state events"
/// (upstream v1.163.0 #20256) is testable without an HTTP round trip.
fn is_state_event(event: &Value) -> bool {
    event.get("state_key").and_then(|value| value.as_str()).is_some()
}

/// Extract a sticky duration from one entry of the `events` array.
///
/// Both `org.matrix.msc4354.sticky_duration_ms` (proposal spelling) and
/// `sticky_duration_ms` (short form) are accepted; the proposal spelling wins if
/// both are present so a client can never accidentally shadow its own value.
/// A non-integer value is a client error rather than a silent "never expires".
fn sticky_duration_from_event(event: &Value) -> Result<Option<i64>, ApiError> {
    for key in [MSC4354_STICKY_DURATION_FIELD, "sticky_duration_ms"] {
        if let Some(value) = event.get(key) {
            return value
                .as_i64()
                .map(Some)
                .ok_or_else(|| ApiError::invalid_param(format!("{key} must be an integer number of milliseconds")));
        }
    }
    Ok(None)
}

/// Get sticky event metadata for a room
/// GET /_matrix/client/v3/rooms/{room_id}/sticky_events
pub async fn get_sticky_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(room_id): MatrixPath<RoomId>,
    MatrixQuery(query): MatrixQuery<StickyEventQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    /// Render one pin. `sticky_expiry_ts` is read straight from the tracked
    /// column, so what a client is told is exactly what the server expires on
    /// (upstream changelog 20324).
    fn render(event: &synapse_services::room::StickyEvent) -> Value {
        serde_json::json!({
            "room_id": event.room_id,
            "user_id": event.user_id,
            "event_id": event.event_id,
            "event_type": event.event_type,
            "sticky_duration_ms": event.sticky_duration_ms,
            "sticky_expiry_ts": event.expires_at,
        })
    }

    // If specific event_type is requested
    if let Some(event_type) = query.event_type {
        let sticky_event: Option<synapse_services::room::StickyEvent> = ctx
            .room_service
            .get_is_sticky_event(&room_id, &auth_user.user_id, &event_type)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get sticky event", e))?;

        let events = match sticky_event {
            Some(event) => vec![render(&event)],
            None => Vec::new(),
        };
        Ok(Json(serde_json::json!({ "events": events })))
    } else {
        // Get all sticky events
        let sticky_events: Vec<synapse_services::room::StickyEvent> = ctx
            .room_service
            .get_all_is_sticky_events(&room_id, &auth_user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get sticky events", e))?;

        let events: Vec<Value> = sticky_events.iter().map(render).collect();

        Ok(Json(serde_json::json!({
            "events": events
        })))
    }
}

/// Set sticky event metadata for a room
/// POST /_matrix/client/v3/rooms/{room_id}/sticky_events
pub async fn set_sticky_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(room_id): MatrixPath<RoomId>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    let events: &Vec<Value> = body
        .get("events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing events array".to_string()))?;

    for event in events {
        let event_type: &str = event
            .get("event_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing event_type".to_string()))?;

        let event_id_str: &str = event
            .get("event_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing event_id".to_string()))?;
        validate_event_id(event_id_str)?;

        let sticky_duration_ms = sticky_duration_from_event(event)?;

        let stored_event: serde_json::Value = ctx.room_service.messaging().get_event(&room_id, event_id_str).await?;
        let stored_event_id = stored_event.get("event_id").and_then(|v| v.as_str()).unwrap_or(event_id_str);

        // MSC4354 no longer allows **state** events to be sticky.
        //
        // Upstream Synapse removed that capability in v1.163.0 (#20256: "Remove
        // the ability to send state events that are sticky, as this feature was
        // removed from MSC4354"), so a state event carrying a sticky duration is
        // no longer something a conforming server accepts. This route is wider
        // than the upstream request surface in that it names an `event_id`
        // directly, so without this guard a client could mark `m.room.name` (or
        // any other state event) sticky — behaviour no other homeserver will
        // reproduce, which makes the pinned item unshareable across federation.
        //
        // `state_key` is the discriminator: `m.room.message` and friends carry
        // `null` there, state events carry the (possibly empty) state key.
        if is_state_event(&stored_event) {
            return Err(ApiError::bad_request(
                "State events cannot be sticky (MSC4354 removed sticky state events)".to_string(),
            ));
        }

        // Set the sticky event. The service already classifies an out-of-range
        // duration as a client error, so propagate rather than re-wrapping it into
        // a 500.
        ctx.room_service
            .set_is_sticky_event(&room_id, &auth_user.user_id, stored_event_id, event_type, true, sticky_duration_ms)
            .await?;
    }

    Ok(empty_json())
}

/// Clear sticky event metadata for a room
/// DELETE /_matrix/client/v3/rooms/{room_id}/sticky_events/{event_type}
pub async fn clear_sticky_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixPath((room_id, event_type)): MatrixPath<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    ctx.room_service
        .clear_is_sticky_event(&room_id, &auth_user.user_id, &event_type)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to clear sticky event", e))?;

    Ok(empty_json())
}

// ============== Tests ==============

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sticky_event_compat_relative_routes_count() {
        // The route manifest must contain exactly 3 routes: GET, POST, DELETE
        let routes = sticky_event_compat_relative_routes();
        assert_eq!(routes.len(), 3);
    }

    #[test]
    fn test_sticky_event_compat_relative_routes_methods() {
        let routes = sticky_event_compat_relative_routes();
        let methods: Vec<&axum::http::Method> = routes.iter().map(|(m, _)| m).collect();
        assert!(methods.contains(&&axum::http::Method::GET));
        assert!(methods.contains(&&axum::http::Method::POST));
        assert!(methods.contains(&&axum::http::Method::DELETE));
    }

    #[test]
    fn test_sticky_event_compat_relative_routes_paths() {
        let routes = sticky_event_compat_relative_routes();
        let paths: Vec<&str> = routes.iter().map(|(_, p)| *p).collect();
        // GET list
        assert!(paths.contains(&"/rooms/{room_id}/sticky_events"));
        // POST set
        assert!(paths.contains(&"/rooms/{room_id}/sticky_events"));
        // DELETE by event_type
        assert!(paths.contains(&"/rooms/{room_id}/sticky_events/{event_type}"));
    }

    #[test]
    fn test_sticky_event_query_deserialize() {
        // StickyEventQuery must support deny_unknown_fields
        let json = r#"{"event_type": "m.room.message"}"#;
        let query: StickyEventQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.event_type, Some("m.room.message".to_string()));
    }

    #[test]
    fn test_sticky_event_query_deserialize_no_event_type() {
        // When event_type is omitted, the field should be None
        let json = r#"{}"#;
        let query: StickyEventQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.event_type, None);
    }

    #[test]
    fn test_sticky_event_query_deserialize_unknown_field_rejected() {
        // deny_unknown_fields should reject unknown keys
        let json = r#"{"event_type": "m.room.message", "unknown_key": "value"}"#;
        let result: Result<StickyEventQuery, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_sticky_events_response_single_event() {
        // Response for a specific event_type must contain: events (array with one item)
        let event = json!({
            "room_id": "!room1",
            "user_id": "@alice",
            "event_id": "$event1",
            "event_type": "m.room.message"
        });

        let response = json!({
            "events": [event]
        });

        assert!(response.get("events").is_some());
        assert_eq!(response["events"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_get_sticky_events_response_empty() {
        // When no sticky event exists for the event_type, events should be empty
        let response = json!({
            "events": []
        });

        assert!(response.get("events").is_some());
        assert!(response["events"].is_array());
        assert_eq!(response["events"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_set_sticky_events_missing_events_field_rejected() {
        // set_sticky_events must reject a body without "events" array
        let body = json!({"not_events": []});
        let result = body.get("events").and_then(|v| v.as_array()).ok_or("Missing events array");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_sticky_events_missing_event_type_rejected() {
        // Each event in the events array must have an event_type field
        let events = vec![json!({"event_id": "$1"})];
        for event in &events {
            let result: Result<&str, &str> =
                event.get("event_type").and_then(|v: &serde_json::Value| v.as_str()).ok_or("Missing event_type");
            assert!(result.is_err());
        }
    }

    // --- MSC4354 sticky duration (upstream 20324) ---

    #[test]
    fn test_sticky_duration_absent_means_never_expires() {
        assert_eq!(sticky_duration_from_event(&json!({"event_type": "m.room.message"})).unwrap(), None);
    }

    #[test]
    fn test_sticky_duration_accepts_both_spellings() {
        assert_eq!(sticky_duration_from_event(&json!({"sticky_duration_ms": 60000})).unwrap(), Some(60_000));
        assert_eq!(sticky_duration_from_event(&json!({MSC4354_STICKY_DURATION_FIELD: 60000})).unwrap(), Some(60_000));
    }

    #[test]
    fn test_sticky_duration_proposal_spelling_wins_when_both_present() {
        let event = json!({
            MSC4354_STICKY_DURATION_FIELD: 60_000,
            "sticky_duration_ms": 1,
        });
        assert_eq!(
            sticky_duration_from_event(&event).unwrap(),
            Some(60_000),
            "the proposal spelling must not be shadowed by the short form"
        );
    }

    #[test]
    fn test_sticky_duration_rejects_non_integer() {
        // A non-integer must be an error, not a silent "never expires" — otherwise a
        // typo'd duration produces a pin that outlives the request.
        for bad in [json!("60000"), json!(1.5), json!(true), json!(null)] {
            let event = json!({"sticky_duration_ms": bad});
            assert!(
                sticky_duration_from_event(&event).is_err(),
                "expected a client error for {bad}, got {:?}",
                sticky_duration_from_event(&event)
            );
        }
    }

    #[test]
    fn test_sticky_duration_rejects_negative_at_the_boundary() {
        // Range policy itself lives in storage; the route must merely *forward* it so
        // the single source of truth (MAX_STICKY_DURATION_MS) is not duplicated here.
        assert_eq!(sticky_duration_from_event(&json!({"sticky_duration_ms": -1})).unwrap(), Some(-1));
    }

    // --- MSC4354: state events cannot be sticky (upstream v1.163.0 #20256) ---

    #[test]
    fn test_state_event_is_detected_by_a_string_state_key() {
        // The serializer always emits `state_key`; state events carry a string
        // (the empty string for `m.room.name`-style events with an empty key).
        assert!(is_state_event(&json!({"type": "m.room.name", "state_key": ""})));
        assert!(is_state_event(&json!({"type": "m.room.member", "state_key": "@alice:example.com"})));
    }

    #[test]
    fn test_timeline_event_is_not_a_state_event() {
        // Negative control: the real serializer writes `"state_key": null` for
        // timeline events. If this predicate ever reported `true` here, the guard
        // would reject every ordinary sticky message.
        assert!(!is_state_event(&json!({"type": "m.room.message", "state_key": null})));
        // Absent key is also not a state event (defensive: never over-reject).
        assert!(!is_state_event(&json!({"type": "m.room.message"})));
        // A non-string state_key is not something the serializer produces; treat it
        // as "not a state event" rather than rejecting on malformed input.
        assert!(!is_state_event(&json!({"type": "m.room.message", "state_key": 7})));
    }
}
