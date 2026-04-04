// Typing Routes - 打字提示路由
// Typing indicator management

use crate::services::TypingService;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};

/// Set typing indicator
/// PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}
pub async fn set_typing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden(
            "Cannot set typing for other users".to_string(),
        ));
    }

    let timeout = body
        .get("timeout")
        .and_then(|v| v.as_i64())
        .unwrap_or(30000) as u64;

    let is_typing = body.get("typing").and_then(|v| v.as_bool()).unwrap_or(true);

    if is_typing {
        state
            .services
            .typing_service
            .set_typing(&room_id, &user_id, timeout)
            .await?;
    } else {
        state
            .services
            .typing_service
            .clear_typing(&room_id, &user_id)
            .await?;
    }

    Ok(Json(json!({})))
}

/// Get typing users in a room
/// GET /_matrix/client/v3/rooms/{room_id}/typing
pub async fn get_typing_users(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let typing = state
        .services
        .typing_service
        .get_typing_users(&room_id)
        .await?;
    let users: Vec<String> = typing.into_keys().collect();
    Ok(Json(json!({ "typing": users })))
}

/// Get user typing
/// GET /_matrix/client/r0/rooms/{room_id}/typing/{user_id}
pub async fn get_user_typing(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let is_typing = state
        .services
        .typing_service
        .get_user_typing(&room_id, &user_id)
        .await?
        .is_some();
    Ok(Json(json!({ "typing": is_typing })))
}

/// Bulk get typing status
/// POST /_matrix/client/v3/rooms/typing
pub async fn bulk_get_typing(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_ids = body
        .get("rooms")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut result = serde_json::Map::new();
    for room_id in room_ids {
        let typing = state
            .services
            .typing_service
            .get_typing_users(&room_id)
            .await?;
        let users: Vec<String> = typing.into_keys().collect();
        result.insert(room_id, json!({ "typing": users }));
    }

    Ok(Json(json!(result)))
}

pub fn create_typing_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
            put(set_typing).get(get_user_typing),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/typing",
            get(get_typing_users),
        )
        .route("/_matrix/client/v3/rooms/typing", post(bulk_get_typing))
        .with_state(state)
}
