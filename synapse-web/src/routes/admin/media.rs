use crate::routes::context::AdminContext;
use crate::routes::AdminUser;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use synapse_common::types::{MediaId, RoomId, ServerName, UserId};
use synapse_common::ApiError;
use synapse_services::admin_media_service::decode_media_cursor;

/// Request body for room-level media quarantine/unquarantine.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuarantineRoomMediaRequest {
    /// Optional user_id to filter which media to act on.
    #[serde(default)]
    pub user_id: Option<String>,
}

/// Request body for `POST /_synapse/admin/v1/media/delete`.
///
/// Either or both of `before_ts` and `max_size` may be provided.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeleteMediaByPolicyRequest {
    /// Delete local media whose `created_ts` is before this Unix
    /// timestamp in milliseconds. `0` or omitted means "no time filter".
    #[serde(default)]
    pub before_ts: Option<i64>,
    /// Delete local media whose `size` exceeds this many bytes.
    /// `0` or omitted means "no size filter".
    #[serde(default)]
    pub max_size: Option<i64>,
}

/// See [`create_media_router`].
pub fn create_media_router() -> Router<crate::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/media", get(get_all_media))
        .route("/_synapse/admin/v1/media/{media_id}", get(get_media_info))
        .route("/_synapse/admin/v1/media/{media_id}", delete(delete_media))
        .route("/_synapse/admin/v1/media/quota", get(get_media_quota))
        .route("/_synapse/admin/v1/users/{user_id}/media", get(get_user_media))
        .route("/_synapse/admin/v1/users/{user_id}/media", delete(delete_user_media))
        .route("/_synapse/admin/v1/rooms/{room_id}/media", get(get_room_media))
        .route("/_synapse/admin/v1/rooms/{room_id}/media/{media_id}", delete(delete_room_media))
        .route("/_synapse/admin/v1/quarantine_media/{media_id}/changes", get(get_media_quarantine_changes))
        .route("/_synapse/admin/v1/media/quarantine/{server_name}/{media_id}", post(quarantine_media))
        .route("/_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}", post(unquarantine_media))
        .route("/_synapse/admin/v1/rooms/{room_id}/media/quarantine", post(quarantine_room_media))
        .route("/_synapse/admin/v1/rooms/{room_id}/media/unquarantine", post(unquarantine_room_media))
        .route("/_synapse/admin/v1/media/protect/{server_name}/{media_id}", post(protect_media))
        // ─────────────────────────────────────────────────────────────────────
        // U-5: Missing endpoints being implemented
        // ─────────────────────────────────────────────────────────────────────
        .route("/_synapse/admin/v1/user/{user_id}/media/quarantine", post(quarantine_user_media))
        .route("/_synapse/admin/v1/media/delete", post(delete_media_by_policy))
        .route("/_synapse/admin/v1/purge_media_cache", post(purge_media_cache))
        .route("/_synapse/admin/v1/media/unprotect/{media_id}", post(unprotect_media_by_id))
}

/// See [`get_all_media`].
#[axum::debug_handler]
pub async fn get_all_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100_i64).clamp(1, 500);
    let cursor = decode_media_cursor(params.get("from").map(String::as_str));

    let page = ctx.admin_media_service.get_all_media(limit, cursor).await?;

    let media_list: Vec<Value> = page
        .media
        .iter()
        .map(|row| {
            json!({
                "media_id": row.media_id,
                "media_type": row.content_type,
                "upload_name": row.file_name,
                "created_ts": row.created_ts,
                "last_access_ts": row.last_accessed_at,
                "media_length": row.size,
                "user_id": row.uploader_user_id,
                "quarantined": row.quarantined
            })
        })
        .collect();

    Ok(Json(json!({
        "media": media_list,
        "total": media_list.len(),
        "next_batch": page.next_batch
    })))
}

/// See [`get_media_info`].
#[axum::debug_handler]
pub async fn get_media_info(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(media_id): Path<MediaId>,
) -> Result<Json<Value>, ApiError> {
    let media = ctx.admin_media_service.get_media_info(&media_id).await?;

    match media {
        Some(row) => Ok(Json(json!({
            "media_id": row.media_id,
            "media_type": row.content_type,
            "upload_name": row.file_name,
            "created_ts": row.created_ts,
            "last_access_ts": row.last_accessed_at,
            "media_length": row.size,
            "user_id": row.uploader_user_id,
            "quarantined": row.quarantined
        }))),
        None => Err(ApiError::not_found("Media not found".to_string())),
    }
}

