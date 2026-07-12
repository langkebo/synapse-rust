// Sticky Event Routes - MSC4354
// Allows clients to get and set sticky (pinned) event metadata

use crate::web::routes::context::RoomContext;
use crate::web::routes::response_helpers::empty_json;
use crate::web::routes::{ensure_room_member_ctx, validate_event_id, validate_room_id, ApiError, AuthenticatedUser};
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
pub struct StickyEventQuery {
    /// The event type to query (optional)
    #[serde(rename = "event_type")]
    pub event_type: Option<String>,
}

/// Get sticky event metadata for a room
/// GET /_matrix/client/v3/rooms/{room_id}/sticky_events
pub async fn get_sticky_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(query): Query<StickyEventQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    // If specific event_type is requested
    if let Some(event_type) = query.event_type {
        let sticky_event: Option<synapse_storage::sticky_event::StickyEvent> = ctx
            .sticky_event_storage
            .get_is_sticky_event(&room_id, &auth_user.user_id, &event_type)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get sticky event", &e))?;

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
        let sticky_events: Vec<synapse_storage::sticky_event::StickyEvent> = ctx
            .sticky_event_storage
            .get_all_is_sticky_events(&room_id, &auth_user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get sticky events", &e))?;

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
    Path(room_id): Path<String>,
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

        let stored_event: serde_json::Value = ctx.room_service.messaging.get_event(&room_id, event_id_str).await?;
        let stored_event_id = stored_event.get("event_id").and_then(|v| v.as_str()).unwrap_or(event_id_str);

        // Set the sticky event
        ctx.sticky_event_storage
            .set_is_sticky_event(&room_id, &auth_user.user_id, stored_event_id, event_type, true)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set sticky event", &e))?;
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

    ctx.sticky_event_storage
        .clear_is_sticky_event(&room_id, &auth_user.user_id, &event_type)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to clear sticky event", &e))?;

    Ok(empty_json())
}
