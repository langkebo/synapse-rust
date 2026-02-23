use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::common::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser, validate_user_id};

pub fn create_friend_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/friends", get(get_friends))
        .route("/_matrix/client/v1/friends/request", post(send_friend_request))
        .route("/_matrix/client/v1/friends/request/{user_id}/accept", post(accept_friend_request))
        .route("/_matrix/client/v1/friends/request/{user_id}/reject", post(reject_friend_request))
        .route("/_matrix/client/v1/friends/request/{user_id}/cancel", post(cancel_friend_request))
        .route("/_matrix/client/v1/friends/requests/incoming", get(get_incoming_requests))
        .route("/_matrix/client/v1/friends/requests/outgoing", get(get_outgoing_requests))
        .route("/_matrix/client/v1/friends/{user_id}", delete(remove_friend))
        .route("/_matrix/client/v1/friends/{user_id}/note", put(update_friend_note))
        .route("/_matrix/client/v1/friends/{user_id}/status", put(update_friend_status))
        .route("/_matrix/client/v1/friends/{user_id}/info", get(get_friend_info))
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
        return Err(ApiError::bad_request("Cannot send friend request to yourself".to_string()));
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
        return Err(ApiError::bad_request("Note exceeds maximum length of 1000 characters".to_string()));
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
