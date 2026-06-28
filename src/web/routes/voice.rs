#![allow(clippy::unused_async)]
use super::{ensure_room_member, validate_user_id, AppState, AuthenticatedUser};
use crate::common::ApiError;
use synapse_services::voice_service::VoiceMessageUploadParams;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct VoiceListQuery {
    pub limit: Option<i64>,
    pub from: Option<i64>,
}

pub fn create_voice_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/r0/voice/upload", post(upload_voice_message))
        .route("/_matrix/client/r0/voice/config", get(get_voice_config))
        .route("/_matrix/client/v1/voice/config", get(get_voice_config))
        .route("/_matrix/client/v1/voice/upload", post(upload_voice_message))
        .route("/_matrix/client/v1/voice/stats", get(get_voice_stats))
        .route("/_matrix/client/v1/voice/room/{room_id}/stats", get(get_room_voice_stats))
        .route("/_matrix/client/v1/voice/user/{user_id}/stats", get(get_user_voice_stats))
        .route("/_matrix/client/v3/voice/upload", post(upload_voice_message))
        .route("/_matrix/client/v3/voice/config", get(get_voice_config))
        .route("/_matrix/client/v3/voice/stats", get(get_voice_stats))
        .route("/_matrix/client/v3/voice/room/{room_id}/stats", get(get_room_voice_stats))
        .route("/_matrix/client/v3/voice/user/{user_id}/stats", get(get_user_voice_stats))
        .route("/_matrix/client/v3/voice/room/{room_id}", get(get_room_voice_messages))
        .route("/_matrix/client/v3/voice/user/{user_id}", get(get_user_voice_messages))
        .route("/_matrix/client/v3/voice/{media_id}", get(get_voice_message_content))
        .route("/_matrix/client/v3/voice/{media_id}/convert", post(convert_voice_message))
        .route("/_matrix/client/v3/voice/{media_id}/optimize", post(optimize_voice_message))
        .route("/_matrix/client/v3/voice/{media_id}/transcription", post(transcribe_voice_message))
}

pub fn voice_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [
        (Method::POST, "/_matrix/client/r0/voice/upload"),
        (Method::GET, "/_matrix/client/r0/voice/config"),
        (Method::GET, "/_matrix/client/v1/voice/config"),
        (Method::POST, "/_matrix/client/v1/voice/upload"),
        (Method::GET, "/_matrix/client/v1/voice/stats"),
        (Method::GET, "/_matrix/client/v1/voice/room/{room_id}/stats"),
        (Method::GET, "/_matrix/client/v1/voice/user/{user_id}/stats"),
        (Method::POST, "/_matrix/client/v3/voice/upload"),
        (Method::GET, "/_matrix/client/v3/voice/config"),
        (Method::GET, "/_matrix/client/v3/voice/stats"),
        (Method::GET, "/_matrix/client/v3/voice/room/{room_id}/stats"),
        (Method::GET, "/_matrix/client/v3/voice/user/{user_id}/stats"),
        (Method::GET, "/_matrix/client/v3/voice/room/{room_id}"),
        (Method::GET, "/_matrix/client/v3/voice/user/{user_id}"),
        (Method::GET, "/_matrix/client/v3/voice/{media_id}"),
        (Method::POST, "/_matrix/client/v3/voice/{media_id}/convert"),
        (Method::POST, "/_matrix/client/v3/voice/{media_id}/optimize"),
        (Method::POST, "/_matrix/client/v3/voice/{media_id}/transcription"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "voice"))
    .collect()
}

#[axum::debug_handler]
async fn get_voice_config() -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "enabled": true,
        "max_duration": 600,
        "allowed_formats": ["audio/ogg", "audio/mpeg", "audio/wav", "audio/webm", "audio/mp4", "audio/aac", "audio/flac"],
        "supported_formats": ["audio/ogg", "audio/mpeg", "audio/wav", "audio/webm", "audio/mp4", "audio/aac", "audio/flac"],
        "max_size_bytes": 52428800,
        "max_duration_ms": 60_0000,
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
    let voice_service = &state.services.extensions.voice_service;

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
        return Err(ApiError::bad_request(format!("Voice message too large. Max size is {} bytes", MAX_SIZE)));
    }

    let content_type = body.get("content_type").and_then(|v| v.as_str()).unwrap_or("audio/ogg");

    if let Some(kind) = infer::get(&content) {
        if !kind.mime_type().starts_with("audio/") && kind.mime_type() != "application/ogg" {
            return Err(ApiError::bad_request(format!(
                "Invalid file type: {}. Expected audio file.",
                kind.mime_type()
            )));
        }
    } else if !content_type.starts_with("audio/") && content_type != "application/ogg" {
        return Err(ApiError::bad_request(format!("Invalid content_type: {}. Expected audio type.", content_type)));
    }

    let duration_ms = body.get("duration_ms").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

    if duration_ms <= 0 {
        return Err(ApiError::bad_request("Duration must be positive".to_string()));
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

    let waveform = body
        .get("waveform")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u16)).collect());

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

#[axum::debug_handler]
async fn get_voice_stats(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Result<Json<Value>, ApiError> {
    let stats = state.services.extensions.voice_service.get_voice_stats(&auth_user.user_id).await?;
    Ok(Json(stats))
}

#[axum::debug_handler]
async fn get_room_voice_stats(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member(&state, &auth_user, &room_id, "You must be a member of this room to view voice stats").await?;
    let stats = state.services.extensions.voice_service.get_room_voice_stats(&room_id).await?;
    Ok(Json(stats))
}

#[axum::debug_handler]
async fn get_user_voice_stats(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    let stats = state.services.extensions.voice_service.get_user_voice_stats(&user_id).await?;
    Ok(Json(stats))
}

#[axum::debug_handler]
async fn get_room_voice_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(query): Query<VoiceListQuery>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member(&state, &auth_user, &room_id, "You must be a member of this room to view voice messages")
        .await?;
    let limit = query.limit.unwrap_or(50).min(100);
    let result = state.services.extensions.voice_service.get_room_voice_messages(&room_id, limit, query.from).await?;
    Ok(Json(result))
}

#[axum::debug_handler]
async fn get_user_voice_messages(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Query(query): Query<VoiceListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(50).min(100);
    let result = state.services.extensions.voice_service.get_user_voice_messages(&user_id, limit, query.from).await?;
    Ok(Json(result))
}

#[axum::debug_handler]
async fn get_voice_message_content(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = state.services.extensions.voice_service.get_voice_message_content(&media_id).await?;
    Ok(Json(result))
}

#[axum::debug_handler]
async fn convert_voice_message(
    _state: State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::unrecognized(
        "Voice conversion is handled client-side per MSC3245. Server-side processing is not supported",
    ))
}

#[axum::debug_handler]
async fn optimize_voice_message(
    _state: State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::unrecognized(
        "Voice optimization is handled client-side per MSC3245. Server-side processing is not supported",
    ))
}

#[axum::debug_handler]
async fn transcribe_voice_message(
    _state: State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::unrecognized(
        "Voice transcription is handled client-side per MSC3245. Use Web Speech API or local Whisper model on the client",
    ))
}
