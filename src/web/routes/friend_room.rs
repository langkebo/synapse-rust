//! Friend Room API Routes
//!
//! API endpoints for the room-based friend system.
//! These endpoints provide a Matrix-compatible interface for managing friends.

use super::AppState;
use crate::common::ApiError;
use crate::services::friend_room_service::{FriendRoomService, FriendResult};
use crate::storage::friend_room::EVENT_TYPE_FRIENDS_LIST;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use validator::Validate;

/// Create the friend room router
pub fn create_friend_room_router() -> Router<AppState> {
    Router::new()
        // Friend list management
        .route("/_matrix/client/v1/friends/room", get(get_friend_list_room))
        .route("/_matrix/client/v1/friends", get(get_friends))
        .route("/_matrix/client/v1/friends", delete(remove_friend))

        // Friend requests (room-based)
        .route("/_matrix/client/v1/friends/request", post(send_friend_request))
        .route("/_matrix/client/v1/friends/requests", get(get_pending_requests))
        .route(
            "/_matrix/client/v1/friends/request/:request_id/accept",
            post(accept_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/request/:request_id/decline",
            post(decline_friend_request),
        )

        // Direct message rooms
        .route("/_matrix/client/v1/friends/dm/:user_id", get(get_dm_room))
        .route("/_matrix/client/v1/friends/dm/:user_id", post(create_dm_room))

        // Friend check
        .route("/_matrix/client/v1/friends/check/:user_id", get(check_friendship))
}

// ==============================================================================
// Request/Response Types
// ==============================================================================

#[derive(Debug, Deserialize, Validate)]
pub struct SendFriendRequestRequest {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateDmRoomRequest {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
    pub is_private: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RemoveFriendRequest {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
}

// ==============================================================================
// Helper Functions
// ==============================================================================

/// Get the FriendRoomService from the app state
fn get_friend_room_service(state: &AppState) -> FriendResult<FriendRoomService> {
    Ok(FriendRoomService::new(
        &state.services.user_storage.pool,
        state.services.registration_service.clone(),
        state.services.user_storage.clone(),
        state.services.server_name.clone(),
    ))
}

// ==============================================================================
// Route Handlers
// ==============================================================================

/// Get the friend list room ID for the current user
///
/// Returns the room ID of the user's friend list room.
/// This room contains the user's friend relationships as state events.
#[axum::debug_handler]
async fn get_friend_list_room(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let service = get_friend_room_service(&state)?;
    let room_id = service.get_friend_list_room_id(&auth_user.user_id).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "room_type": ROOM_TYPE_FRIEND_LIST,
        "event_type": EVENT_TYPE_FRIENDS_LIST
    })))
}

/// Get the friend list for the current user
///
/// Returns all friends with their profile information.
#[axum::debug_handler]
async fn get_friends(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let service = get_friend_room_service(&state)?;
    let friends = service.get_friends(&auth_user.user_id).await?;

    Ok(Json(friends))
}

/// Send a friend request to another user
///
/// Creates a friend request that the recipient can accept or decline.
/// When accepted, both users will be added to each other's friend lists
/// and a direct message room will be created.
#[axum::debug_handler]
async fn send_friend_request(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
    Json(body): Json<SendFriendRequestRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }

    let service = get_friend_room_service(&state)?;
    let result = service
        .send_friend_request(&auth_user.user_id, &body.user_id, body.message)
        .await?;

    Ok(Json(result))
}

/// Get pending friend requests for the current user
///
/// Returns all friend requests that have been sent to the user
/// and are still pending.
#[axum::debug_handler]
async fn get_pending_requests(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let service = get_friend_room_service(&state)?;
    let requests = service.get_pending_requests(&auth_user.user_id).await?;

    Ok(Json(requests))
}

/// Accept a friend request
///
/// Accepts a pending friend request and creates a bidirectional friendship.
/// A direct message room is also created for the two users.
#[axum::debug_handler]
async fn accept_friend_request(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let request_id_i64: i64 = request_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid request ID format".to_string()))?;

    if request_id_i64 <= 0 {
        return Err(ApiError::bad_request("Invalid request ID".to_string()));
    }

    let service = get_friend_room_service(&state)?;
    let result = service
        .accept_friend_request(request_id_i64, &auth_user.user_id)
        .await?;

    Ok(Json(result))
}

/// Decline a friend request
///
/// Declines a pending friend request.
#[axum::debug_handler]
async fn decline_friend_request(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let request_id_i64: i64 = request_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid request ID format".to_string()))?;

    if request_id_i64 <= 0 {
        return Err(ApiError::bad_request("Invalid request ID".to_string()));
    }

    let service = get_friend_room_service(&state)?;
    let result = service
        .decline_friend_request(request_id_i64, &auth_user.user_id)
        .await?;

    Ok(Json(result))
}

/// Get or create a direct message room with a friend
///
/// Returns the DM room ID for the specified friend.
/// If a DM room doesn't exist, one will be created.
#[axum::debug_handler]
async fn get_dm_room(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let service = get_friend_room_service(&state)?;

    // Default is_private to false
    let result = service
        .get_dm_room(&auth_user.user_id, &user_id, false)
        .await?;

    Ok(Json(result))
}

/// Create a direct message room with a friend
///
/// Creates a new DM room with the specified friend.
/// Optionally marks the room as private/secret.
#[axum::debug_handler]
async fn create_dm_room(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: super::AuthenticatedUser,
    Json(body): Json<CreateDmRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }

    let service = get_friend_room_service(&state)?;
    let is_private = body.is_private.unwrap_or(false);
    let result = service
        .get_dm_room(&auth_user.user_id, &user_id, is_private)
        .await?;

    Ok(Json(result))
}

/// Remove a friend
///
/// Removes the specified user from the current user's friend list.
/// This is a bidirectional operation - the user will also be removed
/// from the other user's friend list.
#[axum::debug_handler]
async fn remove_friend(
    State(state): State<AppState>,
    auth_user: super::AuthenticatedUser,
    Json(body): Json<RemoveFriendRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }

    let service = get_friend_room_service(&state)?;
    service
        .remove_friend(&auth_user.user_id, &body.user_id)
        .await?;

    Ok(Json(json!({
        "status": "removed",
        "message": format!("Removed {} from friends", body.user_id)
    })))
}

/// Check if two users are friends
///
/// Returns the friendship status between the current user and the specified user.
#[axum::debug_handler]
async fn check_friendship(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: super::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let service = get_friend_room_service(&state)?;
    let are_friends = service
        .are_friends(&auth_user.user_id, &user_id)
        .await?;

    Ok(Json(json!({
        "are_friends": are_friends,
        "user_id": user_id
    })))
}

// ==============================================================================
// Constants
// ==============================================================================

use crate::storage::friend_room::ROOM_TYPE_FRIEND_LIST;

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_friend_request_validation() {
        let valid = SendFriendRequestRequest {
            user_id: "@alice:example.com".to_string(),
            message: Some("Hi, let's be friends!".to_string()),
        };
        assert!(valid.validate().is_ok());

        // Empty user_id should fail
        let invalid = SendFriendRequestRequest {
            user_id: "".to_string(),
            message: None,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_remove_friend_validation() {
        let valid = RemoveFriendRequest {
            user_id: "@bob:example.com".to_string(),
        };
        assert!(valid.validate().is_ok());

        let invalid = RemoveFriendRequest {
            user_id: "".to_string(),
        };
        assert!(invalid.validate().is_err());
    }
}
