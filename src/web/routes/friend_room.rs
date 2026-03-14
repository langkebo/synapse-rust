use crate::common::ApiError;
use crate::web::routes::{validate_user_id, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub fn create_friend_router(state: AppState) -> Router<AppState> {
    Router::new()
        // v3 路径
        .route("/_matrix/client/v3/friends", get(get_friends))
        // v1 和 r0 路径 - 主路由
        .route("/_matrix/client/v1/friends", get(get_friends))
        .route("/_matrix/client/v1/friends", post(send_friend_request))
        .route("/_matrix/client/r0/friendships", get(get_friends))
        .route("/_matrix/client/r0/friendships", post(send_friend_request))
        // 好友请求
        .route(
            "/_matrix/client/v1/friends/request",
            post(send_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/request/received",
            get(get_received_requests),
        )
        .route(
            "/_matrix/client/v1/friends/request/{user_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/request/{user_id}/reject",
            post(reject_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/request/{user_id}/cancel",
            post(cancel_friend_request),
        )
        // r0 兼容路由
        .route(
            "/_matrix/client/r0/friends/request",
            post(send_friend_request),
        )
        .route(
            "/_matrix/client/r0/friends/request/received",
            get(get_received_requests),
        )
        .route(
            "/_matrix/client/r0/friends/request/{user_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/_matrix/client/r0/friends/request/{user_id}/reject",
            post(reject_friend_request),
        )
        .route(
            "/_matrix/client/r0/friends/request/{user_id}/cancel",
            post(cancel_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/requests/incoming",
            get(get_incoming_requests),
        )
        .route(
            "/_matrix/client/v1/friends/requests/outgoing",
            get(get_outgoing_requests),
        )
        .route(
            "/_matrix/client/r0/friends/requests/incoming",
            get(get_incoming_requests),
        )
        .route(
            "/_matrix/client/r0/friends/requests/outgoing",
            get(get_outgoing_requests),
        )
        .route(
            "/_matrix/client/v1/friends/check/{user_id}",
            get(check_friendship),
        )
        .route(
            "/_matrix/client/r0/friends/check/{user_id}",
            get(check_friendship),
        )
        .route(
            "/_matrix/client/v1/friends/suggestions",
            get(get_friend_suggestions),
        )
        .route(
            "/_matrix/client/r0/friends/suggestions",
            get(get_friend_suggestions),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}",
            delete(remove_friend),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}",
            delete(remove_friend),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/note",
            put(update_friend_note),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/note",
            put(update_friend_note),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/status",
            get(get_friend_status),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/status",
            put(update_friend_status),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/status",
            get(get_friend_status),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/status",
            put(update_friend_status),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/info",
            get(get_friend_info),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/info",
            get(get_friend_info),
        )
        // 好友分组
        .route("/_matrix/client/v1/friends/groups", get(get_friend_groups))
        .route("/_matrix/client/v1/friends/groups", post(create_friend_group))
        .route("/_matrix/client/r0/friends/groups", get(get_friend_groups))
        .route("/_matrix/client/r0/friends/groups", post(create_friend_group))
        .route("/_matrix/client/v1/friends/groups/{group_id}", delete(delete_friend_group))
        .route("/_matrix/client/r0/friends/groups/{group_id}", delete(delete_friend_group))
        .route("/_matrix/client/v1/friends/groups/{group_id}/name", put(rename_friend_group))
        .route("/_matrix/client/r0/friends/groups/{group_id}/name", put(rename_friend_group))
        .route("/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}", post(add_friend_to_group))
        .route("/_matrix/client/r0/friends/groups/{group_id}/add/{user_id}", post(add_friend_to_group))
        .route("/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}", delete(remove_friend_from_group))
        .route("/_matrix/client/r0/friends/groups/{group_id}/remove/{user_id}", delete(remove_friend_from_group))
        .route("/_matrix/client/v1/friends/groups/{group_id}/friends", get(get_friends_in_group))
        .route("/_matrix/client/r0/friends/groups/{group_id}/friends", get(get_friends_in_group))
        .route("/_matrix/client/v1/friends/{user_id}/groups", get(get_groups_for_user))
        .route("/_matrix/client/r0/friends/{user_id}/groups", get(get_groups_for_user))
        .with_state(state)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddFriendRequest {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FriendRequest {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub message: Option<String>,
    pub timestamp: i64,
    pub status: FriendRequestStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FriendRequestStatus {
    Pending,
    Accepted,
    Rejected,
    Cancelled,
}

async fn get_friends(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let friends = state
        .services
        .friend_room_service
        .get_friends(&auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "friends": friends,
        "total": friends.len()
    })))
}

async fn send_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<AddFriendRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&body.user_id)?;

    if body.user_id == auth_user.user_id {
        return Err(ApiError::bad_request(
            "Cannot send friend request to yourself".to_string(),
        ));
    }

    let room_id = state
        .services
        .friend_room_service
        .add_friend(&auth_user.user_id, &body.user_id)
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "status": "pending"
    })))
}

async fn accept_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(requester_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&requester_id)?;

    let room_id = state
        .services
        .friend_room_service
        .add_friend(&auth_user.user_id, &requester_id)
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "status": "accepted"
    })))
}

async fn reject_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(requester_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&requester_id)?;

    state
        .services
        .friend_room_service
        .reject_friend_request(&auth_user.user_id, &requester_id)
        .await?;

    Ok(Json(json!({ "status": "rejected" })))
}

