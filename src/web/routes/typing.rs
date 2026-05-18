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
    ensure_room_member_strict(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to access typing status",
    )
    .await
}

async fn write_typing_ephemeral(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    typing_user_ids: &[String],
    timeout_ms: i64,
) {
    let content = json!({
        "user_ids": typing_user_ids
    });
    let now = chrono::Utc::now().timestamp_millis();
    let _ = sqlx::query(
        r"
        INSERT INTO room_ephemeral (room_id, event_type, user_id, content, stream_id, created_ts, expires_at)
        VALUES ($1, 'm.typing', $2, $3, $4, $5, $6)
        ON CONFLICT (room_id, event_type, user_id) DO UPDATE
        SET content = EXCLUDED.content, stream_id = EXCLUDED.stream_id, created_ts = EXCLUDED.created_ts, expires_at = EXCLUDED.expires_at
        ",
    )
    .bind(room_id)
    .bind(user_id)
    .bind(&content)
    .bind(now)
    .bind(now)
    .bind(now + timeout_ms)
    .execute(&*state.services.event_storage.pool)
    .await;
}

async fn clear_typing_ephemeral(state: &AppState, room_id: &str, user_id: &str) {
    let _ = sqlx::query(
        r"
        DELETE FROM room_ephemeral
        WHERE room_id = $1 AND event_type = 'm.typing' AND user_id = $2
        ",
    )
    .bind(room_id)
    .bind(user_id)
    .execute(&*state.services.event_storage.pool)
    .await;
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
        return Err(ApiError::forbidden(
            "Cannot set typing for other users".to_string(),
        ));
    }

    ensure_typing_room_access(&state, &auth_user, &room_id).await?;

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

        write_typing_ephemeral(
            &state,
            &room_id,
            &user_id,
            std::slice::from_ref(&user_id),
            timeout as i64,
        )
        .await;

        let edu = serde_json::json!({
            "edu_type": "m.typing",
            "room_id": room_id,
            "content": {
                "user_ids": [user_id]
            }
        });
        let _ = state
            .services
            .event_broadcaster
            .broadcast_edu_to_room(
                &room_id,
                &edu,
                state
                    .services
                    .config
                    .server
                    .server_name
                    .as_deref()
                    .unwrap_or("localhost"),
            )
            .await;

        let expires_at = chrono::Utc::now().timestamp_millis() + timeout as i64;

        Ok(Json(json!({
            "timeout": timeout,
            "expires_at": expires_at
        })))
    } else {
        state
            .services
            .typing_service
            .clear_typing(&room_id, &user_id)
            .await?;

        clear_typing_ephemeral(&state, &room_id, &user_id).await;

        let edu = serde_json::json!({
            "edu_type": "m.typing",
            "room_id": room_id,
            "content": {
                "user_ids": []
            }
        });
        let _ = state
            .services
            .event_broadcaster
            .broadcast_edu_to_room(
                &room_id,
                &edu,
                state
                    .services
                    .config
                    .server
                    .server_name
                    .as_deref()
                    .unwrap_or("localhost"),
            )
            .await;

        Ok(Json(json!({
            "typing": false
        })))
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
    auth_user: AuthenticatedUser,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ensure_typing_room_access(&state, &auth_user, &room_id).await?;

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
    auth_user: AuthenticatedUser,
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
        ensure_typing_room_access(&state, &auth_user, &room_id).await?;

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
            "/_matrix/client/r0/rooms/{room_id}/typing/{user_id}",
            put(set_typing).get(get_user_typing),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/typing",
            get(get_typing_users),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/typing",
            get(get_typing_users),
        )
        .route("/_matrix/client/v3/rooms/typing", post(bulk_get_typing))
        .route("/_matrix/client/r0/rooms/typing", post(bulk_get_typing))
        .with_state(state)
}

pub fn typing_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [
        (
            Method::PUT,
            "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
        ),
        (
            Method::GET,
            "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
        ),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/typing"),
        (Method::POST, "/_matrix/client/v3/rooms/typing"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "typing"))
    .collect()
}
