// Typing Routes - 打字提示路由
// Typing indicator management

use crate::web::routes::{ensure_room_member_strict, ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};

async fn ensure_typing_room_access(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict(state, auth_user, room_id, "You must be a member of this room to access typing status")
        .await
}

/// Set typing indicator
/// PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}
pub async fn set_typing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot set typing for other users".to_string()));
    }

    ensure_typing_room_access(&state, &auth_user, &room_id).await?;

    let timeout = body.get("timeout").and_then(|v| v.as_i64()).unwrap_or(30000) as u64;

    let is_typing = body.get("typing").and_then(|v| v.as_bool()).unwrap_or(true);

    if is_typing {
        state.services.rooms.typing_service.set_typing(&room_id, &user_id, timeout).await?;

        state
            .services
            .rooms
            .room_service
            .set_typing_ephemeral_event(&room_id, &user_id, std::slice::from_ref(&user_id), timeout as i64)
            .await?;

        let edu = serde_json::json!({
            "edu_type": "m.typing",
            "room_id": room_id,
            "content": {
                "user_ids": [user_id]
            }
        });
        if let Err(e) = state
            .services
            .core
            .event_broadcaster
            .broadcast_edu_to_room(&room_id, &edu, state.services.core.server_name.as_str())
            .await
        {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to broadcast typing EDU to federation — local state already updated"
            );
        }

        Ok(Json(json!({})))
    } else {
        state.services.rooms.typing_service.clear_typing(&room_id, &user_id).await?;

        state.services.rooms.room_service.clear_typing_ephemeral_event(&room_id, &user_id).await?;

        let edu = serde_json::json!({
            "edu_type": "m.typing",
            "room_id": room_id,
            "content": {
                "user_ids": []
            }
        });
        if let Err(e) = state
            .services
            .core
            .event_broadcaster
            .broadcast_edu_to_room(&room_id, &edu, state.services.core.server_name.as_str())
            .await
        {
            ::tracing::warn!(
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Failed to broadcast typing-clear EDU to federation — local state already updated"
            );
        }

        Ok(Json(json!({})))
    }
}

/// Get typing users in a room
/// GET /_matrix/client/v3/rooms/{room_id}/typing
pub async fn get_typing_users(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_typing_room_access(&state, &auth_user, &room_id).await?;

    let typing: std::collections::HashMap<String, u64> =
        state.services.rooms.typing_service.get_typing_users(&room_id).await?;
    let users: Vec<String> = typing.into_keys().collect();
    Ok(Json(json!({ "typing": users })))
}

/// Get user typing
/// GET /_matrix/client/r0/rooms/{room_id}/typing/{user_id}
pub async fn get_user_typing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ensure_typing_room_access(&state, &auth_user, &room_id).await?;

    let typing_ts: Option<u64> = state.services.rooms.typing_service.get_user_typing(&room_id, &user_id).await?;
    let is_typing: bool = typing_ts.is_some();
    Ok(Json(json!({ "typing": is_typing })))
}

/// Bulk get typing status
/// POST /_matrix/client/v3/rooms/typing
pub async fn bulk_get_typing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_ids = body
        .get("rooms")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut result = serde_json::Map::new();
    for room_id in room_ids {
        ensure_typing_room_access(&state, &auth_user, &room_id).await?;

        let typing: std::collections::HashMap<String, u64> =
            state.services.rooms.typing_service.get_typing_users(&room_id).await?;
        let users: Vec<String> = typing.into_keys().collect();
        result.insert(room_id, json!({ "typing": users }));
    }

    Ok(Json(json!(result)))
}

pub fn create_typing_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
            put(set_typing).post(set_typing).get(get_user_typing),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/typing/{user_id}",
            put(set_typing).post(set_typing).get(get_user_typing),
        )
        .route("/_matrix/client/v3/rooms/{room_id}/typing", get(get_typing_users))
        .route("/_matrix/client/r0/rooms/{room_id}/typing", get(get_typing_users))
        .route("/_matrix/client/v3/rooms/typing", post(bulk_get_typing))
        .route("/_matrix/client/r0/rooms/typing", post(bulk_get_typing))
        .with_state(state)
}

pub fn typing_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [
        (Method::PUT, "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}"),
        (Method::POST, "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/typing"),
        (Method::POST, "/_matrix/client/v3/rooms/typing"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "typing"))
    .collect()
}
