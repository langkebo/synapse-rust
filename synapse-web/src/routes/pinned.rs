use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::routes::{ensure_room_member, validate_event_id, validate_room_id, AppState, AuthenticatedUser};
use synapse_common::error::ApiError;

#[derive(Debug, Deserialize)]
pub struct PinRequest {
    pub event_id: String,
}

#[derive(Debug, Serialize)]
pub struct PinnedEventsResponse {
    pub pinned_events: Vec<String>,
}

pub async fn get_pinned_events(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<PinnedEventsResponse>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_member(&state, &auth_user, &room_id, "You must be a member of this room to view pinned events").await?;

    let pinned_list = state.services.rooms.room_service.get_pinned_event_ids(&room_id).await?;

    Ok(Json(PinnedEventsResponse { pinned_events: pinned_list }))
}

pub async fn pin_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<PinRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&body.event_id)?;
    ensure_room_member(&state, &auth_user, &room_id, "You must be a member of this room to modify pinned events")
        .await?;
    state
        .services
        .core
        .auth_service
        .verify_state_event_write(&room_id, &auth_user.user_id, "m.room.pinned_events")
        .await?;

    let mut pinned_list = state.services.rooms.room_service.get_pinned_event_ids(&room_id).await?;

    if !pinned_list.contains(&body.event_id) {
        pinned_list.push(body.event_id.clone());
    }
    state.services.rooms.room_service.set_pinned_event_ids(&room_id, &auth_user.user_id, &pinned_list).await?;

    Ok(Json(serde_json::json!({
        "pinned_event": body.event_id
    })))
}

pub async fn unpin_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_member(&state, &auth_user, &room_id, "You must be a member of this room to modify pinned events")
        .await?;
    state
        .services
        .core
        .auth_service
        .verify_state_event_write(&room_id, &auth_user.user_id, "m.room.pinned_events")
        .await?;

    let mut pinned_list = state.services.rooms.room_service.get_pinned_event_ids(&room_id).await?;

    pinned_list.retain(|e| e != &event_id);
    state.services.rooms.room_service.set_pinned_event_ids(&room_id, &auth_user.user_id, &pinned_list).await?;

    Ok(Json(serde_json::json!({
        "unpinned_event": event_id
    })))
}