/// See [`delete_media`].
#[axum::debug_handler]
pub async fn delete_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(media_id): Path<MediaId>,
) -> Result<Json<Value>, ApiError> {
    ctx.admin_media_service.delete_media(&media_id).await?;

    Ok(Json(json!({})))
}

/// See [`get_media_quota`].
#[axum::debug_handler]
pub async fn get_media_quota(_admin: AdminUser, State(ctx): State<AdminContext>) -> Result<Json<Value>, ApiError> {
    let quota = ctx.admin_media_service.get_media_quota().await?;

    Ok(Json(json!({
        "total_size": quota.total_size,
        "total_count": quota.total_count,
        "default_size_limit": 10000000000i64,
        "default_count_limit": 100
    })))
}

/// See [`get_user_media`].
#[axum::debug_handler]
pub async fn get_user_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let (_canonical_user_id, media) = ctx.admin_media_service.get_user_media(&user_id).await?;

    let media_list: Vec<Value> = media
        .iter()
        .map(|row| {
            json!({
                "media_id": row.media_id,
                "media_type": row.content_type,
                "upload_name": row.file_name,
                "created_ts": row.created_ts,
                "media_length": row.size
            })
        })
        .collect();

    Ok(Json(json!({ "media": media_list, "total": media_list.len() })))
}

/// See [`delete_user_media`].
#[axum::debug_handler]
pub async fn delete_user_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let deleted = ctx.admin_media_service.delete_user_media(&user_id).await?;

    Ok(Json(json!({ "deleted": deleted })))
}

/// See [`get_media_quarantine_changes`].
#[axum::debug_handler]
pub async fn get_media_quarantine_changes(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(media_id): Path<MediaId>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let since = params.get("since").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0).max(0);
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100_i64).clamp(1, 500);

    let changes = ctx.admin_media_service.get_media_quarantine_changes(&media_id, since, limit).await?;

    let changes_json: Vec<Value> = changes
        .iter()
        .map(|c| {
            json!({
                "stream_id": c.stream_id,
                "media_id": c.media_id,
                "server_name": c.server_name,
                "change_type": c.change_type,
                "changed_by": c.changed_by,
                "created_ts": c.created_ts
            })
        })
        .collect();

    Ok(Json(json!({ "changes": changes_json, "total": changes_json.len() })))
}

/// See [`quarantine_media`].
///
/// Backs `POST /_synapse/admin/v1/media/quarantine/{server_name}/{media_id}`.
#[axum::debug_handler]
pub async fn quarantine_media(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
) -> Result<Json<Value>, ApiError> {
    let stream_id = ctx.admin_media_service.quarantine_media(&server_name, &media_id, &admin.user_id).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "media_id": media_id,
        "server_name": server_name,
        "quarantined": true,
        "changed_by": admin.user_id
    })))
}

/// See [`unquarantine_media`].
///
/// Backs `POST /_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}`.
#[axum::debug_handler]
pub async fn unquarantine_media(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
) -> Result<Json<Value>, ApiError> {
    let stream_id = ctx.admin_media_service.unquarantine_media(&server_name, &media_id, &admin.user_id).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "media_id": media_id,
        "server_name": server_name,
        "quarantined": false,
        "changed_by": admin.user_id
    })))
}

/// See [`get_room_media`].
///
/// Backs `GET /_synapse/admin/v1/rooms/{room_id}/media`.
/// Lists all media in a room (local and remote).
#[axum::debug_handler]
pub async fn get_room_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100_i64).clamp(1, 500);
    let cursor = decode_media_cursor(params.get("from").map(String::as_str));

    let page = ctx.admin_media_service.get_room_media(&room_id, limit, cursor).await?;

    let media_list: Vec<Value> = page
        .media
        .iter()
        .map(|row| {
            json!({
                "media_id": row.media_id,
                "media_type": row.content_type,
                "upload_name": row.file_name,
                "created_ts": row.created_ts,
                "media_length": row.size
            })
        })
        .collect();

    Ok(Json(json!({
        "media": media_list,
        "total": media_list.len(),
        "next_batch": page.next_batch
    })))
}

