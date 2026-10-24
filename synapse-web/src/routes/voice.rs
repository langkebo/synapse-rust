#![allow(
    clippy::unused_async,
    reason = "handlers keep async signatures for uniform Axum routing even when the body performs synchronous validation only"
)]
use super::{ensure_room_member_ctx, validate_user_id, AppState, AuthenticatedUser};
use crate::routes::context::RoomContext;
use crate::routes::extractors::MediaId;
use crate::routes::extractors::RoomId;
use crate::routes::extractors::UserId;
use crate::routes::MatrixJson;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::{ApiError, ApiResult};
use synapse_services::voice_service::VoiceMessageUploadParams;

/// Request body for `POST /voice/register` - registering an encrypted voice attachment.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// 多部分封装自身的开销上界（boundary、每个字段头、收尾 `--`、CRLF）。
///
/// 请求体上限要比"音频字节上限"多出这部分，否则**恰好等于上限**的音频会被封装开销
/// 顶出去，客户端按 `/voice/config` 声明的 `max_size_bytes` 上传反而失败。
const VOICE_MULTIPART_OVERHEAD_BYTES: usize = 64 * 1024;

/// `/voice/upload` 的请求体上限（字节）。
///
/// 与 `/voice/config` 的 `max_size_bytes` **同源**：后者是权威的音频上限
/// （`config.server.max_upload_size`），这里再加封装开销。
///
/// 为什么必须显式给：axum 的 `DefaultBodyLimit` 默认是 2 MiB，而 `Multipart`
/// 提取器读的就是这个扩展 —— 不给的话，2 MiB 以上的音频在**传输层**被拒，而状态码是
/// **400 `M_BAD_JSON`**（限制经解析器报成"解析失败"），客户端会误判成请求格式错误；
/// 与此同时配置、handler 校验（`content.len() > max_size`）与 `/voice/config` 的宣告
/// 都按 `server.max_upload_size` 算。即"宣告的上限不可达"，且失败原因指向错误的方向。
///
/// 注意本上限与全局上限（`src/server/router.rs` 的 `RequestBodyLimitLayer`，同取
/// `max_upload_size`）的分工：真正拦住超限请求体的是**全局层**；本层的作用只是
/// "不要用 2 MiB 的默认值把宣告的上限遮蔽掉"，因此刻意比全局层大一个封装开销。
///
/// 顺带收口 CVE-2026-102592（ELEMENTSEC-2026-1740，`multipart/form-data` 大小写绕过
/// 导致内存耗尽）：本仓没有"先按键字符串比较再决定是否解析"的前置门，`Content-Type`
/// 只影响 mime 解析（大小写不敏感），因此不存在可绕过的检查；内存占用由请求体上限界定。
pub fn voice_multipart_body_limit(max_upload_size: usize) -> usize {
    max_upload_size.saturating_add(VOICE_MULTIPART_OVERHEAD_BYTES)
}

/// See [`create_voice_router`].
pub fn create_voice_router(state: &AppState) -> Router<AppState> {
    let body_limit = voice_multipart_body_limit(state.services.core.config.server.max_upload_size as usize);
    Router::new()
// ISSUE-13: 私有端点唯一规范位置是 /_matrix/vendor/v1（client 别名已按 AGENTS.md 铁律 1 删除）
        .route("/_matrix/vendor/v1/voice/upload", post(upload_voice_message))
        .route("/_matrix/vendor/v1/voice/config", get(get_voice_config))
        .route("/_matrix/vendor/v1/voice/stats", get(get_voice_stats))
        .route("/_matrix/vendor/v1/voice/room/{room_id}/stats", get(get_room_voice_stats))
        .route("/_matrix/vendor/v1/voice/user/{user_id}/stats", get(get_user_voice_stats))
        .route("/_matrix/vendor/v1/voice/room/{room_id}", get(get_room_voice_messages))
        .route("/_matrix/vendor/v1/voice/user/{user_id}", get(get_user_voice_messages))
        .route("/_matrix/vendor/v1/voice/{media_id}", get(get_voice_message_content))
        .route("/_matrix/vendor/v1/voice/{media_id}", delete(delete_voice_message))
        .route("/_matrix/vendor/v1/voice/{media_id}/convert", post(convert_voice_message))
        .route("/_matrix/vendor/v1/voice/{media_id}/optimize", post(optimize_voice_message))
        .route("/_matrix/vendor/v1/voice/{media_id}/transcription", post(transcribe_voice_message))
        .route("/_matrix/vendor/v1/voice/register", post(register_encrypted_voice))
        // 只作用于本模块的路由：不给则落到 axum 的 2 MiB 默认值（见
        // `voice_multipart_body_limit` 的说明）。GET 路由不受影响。
        .layer(DefaultBodyLimit::max(body_limit))
}

