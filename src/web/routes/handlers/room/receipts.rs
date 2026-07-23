use super::{ensure_room_view_access, get_room_event};
use crate::common::ApiError;
use crate::web::routes::context::RoomContext;
use crate::web::routes::{
    ensure_room_member_ctx, is_member_or_creator_ctx, validate_event_id, validate_receipt_type, validate_room_id,
    AuthenticatedUser,
};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

pub(crate) async fn send_receipt(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
    body: String,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a member of this room to send receipts").await?;

    // Element may attempt to send a read receipt for a local-echo event before the
    // remote echo has landed in the event store. Synapse tolerates that race; we
    // therefore treat "event not found" as a compatibility no-op while still
    // rejecting receipts that explicitly target an event from another room.
    if let Some(event) = ctx.room_service.messaging().get_event_record(&event_id).await? {
        if event.room_id != room_id {
            return Err(ApiError::not_found("Event not found".to_string()));
        }
    } else {
        return Ok(Json(json!({
            "room_id": room_id,
            "event_id": event_id,
            "receipt_type": receipt_type,
            "ts": current_timestamp_millis()
        })));
    }

    let body: Value = if body.trim().is_empty() { json!({}) } else { serde_json::from_str(&body).unwrap_or(json!({})) };

    ctx.room_service.messaging().send_receipt(&room_id, &auth_user.user_id, &event_id, &receipt_type, &body).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id,
        "receipt_type": receipt_type,
        "ts": current_timestamp_millis()
    })))
}

pub(crate) async fn get_receipts(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;
    get_room_event(&ctx, &room_id, &event_id).await?;

    let receipts = ctx.room_service.messaging().get_receipts(&room_id, &receipt_type, &event_id).await?;

    Ok(Json(json!({
        "receipts": receipts
    })))
}

pub(crate) async fn set_read_markers(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = ctx.room_service.get_room(&room_id).await?;

    let is_member =
        is_member_or_creator_ctx(&ctx, &auth_user.user_id, &room_id, room.get("creator").and_then(|v| v.as_str()))
            .await?;

    if !is_member {
        return Err(ApiError::forbidden("You are not a member of this room".to_string()));
    }

    ctx.room_service.messaging().set_read_markers(&room_id, &auth_user.user_id, &body).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "updated_ts": current_timestamp_millis()
    })))
}
