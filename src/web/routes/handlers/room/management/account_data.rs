use super::super::ensure_room_view_access;
use crate::common::ApiError;
use crate::web::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

use crate::web::routes::context::RoomContext;

pub(crate) async fn set_room_account_data(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, data_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    ctx.account_data_service.set_room_account_data(&auth_user.user_id, &room_id, &data_type, &body).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "data_type": data_type,
        "updated_ts": current_timestamp_millis()
    })))
}

pub(crate) async fn get_room_account_data(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, data_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let result = ctx.account_data_service.get_room_account_data(&auth_user.user_id, &room_id, &data_type).await?;

    match result {
        Some(data) => Ok(Json(data)),
        None => Err(ApiError::not_found("Room account data not found".to_string())),
    }
}
