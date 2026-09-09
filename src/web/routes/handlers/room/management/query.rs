use crate::common::ApiError;
use crate::web::routes::extractors::{RoomId, UserId};
use crate::web::routes::{validate_room_id, validate_user_id, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};

use crate::web::routes::context::RoomContext;

/// See [`get_room_info`].
pub(crate) async fn get_room_info(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
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

/// See [`get_joined_rooms`].
pub(crate) async fn get_joined_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let room_ids = ctx.room_service.membership().get_joined_rooms(&auth_user.user_id).await?;

    Ok(Json(json!({
        "joined_rooms": room_ids
    })))
}

/// See [`get_my_rooms`].
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

/// See [`get_user_rooms`].
pub(crate) async fn get_user_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user_id = user_id.as_str();
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let rooms = ctx.room_service.membership().get_joined_rooms(user_id).await?;

    Ok(Json(json!({
        "joined_rooms": rooms
    })))
}

/// MSC2666: Get rooms in common (mutual rooms) with another user.
///
/// Per the spec, `user_id` is a query parameter containing the MXID of the
/// target user whose mutual rooms are being queried against the authenticated
/// user from the access token.
/// Querying mutual rooms with yourself returns M_FORBIDDEN.
///
/// Response:
/// ```json
/// {
///   "joined": ["!room1:server", ...],
///   "next_batch_token": "optional_pagination_token"
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub(crate) async fn get_mutual_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let other_user_id = params
        .get("user_id")
        .map(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing user_id query parameter".to_string()))?;

    validate_user_id(other_user_id)?;

    let user_id = &auth_user.user_id;
    let other = other_user_id;

    // Prevent self-query per spec
    if user_id == other {
        return Err(ApiError::forbidden("You cannot query mutual rooms with yourself".to_string()));
    }

    // Limit handling with default
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .min(1000);

    // Pagination: supports both `from` and `batch_token` query param names
    let after = params
        .get("from")
        .or_else(|| params.get("batch_token"))
        .map(|s| s.as_str());

    let result = ctx
        .room_service
        .membership()
        .get_mutual_rooms_between(user_id, other, limit, after)
        .await?;

    Ok(Json(result))
}