/// See [`delete_room_media`].
///
/// Backs `DELETE /_synapse/admin/v1/rooms/{room_id}/media/{media_id}`.
/// Deletes a specific media item from a room.
#[axum::debug_handler]
pub async fn delete_room_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((room_id, media_id)): Path<(RoomId, MediaId)>,
) -> Result<Json<Value>, ApiError> {
    ctx.admin_media_service.delete_room_media(&room_id, &media_id).await?;

    Ok(Json(json!({})))
}

/// Quarantine media in a room, optionally filtered by user.
///
/// Backs `POST /_synapse/admin/v1/rooms/{roomId}/media/quarantine`.
/// The request body may contain a `user_id` to limit quarantine to that user's media.
#[axum::debug_handler]
pub async fn quarantine_room_media(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    Json(body): Json<QuarantineRoomMediaRequest>,
) -> Result<Json<Value>, ApiError> {
    let stream_id =
        ctx.admin_media_service.quarantine_room_media(&room_id, body.user_id.as_deref(), &admin.user_id).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "room_id": room_id,
        "quarantined": true,
        "changed_by": admin.user_id
    })))
}

/// Unquarantine media in a room, optionally filtered by user.
///
/// Backs `POST /_synapse/admin/v1/rooms/{roomId}/media/unquarantine`.
/// The request body may contain a `user_id` to limit unquarantine to that user's media.
#[axum::debug_handler]
pub async fn unquarantine_room_media(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    Json(body): Json<QuarantineRoomMediaRequest>,
) -> Result<Json<Value>, ApiError> {
    let stream_id =
        ctx.admin_media_service.unquarantine_room_media(&room_id, body.user_id.as_deref(), &admin.user_id).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "room_id": room_id,
        "quarantined": false,
        "changed_by": admin.user_id
    })))
}

/// Protect media from automatic quarantine.
///
/// Backs `POST /_synapse/admin/v1/media/protect/{serverName}/{mediaId}`.
#[axum::debug_handler]
pub async fn protect_media(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
) -> Result<Json<Value>, ApiError> {
    let stream_id = ctx.admin_media_service.protect_media(&server_name, &media_id, &admin.user_id).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "server_name": server_name,
        "media_id": media_id,
        "protected": true,
        "changed_by": admin.user_id
    })))
}

// ─────────────────────────────────────────────────────────────────────────────
// U-5: Missing endpoint handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Quarantine all media uploaded by a given user.
///
/// Backs `POST /_synapse/admin/v1/user/{user_id}/media/quarantine`.
#[axum::debug_handler]
pub async fn quarantine_user_media(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let stream_id = ctx.admin_media_service.quarantine_user_media(&user_id, &admin.user_id).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "user_id": user_id,
        "quarantined": true,
        "changed_by": admin.user_id
    })))
}

/// Batch-delete local media by policy.
///
/// Backs `POST /_synapse/admin/v1/media/delete`.
/// Parameters are optional; `0` or omitted means "no limit".
#[axum::debug_handler]
pub async fn delete_media_by_policy(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<DeleteMediaByPolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    let before_ts = body.before_ts.unwrap_or(0);
    let max_size = body.max_size.unwrap_or(0);

    let deleted = ctx.admin_media_service.delete_media_by_policy(before_ts, max_size).await?;

    Ok(Json(json!({
        "deleted": deleted
    })))
}

/// Unprotect a media item so it can be quarantined or deleted again.
///
/// Backs `POST /_synapse/admin/v1/media/unprotect/{media_id}`.
#[axum::debug_handler]
pub async fn unprotect_media_by_id(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(media_id): Path<MediaId>,
) -> Result<Json<Value>, ApiError> {
    let stream_id = ctx.admin_media_service.unprotect_media(&media_id, &admin.user_id).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "media_id": media_id,
        "unprotected": true,
        "changed_by": admin.user_id
    })))
}

/// Purge remote media from the local cache.
///
/// Backs `POST /_synapse/admin/v1/purge_media_cache?before_ts=<unix_ms>`.
/// In this implementation only local media exists, so this degrades to deleting
/// local media that matches the access-time policy.
///
/// Matches upstream Synapse semantics: `before_ts` is an optional query
/// parameter (defaults to 0, meaning "no time filter"); the endpoint returns
/// the number of purged media items.
#[axum::debug_handler]
pub async fn purge_media_cache(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let before_ts = params.get("before_ts").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0).max(0);

    let purged = ctx.admin_media_service.purge_media_cache(before_ts).await?;

    Ok(Json(json!({ "deleted": purged })))
}
