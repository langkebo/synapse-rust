use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::error::ApiError;
use crate::web::routes::context::RoomContext;
use crate::web::routes::room_access::ensure_room_member_ctx;
use crate::web::routes::{validate_event_id, validate_room_id, AuthenticatedUser};
use synapse_common::types::{EventId, RoomId};

#[derive(Debug, Deserialize)]
pub struct PinRequest {
    pub event_id: String,
}

#[derive(Debug, Serialize)]
pub struct PinnedEventsResponse {
    pub pinned_events: Vec<String>,
}

pub async fn get_pinned_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<PinnedEventsResponse>, ApiError> {
    let room_id_str = room_id.as_str();
    validate_room_id(room_id_str)?;
    ensure_room_member_ctx(&ctx, &auth_user, room_id_str, "You must be a member of this room to view pinned events")
        .await?;

    let pinned_list: Vec<String> = ctx.room_service.messaging().get_pinned_event_ids(room_id_str).await?;

    Ok(Json(PinnedEventsResponse { pinned_events: pinned_list }))
}

pub async fn pin_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Json(body): Json<PinRequest>,
) -> Result<Json<Value>, ApiError> {
    let room_id_str = room_id.as_str();
    validate_room_id(room_id_str)?;
    validate_event_id(&body.event_id)?;

    // Event existence + room-membership-of-event check.
    // `messaging().get_event` returns 404 ("Event not found") when the event
    // doesn't exist, and 404 ("Event not found in this room") when it exists
    // but belongs to a different room — exactly the semantics we need here.
    // We discard the returned payload; the call is purely for validation.
    let _ = ctx.room_service.messaging().get_event(room_id_str, &body.event_id).await?;

    ensure_room_member_ctx(&ctx, &auth_user, room_id_str, "You must be a member of this room to modify pinned events")
        .await?;
    ctx.room_auth.verify_state_event_write(room_id_str, &auth_user.user_id, "m.room.pinned_events").await?;

    let mut pinned_list: Vec<String> = ctx.room_service.messaging().get_pinned_event_ids(room_id_str).await?;

    if !pinned_list.contains(&body.event_id) {
        pinned_list.push(body.event_id.clone());
    }

    ctx.room_service.messaging().set_pinned_event_ids(room_id_str, &auth_user.user_id, &pinned_list).await?;

    Ok(Json(serde_json::json!({
        "pinned_event": body.event_id
    })))
}

pub async fn unpin_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    let room_id_str = room_id.as_str();
    let event_id_str = event_id.as_str();
    validate_room_id(room_id_str)?;
    validate_event_id(event_id_str)?;
    ensure_room_member_ctx(&ctx, &auth_user, room_id_str, "You must be a member of this room to modify pinned events")
        .await?;
    ctx.room_auth.verify_state_event_write(room_id_str, &auth_user.user_id, "m.room.pinned_events").await?;

    let mut pinned_list: Vec<String> = ctx.room_service.messaging().get_pinned_event_ids(room_id_str).await?;

    pinned_list.retain(|e| e != event_id_str);

    ctx.room_service.messaging().set_pinned_event_ids(room_id_str, &auth_user.user_id, &pinned_list).await?;

    Ok(Json(serde_json::json!({
        "unpinned_event": event_id_str
    })))
}
