use super::super::{ensure_room_state_write_access, UpgradeRoomRequest, UpgradeRoomResponse};
use crate::common::ApiError;
use crate::web::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

use crate::web::routes::context::RoomContext;

pub(crate) async fn upgrade_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpgradeRoomRequest>,
) -> Result<Json<UpgradeRoomResponse>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_state_write_access(&ctx, &auth_user, &room_id, "m.room.tombstone").await?;

    let new_room_id = ctx.room_service.upgrade_room(&room_id, &body.new_version, &auth_user.user_id).await?;

    Ok(Json(UpgradeRoomResponse { replacement_room: new_room_id }))
}

pub(crate) async fn get_room_version(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let membership = ctx.room_service.membership().get_room_membership(&room_id, &auth_user.user_id).await?;

    if membership.is_none() {
        return Err(ApiError::not_found("Room not found or not a member".to_string()));
    }

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    Ok(Json(json!({
        "room_id": room_id,
        "room_version": room.room_version
    })))
}

pub(crate) async fn forget_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ctx.room_service.membership().forget_room(&room_id, &auth_user.user_id).await?;
    Ok(Json(json!({
        "room_id": room_id,
        "is_forgotten": true,
        "updated_ts": current_timestamp_millis()
    })))
}
