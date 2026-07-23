use crate::common::ApiError;
use crate::map_internal;
use crate::web::routes::{ensure_room_member_ctx, AuthenticatedUser, OptionalAuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

use crate::web::routes::context::RoomContext;

#[axum::debug_handler]
pub(crate) async fn get_room_visibility(
    State(ctx): State<RoomContext>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = ctx
        .room_service
        .state()
        .get_room_visibility(&room_id)
        .await
        .map_err(map_internal!("Failed to get room visibility"))?;

    Ok(Json(json!({
        "visibility": visibility
    })))
}

#[axum::debug_handler]
pub(crate) async fn set_room_visibility(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = body
        .get("visibility")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing visibility field".to_string()))?;

    if visibility != "public" && visibility != "private" {
        return Err(ApiError::bad_request("visibility must be 'public' or 'private'".to_string()));
    }

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a member of this room to update room visibility")
        .await?;

    let is_creator = ctx
        .room_service
        .state()
        .is_room_creator(&room_id, &auth_user.user_id)
        .await
        .map_err(map_internal!("Failed to check room creator"))?;

    if !is_creator {
        return Err(ApiError::forbidden("Only the room creator can update room visibility".to_string()));
    }

    let is_public = visibility == "public";

    ctx.room_service.state().set_room_directory(&room_id, is_public).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "visibility": visibility,
        "updated_ts": current_timestamp_millis()
    })))
}
