use super::{AppState, AuthenticatedUser};
use crate::services::VoiceMessageUploadParams;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use base64::Engine;
use serde_json::Value;

pub fn create_voice_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/r0/voice/upload",
            post(upload_voice_message),
        )
        .route(
            "/_matrix/client/r0/voice/{message_id}",
            get(get_voice_message),
        )
        .route(
            "/_matrix/client/r0/voice/{message_id}",
            delete(delete_voice_message),
        )
        .route(
            "/_matrix/client/r0/voice/user/{user_id}",
            get(get_user_voice_messages),
        )
        .route(
            "/_matrix/client/r0/voice/room/{room_id}",
            get(get_room_voice_messages),
        )
        .route(
            "/_matrix/client/r0/voice/user/{user_id}/stats",
            get(get_user_voice_stats),
        )
}

#[axum::debug_handler]
async fn upload_voice_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Json<Value> {
    let voice_service = &state.services.voice_service;

    let content_base64 = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let engine = base64::engine::general_purpose::STANDARD;
    let content = match engine.decode(content_base64) {
        Ok(data) => data,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid base64 content"
            }))
        }
    };

    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("audio/ogg");
    let duration_ms = body
        .get("duration_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let room_id = body.get("room_id").and_then(|v| v.as_str());
    let session_id = body.get("session_id").and_then(|v| v.as_str());

    match voice_service
        .save_voice_message(VoiceMessageUploadParams {
            user_id: auth_user.user_id,
            room_id: room_id.map(|s| s.to_string()),
            session_id: session_id.map(|s| s.to_string()),
            content,
            content_type: content_type.to_string(),
            duration_ms,
        })
        .await
    {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[axum::debug_handler]
async fn get_voice_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
) -> Json<Value> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_voice_message(&message_id).await {
        Ok(Some((content, content_type))) => {
            let engine = base64::engine::general_purpose::STANDARD;
            let content_base64 = engine.encode(&content);
            Json(serde_json::json!({
                "message_id": message_id,
                "content": content_base64,
                "content_type": content_type,
                "size": content.len()
            }))
        }
        Ok(None) => Json(serde_json::json!({
            "error": "Voice message not found"
        })),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[axum::debug_handler]
async fn delete_voice_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(message_id): Path<String>,
) -> Json<Value> {
    let voice_service = &state.services.voice_service;

    match voice_service
        .delete_voice_message(&auth_user.user_id, &message_id)
        .await
    {
        Ok(deleted) => Json(serde_json::json!({
            "deleted": deleted,
            "message_id": message_id
        })),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[axum::debug_handler]
async fn get_user_voice_messages(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Json<Value> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_user_messages(&user_id, 50, 0).await {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string(),
            "messages": []
        })),
    }
}

#[axum::debug_handler]
async fn get_room_voice_messages(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Json<Value> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_room_messages(&room_id, 50).await {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string(),
            "messages": []
        })),
    }
}

#[axum::debug_handler]
async fn get_user_voice_stats(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Json<Value> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_user_stats(&user_id, None, None).await {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}
