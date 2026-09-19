#![allow(clippy::unused_async)]
use super::{ensure_room_member_ctx, validate_user_id, AppState, AuthenticatedUser};
use crate::routes::context::RoomContext;
use crate::routes::extractors::RoomId;
use crate::routes::extractors::UserId;
use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use synapse_common::{ApiError, ApiResult};
use synapse_services::voice_service::VoiceMessageUploadParams;

/// Request body for `POST /voice/register` - registering an encrypted voice attachment.
#[derive(Debug, Deserialize)]
pub struct RegisterEncryptedVoiceRequest {
    /// The media_id of the uploaded voice (from mxc:// URL)
    pub media_id: String,
    /// The room_id the voice belongs to
    pub room_id: Option<String>,
    /// The content type (should be "audio/*" for unencrypted, omitted/encrypted for encrypted)
    pub content_type: String,
    /// Duration in milliseconds
    pub duration_ms: i32,
    /// File size in bytes
    pub size_bytes: i64,
}

/// Clamp the `limit` query parameter for voice listing endpoints.
///
/// Lower bound is 1, upper bound is 100, default (when `None`) is 50.
pub fn clamp_voice_list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(50).clamp(1, 100)
}

/// Map a `upload_voice_message` service result into the handler response.
///
/// FT-125: the service already returns an `ApiError`; this preserves the
/// original errcode/error instead of flattening every failure to a 500.
pub fn voice_upload_response(result: ApiResult<Value>) -> Result<Json<Value>, ApiError> {
    match result {
        Ok(value) => Ok(Json(value)),
        Err(e) => Err(e),
    }
}

/// The `VoiceListQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceListQuery {
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `from` field.
    pub from: Option<i64>,
}

/// See [`create_voice_router`].
pub fn create_voice_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/voice/config", get(get_voice_config))
        .route("/_matrix/client/v1/voice/upload", post(upload_voice_message))
        .route("/_matrix/client/v1/voice/register", post(register_encrypted_voice))
        .route("/_matrix/client/v1/voice/stats", get(get_voice_stats))
        .route("/_matrix/client/v1/voice/room/{room_id}/stats", get(get_room_voice_stats))
        .route("/_matrix/client/v1/voice/user/{user_id}/stats", get(get_user_voice_stats))
        .route("/_matrix/client/v3/voice/upload", post(upload_voice_message))
        .route("/_matrix/client/v3/voice/register", post(register_encrypted_voice))
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
        // ISSUE-13: vendor 前缀（私有端点，client 前缀保留为向后兼容别名）
        .route("/_matrix/vendor/v1/voice/upload", post(upload_voice_message))
        .route("/_matrix/vendor/v1/voice/config", get(get_voice_config))
        .route("/_matrix/vendor/v1/voice/stats", get(get_voice_stats))
        .route("/_matrix/vendor/v1/voice/room/{room_id}/stats", get(get_room_voice_stats))
        .route("/_matrix/vendor/v1/voice/user/{user_id}/stats", get(get_user_voice_stats))
        .route("/_matrix/vendor/v1/voice/room/{room_id}", get(get_room_voice_messages))
        .route("/_matrix/vendor/v1/voice/user/{user_id}", get(get_user_voice_messages))
        .route("/_matrix/vendor/v1/voice/{media_id}", get(get_voice_message_content))
        .route("/_matrix/vendor/v1/voice/{media_id}/convert", post(convert_voice_message))
        .route("/_matrix/vendor/v1/voice/{media_id}/optimize", post(optimize_voice_message))
        .route("/_matrix/vendor/v1/voice/{media_id}/transcription", post(transcribe_voice_message))
        .route("/_matrix/vendor/v1/voice/register", post(register_encrypted_voice))
}

#[axum::debug_handler]
async fn get_voice_config(
    State(_ctx): State<RoomContext>,
    _auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "enabled": true,
        "max_duration": 600,
        "allowed_formats": ["audio/ogg", "audio/mpeg", "audio/wav", "audio/webm", "audio/mp4", "audio/aac", "audio/flac"],
        "supported_formats": ["audio/ogg", "audio/mpeg", "audio/wav", "audio/webm", "audio/mp4", "audio/aac", "audio/flac"],
        "max_size_bytes": 52428800,
        "max_duration_ms": 60_0000,
        "content_type": "m.audio",
        "voice_extension": "org.matrix.msc3245.voice",
        "auto_transcribe": false
    })))
}

