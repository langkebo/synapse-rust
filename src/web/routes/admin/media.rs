use crate::common::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::AdminUser;
use axum::{
    extract::{Path, State},
    routing::{delete, get},
    Json, Router,
};
use serde_json::{json, Value};
use synapse_services::admin_media_service::decode_media_cursor;

pub fn create_media_router() -> Router<crate::web::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/media", get(get_all_media))
        .route("/_synapse/admin/v1/media/{media_id}", get(get_media_info))
        .route("/_synapse/admin/v1/media/{media_id}", delete(delete_media))
        .route("/_synapse/admin/v1/media/quota", get(get_media_quota))
        .route("/_synapse/admin/v1/users/{user_id}/media", get(get_user_media))
        .route("/_synapse/admin/v1/users/{user_id}/media", delete(delete_user_media))
        .route("/_synapse/admin/v1/quarantine_media/{media_id}/changes", get(get_media_quarantine_changes))
}

pub fn admin_media_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/media"),
        (Method::GET, "/_synapse/admin/v1/media/{media_id}"),
        (Method::DELETE, "/_synapse/admin/v1/media/{media_id}"),
        (Method::GET, "/_synapse/admin/v1/media/quota"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/media"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/media"),
        (Method::GET, "/_synapse/admin/v1/quarantine_media/{media_id}/changes"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::media"))
    .collect()
}

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

#[axum::debug_handler]
pub async fn get_media_info(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(media_id): Path<String>,
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

#[axum::debug_handler]
pub async fn delete_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.admin_media_service.delete_media(&media_id).await?;

    Ok(Json(json!({})))
}

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

#[axum::debug_handler]
pub async fn get_user_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<String>,
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

#[axum::debug_handler]
pub async fn delete_user_media(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let deleted = ctx.admin_media_service.delete_user_media(&user_id).await?;

    Ok(Json(json!({ "deleted": deleted })))
}

#[allow(clippy::unused_async)]
pub async fn get_media_quarantine_changes(
    _admin: AdminUser,
    State(_ctx): State<AdminContext>,
    Path(_media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    // No dedicated quarantine change history table exists yet; return an empty
    // list to match Synapse behaviour for media with no recorded changes.
    Ok(Json(json!({ "changes": [], "total": 0 })))
}
