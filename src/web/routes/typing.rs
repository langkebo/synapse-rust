// Typing Routes - 打字提示路由
// Typing indicator management

use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type TypingState = Arc<RwLock<HashMap<String, (bool, u64)>>>;

static TYPING_STATE: std::sync::OnceLock<TypingState> = std::sync::OnceLock::new();

fn get_typing_state() -> &'static TypingState {
    TYPING_STATE.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

fn make_key(room_id: &str, user_id: &str) -> String {
    format!("{}:{}", room_id, user_id)
}

/// Set typing indicator
/// PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}
pub async fn set_typing(
    State(_state): State<AppState>,
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

    let key = make_key(&room_id, &user_id);
    let mut typing = get_typing_state().write().await;
    if is_typing {
        typing.insert(key, (true, timeout));
    } else {
        typing.remove(&key);
    }

    Ok(Json(json!({})))
}

/// Get typing users in a room
/// GET /_matrix/client/v3/rooms/{room_id}/typing
pub async fn get_typing_users(
    State(_state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let typing = get_typing_state().read().await;
    let users: Vec<String> = typing
        .keys()
        .filter(|k| k.starts_with(&format!("{}:", room_id)))
        .map(|k| k.split(':').nth(1).unwrap_or("").to_string())
        .collect();
    Ok(Json(json!({ "typing": users })))
}

/// Get user typing
/// GET /_matrix/client/r0/rooms/{room_id}/typing/{user_id}
pub async fn get_user_typing(
    State(_state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let key = make_key(&room_id, &user_id);
    let typing = get_typing_state().read().await;
    let is_typing = typing.contains_key(&key);
    Ok(Json(json!({ "typing": is_typing })))
}

/// Bulk get typing status
/// POST /_matrix/client/v3/rooms/typing
pub async fn bulk_get_typing(
    State(_state): State<AppState>,
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

    let typing = get_typing_state().read().await;
    let mut result = serde_json::Map::new();
    for room_id in room_ids {
        let users: Vec<String> = typing
            .keys()
            .filter(|k| k.starts_with(&format!("{}:", room_id)))
            .map(|k| k.split(':').nth(1).unwrap_or("").to_string())
            .collect();
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
