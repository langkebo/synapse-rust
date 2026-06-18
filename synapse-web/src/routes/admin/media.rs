use crate::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_storage::{decode_media_cursor, AdminMediaInfo, AdminMediaStorage};

fn media_to_json(row: &AdminMediaInfo) -> Value {
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
}

fn user_media_to_json(row: &AdminMediaInfo) -> Value {
    json!({
        "media_id": row.media_id,
        "media_type": row.content_type,
        "upload_name": row.file_name,
        "created_ts": row.created_ts,
        "media_length": row.size
    })
}

pub fn create_media_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/media", get(get_all_media))
        .route("/_synapse/admin/v1/media/{media_id}", get(get_media_info))
        .route("/_synapse/admin/v1/media/{media_id}", delete(delete_media))
        .route("/_synapse/admin/v1/media/quota", get(get_media_quota))
        .route("/_synapse/admin/v1/media/quarantine", get(get_quarantine_changes))
        .route("/_synapse/admin/v1/media/quarantine/{server_name}/{media_id}", post(quarantine_media))
        .route("/_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}", post(unquarantine_media))
        .route("/_synapse/admin/v1/users/{user_id}/media", get(get_user_media))
        .route("/_synapse/admin/v1/users/{user_id}/media", delete(delete_user_media))
}

pub fn admin_media_route_manifest() -> Vec<crate::routes::route_ledger::RouteEntry> {
    use crate::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/media"),
        (Method::GET, "/_synapse/admin/v1/media/{media_id}"),
        (Method::DELETE, "/_synapse/admin/v1/media/{media_id}"),
        (Method::GET, "/_synapse/admin/v1/media/quota"),
        (Method::GET, "/_synapse/admin/v1/media/quarantine"),
        (Method::POST, "/_synapse/admin/v1/media/quarantine/{server_name}/{media_id}"),
        (Method::POST, "/_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/media"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/media"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::media"))
    .collect()
}

#[axum::debug_handler]
pub async fn get_all_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100_i64).clamp(1, 500);
    let cursor = decode_media_cursor(params.get("from").map(String::as_str));
    let storage = AdminMediaStorage::new(&state.services.account.user_storage.pool);
    let page = storage.get_all_media(limit, cursor).await?;
    let media_list: Vec<Value> = page.media.iter().map(media_to_json).collect();

    Ok(Json(json!({
        "media": media_list,
        "total": media_list.len(),
        "next_batch": page.next_batch
    })))
}

#[axum::debug_handler]
pub async fn get_media_info(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let storage = AdminMediaStorage::new(&state.services.account.user_storage.pool);
    let media = storage.get_media_info(&media_id).await?;

    match media {
        Some(row) => Ok(Json(media_to_json(&row))),
        None => Err(ApiError::not_found("Media not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let storage = AdminMediaStorage::new(&state.services.account.user_storage.pool);

    if !storage.delete_media(&media_id).await? {
        return Err(ApiError::not_found("Media not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_media_quota(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let storage = AdminMediaStorage::new(&state.services.account.user_storage.pool);
    let quota = storage.get_media_quota().await?;

    Ok(Json(json!({
        "total_size": quota.total_size,
        "total_count": quota.total_count,
        "default_size_limit": 10000000000i64,
        "default_count_limit": 100
    })))
}

#[axum::debug_handler]
pub async fn get_user_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .account
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let user = user.ok_or_else(|| ApiError::not_found("User not found".to_string()))?;
    let storage = AdminMediaStorage::new(&state.services.account.user_storage.pool);
    let media = storage.get_user_media(&user.user_id).await?;
    let media_list: Vec<Value> = media.iter().map(user_media_to_json).collect();

    Ok(Json(json!({ "media": media_list, "total": media_list.len() })))
}

#[axum::debug_handler]
pub async fn delete_user_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .account
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let user = user.ok_or_else(|| ApiError::not_found("User not found".to_string()))?;
    let storage = AdminMediaStorage::new(&state.services.account.user_storage.pool);
    let deleted = storage.delete_user_media(&user.user_id).await?;

    Ok(Json(json!({ "deleted": deleted })))
}

// ---------------------------------------------------------------------------
// Quarantine stream — admin API handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct QuarantineChangesParams {
    pub since: Option<i64>,
    pub limit: Option<i64>,
}

#[axum::debug_handler]
pub async fn quarantine_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let changed_by = _admin.user_id.clone();
    let stream_id =
        state.services.extensions.media_domain_service.quarantine_media(&server_name, &media_id, &changed_by).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "media_id": media_id,
        "server_name": server_name,
        "quarantined": true
    })))
}

#[axum::debug_handler]
pub async fn unquarantine_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let changed_by = _admin.user_id.clone();
    let stream_id =
        state.services.extensions.media_domain_service.unquarantine_media(&server_name, &media_id, &changed_by).await?;

    Ok(Json(json!({
        "stream_id": stream_id,
        "media_id": media_id,
        "server_name": server_name,
        "quarantined": false
    })))
}

#[axum::debug_handler]
pub async fn get_quarantine_changes(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(params): Query<QuarantineChangesParams>,
) -> Result<Json<Value>, ApiError> {
    let since = params.since.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);

    let changes = state.services.extensions.media_domain_service.get_quarantined_media_changes(since, limit).await?;

    let next_batch = changes.last().map(|c| c.stream_id);

    Ok(Json(json!({
        "changes": changes.iter().map(|c| json!({
            "stream_id": c.stream_id,
            "media_id": c.media_id,
            "server_name": c.server_name,
            "change_type": c.change_type,
            "changed_by": c.changed_by,
            "created_ts": c.created_ts,
        })).collect::<Vec<_>>(),
        "next_batch": next_batch,
    })))
}
