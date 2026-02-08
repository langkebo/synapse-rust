use super::{AppState, AuthenticatedUser};
use crate::common::ApiError;
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
            "/_matrix/client/r0/voice/stats",
            get(get_current_user_voice_stats),
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
        .route(
            "/_matrix/client/r0/voice/config",
            get(get_voice_config),
        )
        .route(
            "/_matrix/client/r0/voice/convert",
            post(convert_voice_message),
        )
        .route(
            "/_matrix/client/r0/voice/optimize",
            post(optimize_voice_message),
        )
}

#[axum::debug_handler]
async fn get_voice_config() -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "supported_formats": ["audio/ogg", "audio/mpeg", "audio/wav"],
        "max_size_bytes": 104857600, // 100MB
        "max_duration_ms": 600000,   // 10 minutes
        "default_sample_rate": 48000,
        "default_channels": 2
    })))
}

#[axum::debug_handler]
async fn convert_voice_message(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // Stub implementation: Just echo back mock success
    // In a real implementation, this would use ffmpeg
    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Conversion simulation successful. (Backend FFmpeg not connected)",
        "converted_content": null // Placeholder
    })))
}

#[axum::debug_handler]
async fn optimize_voice_message(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // Stub implementation
    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Optimization simulation successful. (Backend FFmpeg not connected)",
        "optimized_content": null // Placeholder
    })))
}

#[axum::debug_handler]
async fn upload_voice_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &state.services.voice_service;

    let content_base64 = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let engine = base64::engine::general_purpose::STANDARD;
    let content = match engine.decode(content_base64) {
        Ok(data) => data,
        Err(_) => {
            return Err(ApiError::bad_request("Invalid base64 content".to_string()));
        }
    };

    // 1. Size Validation (Max 10MB)
    const MAX_SIZE: usize = 10 * 1024 * 1024;
    if content.len() > MAX_SIZE {
        return Err(ApiError::bad_request(format!(
            "Voice message too large. Max size is {} bytes",
            MAX_SIZE
        )));
    }

    // 2. Magic Number Validation (File Type Check)
    if let Some(kind) = infer::get(&content) {
        if !kind.mime_type().starts_with("audio/") && kind.mime_type() != "application/ogg" {
             return Err(ApiError::bad_request(format!(
                "Invalid file type: {}. Expected audio file.",
                kind.mime_type()
            )));
        }
    } else {
        // Fallback: if infer cannot detect, we might reject or allow if we trust content_type (unsafe)
        // For P0 security, we should probably reject or strictly check content_type against known headers if infer fails (e.g. some raw formats)
        // But infer is quite good. Let's reject unknown.
        return Err(ApiError::bad_request(
            "Could not determine file type. Please upload a valid audio file.".to_string(),
        ));
    }

    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("audio/ogg");

    // 3. Duration Validation
    let duration_ms = body
        .get("duration_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    
    if duration_ms <= 0 {
         return Err(ApiError::bad_request(
            "Duration must be positive".to_string(),
        ));
    }

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
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_voice_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_voice_message(&message_id).await {
        Ok(Some((content, content_type))) => {
            let engine = base64::engine::general_purpose::STANDARD;
            let content_base64 = engine.encode(&content);
            Ok(Json(serde_json::json!({
                "message_id": message_id,
                "content": content_base64,
                "content_type": content_type,
                "size": content.len()
            })))
        }
        Ok(None) => Err(ApiError::not_found("Voice message not found".to_string())),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn delete_voice_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(message_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &state.services.voice_service;

    match voice_service
        .delete_voice_message(&auth_user.user_id, &message_id)
        .await
    {
        Ok(deleted) => Ok(Json(serde_json::json!({
            "deleted": deleted,
            "message_id": message_id
        }))),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_user_voice_messages(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_user_messages(&user_id, 50, 0).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_room_voice_messages(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_room_messages(&room_id, 50).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_user_voice_stats(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &state.services.voice_service;

    match voice_service.get_user_stats(&user_id, None, None).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

#[axum::debug_handler]
async fn get_current_user_voice_stats(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &state.services.voice_service;

    match voice_service
        .get_user_stats(&auth_user.user_id, None, None)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}
