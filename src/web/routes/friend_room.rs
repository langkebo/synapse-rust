use axum::{
    extract::{State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use crate::common::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser, validate_user_id};

pub fn create_friend_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/friends", get(get_friends))
        .route("/_matrix/client/v1/friends/request", post(add_friend))
        .with_state(state)
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
    Ok(Json(json!({ "friends": friends })))
}

async fn add_friend(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let friend_id = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("user_id is required".to_string()))?;

    validate_user_id(friend_id)?;

    if friend_id == auth_user.user_id {
        return Err(ApiError::bad_request("Cannot add self as friend".to_string()));
    }

    let room_id = state
        .services
        .friend_room_service
        .add_friend(&auth_user.user_id, friend_id)
        .await?;

    Ok(Json(json!({ "room_id": room_id })))
}
