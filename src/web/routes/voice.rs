use super::{ensure_room_member, AppState, AuthenticatedUser};
use crate::common::ApiError;
use crate::services::voice_service::VoiceMessageUploadParams;
use axum::{
    extract::State,
    routing::{get, post},
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
        .route("/_matrix/client/r0/voice/config", get(get_voice_config))
}

#[axum::debug_handler]
async fn get_voice_config() -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "supported_formats": ["audio/ogg", "audio/mpeg", "audio/wav", "audio/webm", "audio/mp4", "audio/aac", "audio/flac"],
        "max_size_bytes": 52428800,
        "max_duration_ms": 600000,
        "content_type": "m.audio",
        "voice_extension": "org.matrix.msc3245.voice"
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

    const MAX_SIZE: usize = 50 * 1024 * 1024;
    if content.len() > MAX_SIZE {
        return Err(ApiError::bad_request(format!(
            "Voice message too large. Max size is {} bytes",
            MAX_SIZE
        )));
    }

    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("audio/ogg");

    if let Some(kind) = infer::get(&content) {
        if !kind.mime_type().starts_with("audio/") && kind.mime_type() != "application/ogg" {
            return Err(ApiError::bad_request(format!(
                "Invalid file type: {}. Expected audio file.",
                kind.mime_type()
            )));
        }
    } else if !content_type.starts_with("audio/") && content_type != "application/ogg" {
        return Err(ApiError::bad_request(format!(
            "Invalid content_type: {}. Expected audio type.",
            content_type
        )));
    }

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

    if let Some(target_room_id) = room_id.filter(|id| !id.is_empty()) {
        ensure_room_member(
            &state,
            &auth_user,
            target_room_id,
            "You must be a member of this room to upload voice messages",
        )
        .await?;
    }

    let waveform = body.get("waveform").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_u64().map(|n| n as u16))
            .collect()
    });

    match voice_service
        .upload_voice_message(VoiceMessageUploadParams {
            user_id: auth_user.user_id,
            room_id: room_id.map(|s| s.to_string()),
            content,
            content_type: content_type.to_string(),
            duration_ms,
            waveform,
        })
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}
