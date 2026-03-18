// Typing Routes - 打字提示路由
// Typing indicator management (简化版)

use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, put, post},
    Json, Router,
};
use serde_json::{json, Value};

/// Set typing indicator
/// PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}
pub async fn set_typing(
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((_room_id, user_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // Verify user is the auth user or has permission
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot set typing for other users".to_string()));
    }

    let _timeout = body
        .get("timeout")
        .and_then(|v| v.as_i64())
        .unwrap_or(30000);

    let _is_typing = body
        .get("typing")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(Json(json!({})))
}

/// Get typing users in a room
/// GET /_matrix/client/v3/rooms/{room_id}/typing
pub async fn get_typing_users(
    State(_state): State<AppState>,
    Path(_room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "typing": {} })))
}

/// Get user typing (v1 compatible)
/// GET /_matrix/client/r0/rooms/{room_id}/typing/{user_id}
pub async fn get_user_typing(
    State(_state): State<AppState>,
    Path((_room_id, _user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "typing": false })))
}

/// Bulk get typing status
/// POST /_matrix/client/v3/rooms/typing
pub async fn bulk_get_typing(
    State(_state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _room_ids = body
        .get("rooms")
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    Ok(Json(json!({})))
}

pub fn create_typing_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v3/rooms/{room_id}/typing/{user_id}", put(set_typing).get(get_user_typing))
        .route("/_matrix/client/v3/rooms/{room_id}/typing", get(get_typing_users))
        .route("/_matrix/client/v3/rooms/typing", post(bulk_get_typing))
        .with_state(state)
}