#[axum::debug_handler]
async fn upload_voice_message(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let voice_service = &ctx.voice_service;

    // Parse multipart form data
    let mut content: Vec<u8> = Vec::new();
    let mut content_type: Option<String> = None;
    let mut room_id: Option<String> = None;
    let mut duration_ms: Option<i32> = None;
    let mut waveform: Option<Vec<u16>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to parse multipart data: {}", e)))?
    {
        // `Field` is consumed by `bytes()` below, so every header read must
        // happen first — `name()` itself comes from `Content-Disposition`.
        // (The `filename=` param is intentionally not consumed: the handler
        // derives the stored name from `content_type`, and a hand-rolled
        // `filename=` scan here was dead code that contradicted its own comment.)
        let name = field.name().unwrap_or("").to_string();

        let data =
            field.bytes().await.map_err(|e| ApiError::bad_request(format!("Failed to read multipart field: {}", e)))?;

        match name.as_str() {
            "file" => {
                content = data.to_vec();
            }
            "content_type" => content_type = Some(String::from_utf8_lossy(&data).to_string()),
            "room_id" => room_id = Some(String::from_utf8_lossy(&data).to_string()),
            "duration_ms" => {
                duration_ms = Some(
                    String::from_utf8_lossy(&data)
                        .parse::<i32>()
                        .map_err(|_| ApiError::bad_request("Invalid duration_ms".to_string()))?,
                );
            }
            "waveform" => {
                let waveform_str = String::from_utf8_lossy(&data);
                let arr: Vec<u16> = waveform_str.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if !arr.is_empty() {
                    waveform = Some(arr);
                }
            }
            _ => {}
        }
    }

    // Validate required fields
    if content.is_empty() {
        return Err(ApiError::bad_request("File content is required".to_string()));
    }
    if duration_ms.is_none() || duration_ms.unwrap() <= 0 {
        return Err(ApiError::bad_request("Duration must be positive".to_string()));
    }

    const MAX_SIZE: usize = 50 * 1024 * 1024;
    if content.len() > MAX_SIZE {
        return Err(ApiError::bad_request(format!("Voice message too large. Max size is {} bytes", MAX_SIZE)));
    }

    let content_type = content_type.unwrap_or_else(|| {
        infer::get(&content).map(|k| k.mime_type().to_string()).unwrap_or_else(|| "audio/ogg".to_string())
    });

    // Validate audio content type
    synapse_services::voice_service::VoiceService::validate_audio_content_type(&content_type)?;

    // Check room membership if room_id is provided
    if let Some(ref rid) = room_id {
        ensure_room_member_ctx(&ctx, &auth_user, rid, "You must be a member of this room to upload voice messages")
            .await?;
    }

    let result = voice_service
        .upload_voice_message(VoiceMessageUploadParams {
            user_id: auth_user.user_id,
            room_id,
            content,
            content_type,
            duration_ms: duration_ms.unwrap(),
            waveform,
        })
        .await;
    voice_upload_response(result)
}

#[axum::debug_handler]
async fn get_voice_stats(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let stats = ctx.voice_service.get_voice_stats(&auth_user.user_id).await?;
    Ok(Json(stats))
}

#[axum::debug_handler]
async fn get_room_voice_stats(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a member of this room to view voice stats").await?;
    let stats = ctx.voice_service.get_room_voice_stats(&room_id).await?;
    Ok(Json(stats))
}

#[axum::debug_handler]
async fn get_user_voice_stats(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    if auth_user.user_id.as_str() != user_id.as_str() {
        return Err(ApiError::forbidden("Cannot view another user's voice stats"));
    }
    let stats = ctx.voice_service.get_user_voice_stats(&user_id).await?;
    Ok(Json(stats))
}

#[axum::debug_handler]
async fn get_room_voice_messages(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<VoiceListQuery>,
) -> Result<Json<Value>, ApiError> {
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a member of this room to view voice messages")
        .await?;
    let limit = clamp_voice_list_limit(query.limit);
    let result = ctx.voice_service.get_room_voice_messages(&room_id, limit, query.from).await?;
    Ok(Json(result))
}

