use super::{AppState, AuthenticatedUser};
use crate::common::ApiError;
use crate::services::VoiceMessageUploadParams;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use base64::Engine;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

static SQL_INJECTION_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\b(SELECT|INSERT|UPDATE|DELETE|DROP|UNION|ALTER|CREATE|TRUNCATE)\b|--|;|' OR '|' AND ')").unwrap()
});

static XSS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(<script>|</script>|<iframe>|javascript:|onload=|onerror=|onmouseover=|alert\(|eval\()").unwrap()
});

fn contains_sql_injection(input: &str) -> bool {
    SQL_INJECTION_PATTERN.is_match(input)
}

fn contains_xss(input: &str) -> bool {
    XSS_PATTERN.is_match(input)
}

fn validate_message_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Message ID cannot be empty".to_string());
    }
    if id.len() > 100 {
        return Err("Message ID must not exceed 100 characters".to_string());
    }
    if contains_sql_injection(id) {
        return Err("Message ID contains invalid characters".to_string());
    }
    if contains_xss(id) {
        return Err("Message ID contains invalid characters".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '.')
    {
        return Err("Message ID contains invalid characters".to_string());
    }
    Ok(())
}

fn validate_audio_format(format: &str, valid_codecs: &[&str]) -> Result<String, String> {
    if format.is_empty() {
        return Err("Format cannot be empty".to_string());
    }
    if !format.starts_with("audio/") {
        return Err("Format must be an audio MIME type (e.g., audio/ogg)".to_string());
    }
    let codec = format.strip_prefix("audio/").unwrap_or("");
    if !valid_codecs.contains(&codec) {
        return Err(format!(
            "Unsupported audio codec: {}. Supported: {}",
            codec,
            valid_codecs.join(", ")
        ));
    }
    if contains_sql_injection(format) || contains_xss(format) {
        return Err("Format contains invalid characters".to_string());
    }
    Ok(codec.to_string())
}

fn validate_bitrate(bitrate: i32) -> Result<(), String> {
    if bitrate < 0 {
        return Err("Bitrate cannot be negative".to_string());
    }
    const MIN_BITRATE: i32 = 64000;
    const MAX_BITRATE: i32 = 320000;
    if bitrate < MIN_BITRATE || bitrate > MAX_BITRATE {
        return Err(format!(
            "Bitrate must be between {} and {} bps",
            MIN_BITRATE, MAX_BITRATE
        ));
    }
    Ok(())
}

fn validate_quality(quality: i32) -> Result<(), String> {
    if quality < 0 {
        return Err("Quality cannot be negative".to_string());
    }
    const MIN_QUALITY: i32 = 32;
    const MAX_QUALITY: i32 = 320;
    if quality < MIN_QUALITY || quality > MAX_QUALITY {
        return Err(format!(
            "Quality must be between {} and {} kbps",
            MIN_QUALITY, MAX_QUALITY
        ));
    }
    Ok(())
}

fn validate_target_size_kb(size: i32) -> Result<(), String> {
    if size < 0 {
        return Err("Target size cannot be negative".to_string());
    }
    const MIN_SIZE: i32 = 10;
    const MAX_SIZE: i32 = 10000;
    if size < MIN_SIZE || size > MAX_SIZE {
        return Err(format!(
            "Target size must be between {} and {} KB",
            MIN_SIZE, MAX_SIZE
        ));
    }
    Ok(())
}

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
        .route("/_matrix/client/r0/voice/config", get(get_voice_config))
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
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let message_id = body
        .get("message_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("message_id is required".to_string()))?;

    if let Err(e) = validate_message_id(message_id) {
        return Err(ApiError::bad_request(e));
    }

    let target_format = body
        .get("target_format")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("target_format is required".to_string()))?;

    let valid_codecs = ["ogg", "mpeg", "wav", "mp3", "flac", "aac", "webm"];
    if let Err(e) = validate_audio_format(target_format, &valid_codecs) {
        return Err(ApiError::bad_request(e));
    }

    let quality = body.get("quality").and_then(|v| v.as_i64()).unwrap_or(128) as i32;

    if let Err(e) = validate_quality(quality) {
        return Err(ApiError::bad_request(e));
    }

    let bitrate = body
        .get("bitrate")
        .and_then(|v| v.as_i64())
        .unwrap_or(128000) as i32;

    if let Err(e) = validate_bitrate(bitrate) {
        return Err(ApiError::bad_request(e));
    }

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Conversion simulation successful. (Backend FFmpeg not connected)",
        "message_id": message_id,
        "target_format": target_format,
        "quality": quality,
        "bitrate": bitrate,
        "converted_content": null
    })))
}

#[axum::debug_handler]
async fn optimize_voice_message(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let message_id = body
        .get("message_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("message_id is required".to_string()))?;

    if let Err(e) = validate_message_id(message_id) {
        return Err(ApiError::bad_request(e));
    }

    let target_size_kb = body
        .get("target_size_kb")
        .and_then(|v| v.as_i64())
        .unwrap_or(500) as i32;

    if let Err(e) = validate_target_size_kb(target_size_kb) {
        return Err(ApiError::bad_request(e));
    }

    let preserve_quality = body
        .get("preserve_quality")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let remove_silence = body
        .get("remove_silence")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let normalize_volume = body
        .get("normalize_volume")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Optimization simulation successful. (Backend FFmpeg not connected)",
        "message_id": message_id,
        "target_size_kb": target_size_kb,
        "preserve_quality": preserve_quality,
        "remove_silence": remove_silence,
        "normalize_volume": normalize_volume,
        "optimized_content": null
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
