use super::{AppState, AuthenticatedUser};
use crate::services::PrivateChatService;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::Value;

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
async fn get_dm_rooms(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(
        &service_container,
        &pool,
        state.services.search_service.clone(),
    );

    match private_chat_service.get_sessions(&auth_user.user_id).await {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string(),
            "rooms": []
        })),
    }
}

#[axum::debug_handler]
async fn create_dm_room(State(_state): State<AppState>, Json(_body): Json<Value>) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": "!dm:localhost"
    }))
}

#[axum::debug_handler]
async fn get_dm_room_details(
    State(_state): State<AppState>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": _room_id,
        "members": [],
        "is_dm": true
    }))
}

#[axum::debug_handler]
async fn get_unread_notifications(
    State(_state): State<AppState>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "notification_count": 0,
        "highlight_count": 0
    }))
}

#[axum::debug_handler]
async fn get_sessions(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(
        &service_container,
        &pool,
        state.services.search_service.clone(),
    );

    match private_chat_service.get_sessions(&auth_user.user_id).await {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string(),
            "sessions": []
        })),
    }
}

#[axum::debug_handler]
async fn create_session(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(
        &service_container,
        &pool,
        state.services.search_service.clone(),
    );

    let other_user_id = body
        .get("other_user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match private_chat_service
        .get_or_create_session(&auth_user.user_id, other_user_id)
        .await
    {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[axum::debug_handler]
async fn get_session_details(
    State(_state): State<AppState>,
    Path(_session_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "session_id": _session_id,
        "participants": [],
        "messages": []
    }))
}

#[axum::debug_handler]
async fn delete_session(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(&service_container, &pool, state.services.search_service.clone());

    match private_chat_service
        .delete_session(&auth_user.user_id, &session_id)
        .await
    {
        Ok(_) => Json(serde_json::json!({})),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[axum::debug_handler]
async fn get_session_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(&service_container, &pool, state.services.search_service.clone());

    match private_chat_service
        .get_messages(&auth_user.user_id, &session_id, 50, None)
        .await
    {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string(),
            "messages": []
        })),
    }
}

#[axum::debug_handler]
async fn send_session_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(&service_container, &pool, state.services.search_service.clone());

    let message_type = body
        .get("message_type")
        .and_then(|v| v.as_str())
        .unwrap_or("m.text");
    let default_content = serde_json::json!({});
    let content = body.get("content").unwrap_or(&default_content);
    let encrypted = body.get("encrypted_content").and_then(|v| v.as_str());

    match private_chat_service
        .send_message(
            &auth_user.user_id,
            &session_id,
            message_type,
            content,
            encrypted,
        )
        .await
    {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[axum::debug_handler]
async fn delete_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(message_id): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(&service_container, &pool, state.services.search_service.clone());

    match private_chat_service
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
) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(&service_container, &pool, state.services.search_service.clone());

    match private_chat_service
        .mark_session_read(&auth_user.user_id, &message_id)
        .await
    {
        Ok(_) => Json(serde_json::json!({})),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[axum::debug_handler]
async fn get_unread_count(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(&service_container, &pool, state.services.search_service.clone());

    match private_chat_service
        .get_unread_count(&auth_user.user_id)
        .await
    {
        Ok(count) => Json(serde_json::json!({
            "unread_count": count
        })),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string(),
            "unread_count": 0
        })),
    }
}

#[axum::debug_handler]
async fn search_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Json<Value> {
    let pool = state.services.user_storage.pool.clone();
    let service_container = state.services.clone();
    let private_chat_service = PrivateChatService::new(&service_container, &pool, state.services.search_service.clone());

    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

    match private_chat_service
        .search_messages(&auth_user.user_id, query, limit)
        .await
    {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string(),
            "results": []
        })),
    }
}
