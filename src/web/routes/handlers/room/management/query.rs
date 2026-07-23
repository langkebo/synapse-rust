use crate::common::ApiError;
use crate::web::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};

use crate::web::routes::context::RoomContext;

pub(crate) async fn get_room_info(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let user_id = &auth_user.user_id;

    let membership = ctx.room_service.membership().get_room_membership(&room_id, user_id).await?;

    if membership.is_none() {
        return Err(ApiError::not_found("Room not found or not a member".to_string()));
    }

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let summary = ctx.room_summary_service.get_summary(&room_id).await.ok().flatten();
    let invited_members_count = ctx.room_service.membership().get_invited_members_count(&room_id).await?;

    let guest_can_join = ctx
        .room_service
        .messaging()
        .get_state_events_by_type(&room_id, "m.room.guest_access")
        .await
        .ok()
        .and_then(|events| {
            events.into_iter().find(|event| event.get("state_key").and_then(Value::as_str) == Some("")).and_then(
                |event| {
                    event
                        .get("content")
                        .and_then(|content| content.get("guest_access"))
                        .and_then(Value::as_str)
                        .map(|value| value == "can_join")
                },
            )
        })
        .unwrap_or_else(|| summary.as_ref().is_some_and(|value| value.guest_access == "can_join"));

    Ok(Json(json!({
        "room_id": room_id,
        "name": room.name,
        "avatar_url": room.avatar_url,
        "topic": room.topic,
        "canonical_alias": room.canonical_alias,
        "joined_members_count": room.member_count,
        "invited_members_count": invited_members_count,
        "world_readable": room.is_public,
        "guest_can_join": guest_can_join,
        "membership": membership
    })))
}

pub(crate) async fn get_joined_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let room_ids = ctx.room_service.membership().get_joined_rooms(&auth_user.user_id).await?;

    Ok(Json(json!({
        "joined_rooms": room_ids
    })))
}

pub(crate) async fn get_my_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let room_list = ctx.room_service.state().get_user_room_list(&auth_user.user_id).await?;

    Ok(Json(json!({
        "rooms": room_list,
        "total": room_list.len()
    })))
}

pub(crate) async fn get_user_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let rooms = ctx.room_service.membership().get_joined_rooms(&user_id).await?;

    Ok(Json(json!({
        "joined_rooms": rooms
    })))
}
