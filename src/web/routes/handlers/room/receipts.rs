use super::{ensure_room_view_access, get_room_event};
use crate::common::ApiError;
use crate::web::routes::{
    ensure_room_member, is_joined_room_member_or_creator, validate_event_id, validate_receipt_type, validate_room_id,
    AppState, AuthenticatedUser,
};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};

pub(crate) async fn send_receipt(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
    body: String,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_member(&state, &auth_user, &room_id, "You must be a member of this room to send receipts").await?;

    get_room_event(&state, &room_id, &event_id).await?;

    let body: Value = if body.trim().is_empty() { json!({}) } else { serde_json::from_str(&body).unwrap_or(json!({})) };

    state
        .services
        .rooms
        .room_service
        .send_receipt(&room_id, &auth_user.user_id, &event_id, &receipt_type, &body)
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id,
        "receipt_type": receipt_type,
        "ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_receipts(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;
    get_room_event(&state, &room_id, &event_id).await?;

    let receipts = state.services.rooms.room_service.get_receipts(&room_id, &receipt_type, &event_id).await?;

    Ok(Json(json!({
        "receipts": receipts
    })))
}

pub(crate) async fn set_read_markers(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state.services.rooms.room_service.get_room(&room_id).await?;

    let is_member = is_joined_room_member_or_creator(
        &state,
        &auth_user.user_id,
        &room_id,
        room.get("creator").and_then(|v| v.as_str()),
    )
    .await?;

    if !is_member {
        return Err(ApiError::forbidden("You are not a member of this room".to_string()));
    }

    state.services.rooms.room_service.set_read_markers(&room_id, &auth_user.user_id, &body).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}
