// Sticky Event Routes - MSC4354
// Allows clients to get and set sticky (pinned) event metadata

use crate::web::routes::response_helpers::empty_json;
use crate::web::routes::{
    ensure_room_member, validate_event_id, validate_room_id, ApiError, AppState, AuthenticatedUser,
};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::Value;

/// Query parameters for sticky events
#[derive(Deserialize)]
pub struct StickyEventQuery {
    /// The event type to query (optional)
    #[serde(rename = "event_type")]
    pub event_type: Option<String>,
}

/// Get sticky event metadata for a room
/// GET /_matrix/client/v3/rooms/{room_id}/sticky_events
pub async fn get_sticky_events(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(query): Query<StickyEventQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member(&state, &auth_user, &room_id, "Not a member of this room").await?;

    // If specific event_type is requested
    if let Some(event_type) = query.event_type {
        let sticky_event = state
            .services
            .sticky_event_storage
            .get_sticky_event(&room_id, &auth_user.user_id, &event_type)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get sticky event: {}", e)))?;

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
        let sticky_events = state
            .services
            .sticky_event_storage
            .get_all_sticky_events(&room_id, &auth_user.user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get sticky events: {}", e)))?;

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
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member(&state, &auth_user, &room_id, "Not a member of this room").await?;

    let events = body
        .get("events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing events array".to_string()))?;

    for event in events {
        let event_type = event
            .get("event_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing event_type".to_string()))?;

        let event_id = event
            .get("event_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing event_id".to_string()))?;
        validate_event_id(event_id)?;

        let stored_event = state
            .services
            .event_storage
            .get_event(event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load sticky event: {}", e)))?;
        let Some(stored_event) =
            stored_event.filter(|stored_event| stored_event.room_id == room_id)
        else {
            return Err(ApiError::not_found("Event not found".to_string()));
        };

        // Set the sticky event
        state
            .services
            .sticky_event_storage
            .set_sticky_event(
                &room_id,
                &auth_user.user_id,
                &stored_event.event_id,
                event_type,
                true,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set sticky event: {}", e)))?;
    }

    Ok(empty_json())
}

/// Clear sticky event metadata for a room
/// DELETE /_matrix/client/v3/rooms/{room_id}/sticky_events/{event_type}
pub async fn clear_sticky_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member(&state, &auth_user, &room_id, "Not a member of this room").await?;

    state
        .services
        .sticky_event_storage
        .clear_sticky_event(&room_id, &auth_user.user_id, &event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to clear sticky event: {}", e)))?;

    Ok(empty_json())
}
