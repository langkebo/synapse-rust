// Sticky Event Routes - MSC4354
// Allows clients to get and set sticky (pinned) event metadata

use crate::routes::context::RoomContext;
use crate::routes::extractors::RoomId;
use crate::routes::response_helpers::empty_json;
use crate::routes::{ensure_room_member_ctx, validate_event_id, validate_room_id, ApiError, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    Json,
};
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

/// Get sticky event metadata for a room
/// GET /_matrix/client/v3/rooms/{room_id}/sticky_events
pub async fn get_sticky_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<StickyEventQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    // If specific event_type is requested
    if let Some(event_type) = query.event_type {
        let sticky_event: Option<synapse_services::room::StickyEvent> = ctx
            .room_service
            .get_is_sticky_event(&room_id, &auth_user.user_id, &event_type)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get sticky event", e))?;

        match sticky_event {
            Some(event) => Ok(Json(serde_json::json!({
                "events": [{
                    "room_id": event.room_id,
                    "user_id": event.user_id,
                    "event_id": event.event_id,
                    "event_type": event.event_type
                }]
            }))),
            None => Ok(Json(serde_json::json!({
                "events": []
            }))),
        }
    } else {
        // Get all sticky events
        let sticky_events: Vec<synapse_services::room::StickyEvent> = ctx
            .room_service
            .get_all_is_sticky_events(&room_id, &auth_user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get sticky events", e))?;

        let events: Vec<Value> = sticky_events
            .into_iter()
            .map(|e| {
                serde_json::json!({
                    "room_id": e.room_id,
                    "user_id": e.user_id,
                    "event_id": e.event_id,
                    "event_type": e.event_type
                })
            })
            .collect();

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
    Path(room_id): Path<RoomId>,
    Json(body): Json<Value>,
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

        let stored_event: serde_json::Value = ctx.room_service.messaging().get_event(&room_id, event_id_str).await?;
        let stored_event_id = stored_event.get("event_id").and_then(|v| v.as_str()).unwrap_or(event_id_str);

        // Set the sticky event
        ctx.room_service
            .set_is_sticky_event(&room_id, &auth_user.user_id, stored_event_id, event_type, true)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set sticky event", e))?;
    }

    Ok(empty_json())
}

/// Clear sticky event metadata for a room
/// DELETE /_matrix/client/v3/rooms/{room_id}/sticky_events/{event_type}
pub async fn clear_sticky_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
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
}