/// Capability declaration for the extended voice surface.
///
/// The three POST routes `/{media_id}/convert`, `/{media_id}/optimize` and
/// `/{media_id}/transcription` are registered so the route ledger stays
/// complete, but they intentionally return `404 M_UNRECOGNIZED` because
/// MSC3245 keeps all media processing client-side (ARCH-03 语义表第 1 行：
/// "端点/能力整体不提供" ⇒ 404，不是"端点存在但字段被拒"的 501)。The
/// `server_side_processing` block below advertises that explicitly, so SDKs can
/// gate on the capability declaration instead of discovering the hard 404 at call time.
#[axum::debug_handler]
async fn get_voice_config(
    State(ctx): State<RoomContext>,
    _auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "enabled": true,
        "max_duration": 600,
        "allowed_formats": ["audio/ogg", "audio/mpeg", "audio/wav", "audio/webm", "audio/mp4", "audio/aac", "audio/flac"],
        "supported_formats": ["audio/ogg", "audio/mpeg", "audio/wav", "audio/webm", "audio/mp4", "audio/aac", "audio/flac"],
        // A11: derive from the single authoritative upload limit
        // (`config.server.max_upload_size`, bytes) instead of hardcoding 50 MiB.
        "max_size_bytes": ctx.config.server.max_upload_size,
        // RD-19: 600_000 ms (10 min) — the grouping was previously written
        // `60_0000`, which reads as 60 seconds at a glance.
        "max_duration_ms": 600_000,
        "content_type": "m.audio",
        "voice_extension": "org.matrix.msc3245.voice",
        "auto_transcribe": false,
        "server_side_processing": {
            "convert": false,
            "optimize": false,
            "transcription": false
        }
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
    // Bind the validated value instead of `is_none()` followed by `unwrap()`:
    // this crate denies `clippy::unwrap_used`, and shadowing makes the
    // "positive duration" invariant explicit for the use below.
    let Some(duration_ms) = duration_ms.filter(|duration| *duration > 0) else {
        return Err(ApiError::bad_request("Duration must be positive".to_string()));
    };

    // A11: use the single authoritative upload limit
    // (`config.server.max_upload_size`, bytes) instead of a hardcoded 50 MiB constant.
    let max_size = ctx.config.server.max_upload_size as usize;
    if content.len() > max_size {
        return Err(ApiError::bad_request(format!("Voice message too large. Max size is {} bytes", max_size)));
    }

    let content_type = content_type
        .unwrap_or_else(|| infer::get(&content).map_or_else(|| "audio/ogg".to_string(), |k| k.mime_type().to_string()));

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
            duration_ms,
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
    Path(media_id): Path<MediaId>,
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

/// B-31: `DELETE /_matrix/vendor/v1/voice/{media_id}` — owner or admin only.
#[axum::debug_handler]
async fn delete_voice_message(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(media_id): Path<MediaId>,
) -> Result<Json<Value>, ApiError> {
    let result =
        ctx.voice_service.delete_voice_message(&media_id, auth_user.user_id.as_str(), auth_user.is_admin).await?;
    Ok(Json(result))
}

/// MSC3245 规定语音的编码/优化/转写**在客户端**完成，服务端三个端点只是"登记在册的不
/// 接受"。语义上属于「该端点整体不提供」而不是「端点存在但某个字段不支持」，所以固定
/// 404 `M_UNRECOGNIZED`（见 `docs/synapse-rust/ROUTE_CONTRACT.md` 的「未实现语义表」，
/// ARCH-03）。抽成函数是为了让语义被用例钉住：改回 501 会被 `arch03_*` 用例判红。
fn voice_operation_not_provided(operation: &str, client_hint: &str) -> ApiError {
    ApiError::not_implemented(format!("Voice {operation} is handled client-side per MSC3245. {client_hint}"))
}

#[axum::debug_handler]
async fn convert_voice_message(
    _state: State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<MediaId>,
) -> Result<Json<Value>, ApiError> {
    Err(voice_operation_not_provided("conversion", "Server-side processing is not supported"))
}

#[axum::debug_handler]
async fn optimize_voice_message(
    _state: State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<MediaId>,
) -> Result<Json<Value>, ApiError> {
    Err(voice_operation_not_provided("optimization", "Server-side processing is not supported"))
}

#[axum::debug_handler]
async fn transcribe_voice_message(
    _state: State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path(_media_id): Path<MediaId>,
) -> Result<Json<Value>, ApiError> {
    Err(voice_operation_not_provided("transcription", "Use Web Speech API or local Whisper model on the client"))
}

#[axum::debug_handler]
async fn register_encrypted_voice(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(req): MatrixJson<RegisterEncryptedVoiceRequest>,
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
    // Note: get_voice_message_content returns Result<Value, ApiError> where NotFound
    // means no existing record exists - we handle this via the Ok(existing) pattern
    if let Ok(existing) = ctx.voice_service.get_voice_message_content(&req.media_id).await {
        let existing_user = existing.get("user_id").and_then(|v| v.as_str());
        if existing_user.is_some_and(|u| u == auth_user.user_id) {
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

    let content_uri =
        synapse_common::media_locator::MediaLocator { server_name: ctx.server_name.clone(), media_id: req.media_id }
            .to_mxc_url();

    Ok(Json(json!({
        "content_uri": content_uri,
        "exists": false
    })))
}

#[cfg(test)]
mod arch03_voice_unimplemented_semantics_tests {
    use super::*;
    use axum::http::StatusCode;

    /// ARCH-03 语义表：**端点整体不提供** ⇒ 404 + `M_UNRECOGNIZED`。
    ///
    /// 这三个 handler 的整个行为就这么一句 Err，所以钉住 `voice_operation_not_provided`
    /// 等于钉住三个端点的响应。变异：把 `not_implemented` 换成 `not_supported` ⇒ 红。
    #[test]
    fn voice_convert_optimize_transcribe_are_404_m_unrecognized() {
        for (operation, hint) in [
            ("conversion", "Server-side processing is not supported"),
            ("optimization", "Server-side processing is not supported"),
            ("transcription", "Use Web Speech API or local Whisper model on the client"),
        ] {
            let err = voice_operation_not_provided(operation, hint);
            assert_eq!(
                err.code.as_str(),
                "M_UNRECOGNIZED",
                "{operation}: 端点整体不提供 ⇒ M_UNRECOGNIZED（wire 值），不能与「字段不支持」的 M_UNKNOWN 混用"
            );
            assert!(err.is_not_implemented(), "{operation}: 内部「未实现」标记必须保留");
            assert_eq!(err.http_status(), StatusCode::NOT_FOUND, "{operation}: 状态码必须是 404");
            assert!(
                err.message.contains("MSC3245") && err.message.contains("client-side"),
                "{operation}: message 必须指路（MSC3245 客户端处理），实际是 {:?}",
                err.message
            );
        }
    }

    /// 反向：语音端点的 404 不能掩盖真实内部错误（真实错误仍是 500 + `M_UNKNOWN`）。
    #[test]
    fn voice_internal_errors_stay_500_not_404() {
        let err = ApiError::internal("voice storage unavailable");
        assert_eq!(err.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code.as_str(), "M_UNKNOWN");
        assert_ne!(err.code.as_str(), "M_UNRECOGNIZED");
    }
}
