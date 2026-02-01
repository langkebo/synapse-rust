use super::{AppState, AuthenticatedUser};
use crate::common::ApiError;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::{json, Value};

pub fn create_private_chat_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/r0/dm", get(get_dm_rooms))
        .route("/_matrix/client/r0/createDM", post(create_dm_room))
        .route(
            "/_matrix/client/r0/rooms/{room_id}/dm",
            get(get_dm_room_details),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/unread",
            get(get_unread_notifications),
        )
        .route("/_synapse/enhanced/private/sessions", get(get_sessions))
        .route("/_synapse/enhanced/private/sessions", post(create_session))
        .route(
            "/_synapse/enhanced/private/sessions/{session_id}",
            get(get_session_details),
        )
        .route(
            "/_synapse/enhanced/private/sessions/{session_id}",
            delete(delete_session),
        )
        .route(
            "/_synapse/enhanced/private/sessions/{session_id}/messages",
            get(get_session_messages),
        )
        .route(
            "/_synapse/enhanced/private/sessions/{session_id}/messages",
            post(send_session_message),
        )
        .route(
            "/_synapse/enhanced/private/messages/{message_id}",
            delete(delete_message),
        )
        .route(
            "/_synapse/enhanced/private/messages/{message_id}/read",
            post(mark_message_read),
        )
        .route(
            "/_synapse/enhanced/private/unread-count",
            get(get_unread_count),
        )
        .route("/_synapse/enhanced/private/search", post(search_messages))
}

#[axum::debug_handler]
async fn get_dm_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    match state
        .services
        .private_chat_service
        .get_sessions(&auth_user.user_id)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn create_dm_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let other_user_id = body
        .get("user_id")
        .or(body.get("other_user_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("user_id required".to_string()))?;

    match state
        .services
        .private_chat_storage
        .get_or_create_session(&auth_user.user_id, other_user_id)
        .await
    {
        Ok(session_id) => Ok(Json(serde_json::json!({
            "room_id": session_id
        }))),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_dm_room_details(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    // 1. Try to find in standard rooms first
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(room) = room {
        let members = state
            .services
            .member_storage
            .get_room_members(&room_id, "")
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if !members.iter().any(|m| m.user_id == auth_user.user_id) {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        return Ok(Json(serde_json::json!({
            "room_id": room_id,
            "name": room.name,
            "topic": room.topic,
            "member_count": members.len(),
            "joined_members": members
        })));
    }

    // 2. If not found, try to find in private sessions
    if room_id.starts_with("ps_") {
        let session = state
            .services
            .private_chat_storage
            .get_session_details(&room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room or Session not found".to_string()))?;

        if session.user_id_1 != auth_user.user_id && session.user_id_2 != auth_user.user_id {
            return Err(ApiError::forbidden(
                "You are not a participant of this session".to_string(),
            ));
        }

        let other_user_id = if session.user_id_1 == auth_user.user_id {
            &session.user_id_2
        } else {
            &session.user_id_1
        };
        let other_profile = state
            .services
            .registration_service
            .get_profile(other_user_id)
            .await
            .unwrap_or(json!({"user_id": other_user_id}));

        return Ok(Json(serde_json::json!({
            "room_id": room_id,
            "name": format!("Chat with {}", other_user_id),
            "is_direct": true,
            "participants": [
                {"user_id": auth_user.user_id},
                other_profile
            ],
            "unread_count": session.unread_count.unwrap_or(0)
        })));
    }

    Err(ApiError::not_found("Room not found".to_string()))
}

#[axum::debug_handler]
async fn get_unread_notifications(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    // For now, return a basic unread count from private_sessions if it's a DM
    let session: Option<(i32,)> =
        sqlx::query_as("SELECT unread_count FROM private_sessions WHERE id = $1")
            .bind(&room_id)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "notification_count": session.map(|s| s.0).unwrap_or(0),
        "highlight_count": 0
    })))
}

#[axum::debug_handler]
async fn get_sessions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    match state
        .services
        .private_chat_service
        .get_sessions(&auth_user.user_id)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn create_session(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let other_user_id = body
        .get("other_user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match state
        .services
        .private_chat_service
        .get_or_create_session(&auth_user.user_id, other_user_id)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_session_details(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let details = state
        .services
        .private_chat_service
        .get_session_details(&auth_user.user_id, &session_id)
        .await?;

    Ok(Json(details))
}

#[axum::debug_handler]
async fn delete_session(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state
        .services
        .private_chat_service
        .delete_session(&auth_user.user_id, &session_id)
        .await
    {
        Ok(_) => Ok(Json(serde_json::json!({}))),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_session_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state
        .services
        .private_chat_service
        .get_messages(&auth_user.user_id, &session_id, 50, None)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn send_session_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let message_type = body
        .get("message_type")
        .and_then(|v| v.as_str())
        .unwrap_or("m.text");
    let default_content = serde_json::json!({});
    let content = body.get("content").unwrap_or(&default_content);
    let encrypted = body.get("encrypted_content").and_then(|v| v.as_str());

    match state
        .services
        .private_chat_service
        .send_message(
            &auth_user.user_id,
            &session_id,
            message_type,
            content,
            encrypted,
        )
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn delete_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(message_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state
        .services
        .private_chat_service
        .delete_message(&auth_user.user_id, &message_id)
        .await
    {
        Ok(_) => Ok(Json(serde_json::json!({}))),
        Err(e) => Err(e),
    }
}

#[axum::debug_handler]
async fn mark_message_read(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(message_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state
        .services
        .private_chat_service
        .mark_session_read(&auth_user.user_id, &message_id)
        .await
    {
        Ok(_) => Ok(Json(serde_json::json!({}))),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_unread_count(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    match state
        .services
        .private_chat_service
        .get_unread_count(&auth_user.user_id)
        .await
    {
        Ok(count) => Ok(Json(serde_json::json!({
            "unread_count": count
        }))),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn search_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

    match state
        .services
        .private_chat_service
        .search_messages(&auth_user.user_id, query, limit)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}