async fn cancel_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(target_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&target_id)?;

    state
        .services
        .friend_room_service
        .cancel_friend_request(&auth_user.user_id, &target_id)
        .await?;

    Ok(Json(json!({ "status": "cancelled" })))
}

async fn get_incoming_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let requests = state
        .services
        .friend_room_service
        .get_incoming_requests(&auth_user.user_id)
        .await?;

    Ok(Json(json!({ "requests": requests })))
}

async fn get_outgoing_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let requests = state
        .services
        .friend_room_service
        .get_outgoing_requests(&auth_user.user_id)
        .await?;

    Ok(Json(json!({ "requests": requests })))
}

async fn remove_friend(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    state
        .services
        .friend_room_service
        .remove_friend(&auth_user.user_id, &friend_id)
        .await?;

    Ok(Json(json!({})))
}

async fn update_friend_note(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
    Json(body): Json<UpdateNoteRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    if body.note.len() > 1000 {
        return Err(ApiError::bad_request(
            "Note exceeds maximum length of 1000 characters".to_string(),
        ));
    }

    state
        .services
        .friend_room_service
        .update_friend_note(&auth_user.user_id, &friend_id, &body.note)
        .await?;

    Ok(Json(json!({})))
}

async fn update_friend_status(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
    Json(body): Json<UpdateStatusRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    let valid_statuses = ["favorite", "normal", "blocked", "hidden"];
    if !valid_statuses.contains(&body.status.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid status. Valid values: {}",
            valid_statuses.join(", ")
        )));
    }

    state
        .services
        .friend_room_service
        .update_friend_status(&auth_user.user_id, &friend_id, &body.status)
        .await?;

    Ok(Json(json!({})))
}

async fn get_friend_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    let info = state
        .services
        .friend_room_service
        .get_friend_info(&auth_user.user_id, &friend_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Friend {} not found", friend_id)))?;

    Ok(Json(info))
}

async fn get_received_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let requests = state
        .services
        .friend_room_service
        .get_incoming_requests(&auth_user.user_id)
        .await?;

    Ok(Json(json!({ "requests": requests })))
}

async fn get_friend_status(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    let status = state
        .services
        .friend_room_service
        .get_friend_status(&auth_user.user_id, &friend_id)
        .await?;

    Ok(Json(status))
}

async fn check_friendship(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(target_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&target_id)?;

    let is_friend = state
        .services
        .friend_room_service
        .check_friendship(&auth_user.user_id, &target_id)
        .await?;

    Ok(Json(json!({
        "user_id": target_id,
        "is_friend": is_friend
    })))
}

async fn get_friend_suggestions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let suggestions = state
        .services
        .friend_room_service
        .get_friend_suggestions(&auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "suggestions": suggestions
    })))
}

// 好友分组相关处理函数

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenameGroupRequest {
    pub name: String,
}

async fn get_friend_groups(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let groups = state
        .services
        .friend_room_service
        .get_friend_groups(&auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "groups": groups
    })))
}

async fn create_friend_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.name.is_empty() || body.name.len() > 50 {
        return Err(ApiError::bad_request(
            "Group name must be between 1 and 50 characters".to_string(),
        ));
    }

    let group = state
        .services
        .friend_room_service
        .create_friend_group(&auth_user.user_id, &body.name)
        .await?;

    Ok(Json(group))
}

async fn delete_friend_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .friend_room_service
        .delete_friend_group(&auth_user.user_id, &group_id)
        .await?;

    Ok(Json(json!({})))
}

async fn rename_friend_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(group_id): Path<String>,
    Json(body): Json<RenameGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.name.is_empty() || body.name.len() > 50 {
        return Err(ApiError::bad_request(
            "Group name must be between 1 and 50 characters".to_string(),
        ));
    }

    state
        .services
        .friend_room_service
        .rename_friend_group(&auth_user.user_id, &group_id, &body.name)
        .await?;

    Ok(Json(json!({})))
}

async fn add_friend_to_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    state
        .services
        .friend_room_service
        .add_friend_to_group(&auth_user.user_id, &group_id, &user_id)
        .await?;

    Ok(Json(json!({})))
}

async fn remove_friend_from_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    state
        .services
        .friend_room_service
        .remove_friend_from_group(&auth_user.user_id, &group_id, &user_id)
        .await?;

    Ok(Json(json!({})))
}

async fn get_friends_in_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let friends = state
        .services
        .friend_room_service
        .get_friends_in_group(&auth_user.user_id, &group_id)
        .await?;

    Ok(Json(json!({
        "friends": friends
    })))
}

async fn get_groups_for_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let groups = state
        .services
        .friend_room_service
        .get_groups_for_user(&auth_user.user_id, &user_id)
        .await?;

    Ok(Json(json!({
        "groups": groups
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_friend_request_serialization() {
        let req = AddFriendRequest {
            user_id: "@test:example.com".to_string(),
            message: Some("Hello!".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("@test:example.com"));
    }

    #[test]
    fn test_update_note_request_serialization() {
        let req = UpdateNoteRequest {
            note: "Best friend".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Best friend"));
    }

    #[test]
    fn test_friend_request_status_serialization() {
        let status = FriendRequestStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending\"");

        let status = FriendRequestStatus::Accepted;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"accepted\"");
    }
}