#[axum::debug_handler]
async fn get_user_voice_messages(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<UserId>,
    Query(query): Query<VoiceListQuery>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id.as_str() != user_id.as_str() {
        return Err(ApiError::forbidden("Cannot view another user's voice messages"));
    }
    let limit = clamp_voice_list_limit(query.limit);
    let result = ctx.voice_service.get_user_voice_messages(&user_id, limit, query.from).await?;
    Ok(Json(result))
}

#[axum::debug_handler]
async fn get_voice_message_content(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(media_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    // FT-105: 先取出内容（含归属信息），在返回给调用者之前完成所有权校验，防止 IDOR
    let result = ctx.voice_service.get_voice_message_content(&media_id).await?;

    let owner_id = result.get("user_id").and_then(|v| v.as_str()).unwrap_or_default();
    let room_id = result.get("room_id").and_then(|v| v.as_str());

    // 非管理员且非上传者：需要校验房间成员身份；管理员/上传者直接视为通过
    let needs_room_check = !auth_user.is_admin && auth_user.user_id.as_str() != owner_id;
    let is_room_member = if needs_room_check {
        match room_id {
            // 非上传者但消息归属某房间：校验调用者是否为该房间成员
            Some(rid) => ensure_room_member_ctx(
                &ctx,
                &auth_user,
                rid,
                "You must be a member of this room to access this voice message",
            )
            .await
            .is_ok(),
            // 非上传者且消息不归属任何房间：无权访问
            None => false,
        }
    } else {
        // 管理员或上传者：不需要房间成员校验
        true
    };

    if !synapse_services::voice_service::VoiceService::can_access_voice_message(
        &auth_user.user_id,
        owner_id,
        auth_user.is_admin,
        room_id,
        is_room_member,
    ) {
        return Err(ApiError::forbidden("You do not have permission to access this voice message"));
    }

    Ok(Json(result))
}

#[axum::debug_handler]
async fn convert_voice_message(
    _state: State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "Voice conversion is handled client-side per MSC3245. Server-side processing is not supported",
    ))
}

#[axum::debug_handler]
async fn optimize_voice_message(
    _state: State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "Voice optimization is handled client-side per MSC3245. Server-side processing is not supported",
    ))
}

#[axum::debug_handler]
async fn transcribe_voice_message(
    _state: State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "Voice transcription is handled client-side per MSC3245. Use Web Speech API or local Whisper model on the client",
    ))
}

#[axum::debug_handler]
async fn register_encrypted_voice(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Json(req): Json<RegisterEncryptedVoiceRequest>,
) -> Result<Json<Value>, ApiError> {
    // Validate room_id from request body
    let room_id = req.room_id.as_ref().ok_or_else(|| ApiError::bad_request("room_id is required".to_string()))?;

    // Validate room membership
    ensure_room_member_ctx(&ctx, &auth_user, room_id, "You must be a member of this room to register voice messages")
        .await?;

    // Validate media_id
    if req.media_id.is_empty() {
        return Err(ApiError::bad_request("media_id is required".to_string()));
    }

    // Check if this voice was already registered (idempotent)
    if let Ok(Some(existing)) = ctx.voice_service.get_voice_message_content(&req.media_id).await {
        let existing_user = existing.get("user_id").and_then(|v: &serde_json::Value| v.as_str());
        if existing_user.map(|u| u == auth_user.user_id).unwrap_or(false) {
            // Already registered by this user, return success
            let content_uri = synapse_common::media_locator::MediaLocator {
                server_name: ctx.server_name.clone(),
                media_id: req.media_id.clone(),
            }
            .to_mxc_url();
            return Ok(Json(json!({
                "content_uri": content_uri,
                "exists": true
            })));
        }
    }

    // Register the encrypted voice
    ctx.voice_service
        .register_encrypted_voice(
            &auth_user.user_id,
            Some(room_id),
            &req.media_id,
            &req.content_type,
            req.duration_ms,
            req.size_bytes,
        )
        .await?;

    let content_uri = synapse_common::media_locator::MediaLocator {
        server_name: ctx.server_name.clone(),
        media_id: req.media_id,
    }
    .to_mxc_url();

    Ok(Json(json!({
        "content_uri": content_uri,
        "exists": false
    })))
}
