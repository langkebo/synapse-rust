use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::Row;

fn decode_media_cursor(cursor: Option<&str>) -> Option<(i64, &str)> {
    let cursor = cursor?;
    let (created_ts, media_id) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if media_id.is_empty() {
        return None;
    }
    Some((created_ts, media_id))
}

fn encode_media_cursor(created_ts: i64, media_id: &str) -> String {
    format!("{created_ts}|{media_id}")
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_media_cursor, encode_media_cursor};

    #[test]
    fn test_media_cursor_round_trip() {
        let cursor = encode_media_cursor(1_700_000_000_000, "abc123");
        assert_eq!(
            decode_media_cursor(Some(&cursor)),
            Some((1_700_000_000_000, "abc123"))
        );
    }

    #[test]
    fn test_media_cursor_rejects_invalid_value() {
        assert_eq!(decode_media_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_media_cursor(Some("123|")), None);
    }
}

fn quarantine_status_to_bool(value: Option<&str>) -> bool {
    matches!(
        value,
        Some("quarantined") | Some("true") | Some("1") | Some("yes")
    )
}

pub fn create_media_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/media", get(get_all_media))
        .route("/_synapse/admin/v1/media/{media_id}", get(get_media_info))
        .route("/_synapse/admin/v1/media/{media_id}", delete(delete_media))
        .route("/_synapse/admin/v1/media/quota", get(get_media_quota))
        .route(
            "/_synapse/admin/v1/users/{user_id}/media",
            get(get_user_media),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/media",
            delete(delete_user_media),
        )
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
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_i64)
        .clamp(1, 500);
    let cursor = decode_media_cursor(params.get("from").map(String::as_str));

    let media = sqlx::query(
        "SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts, last_accessed_at, quarantine_status
         FROM media_metadata
         WHERE ($1::BIGINT IS NULL AND $2::TEXT IS NULL)
            OR created_ts < $1
            OR (created_ts = $1 AND media_id < $2)
         ORDER BY created_ts DESC, media_id DESC
         LIMIT $3"
    )
    .bind(cursor.map(|(created_ts, _)| created_ts))
    .bind(cursor.map(|(_, media_id)| media_id))
    .bind(limit)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let media_list: Vec<Value> = media
        .iter()
        .map(|row| {
            json!({
                "media_id": row.get::<Option<String>, _>("media_id"),
                "media_type": row.get::<Option<String>, _>("content_type"),
                "upload_name": row.get::<Option<String>, _>("file_name"),
                "created_ts": row.get::<Option<i64>, _>("created_ts"),
                "last_access_ts": row.get::<Option<i64>, _>("last_accessed_at"),
                "media_length": row.get::<Option<i64>, _>("size"),
                "user_id": row.get::<Option<String>, _>("uploader_user_id"),
                "quarantined": quarantine_status_to_bool(row.get::<Option<String>, _>("quarantine_status").as_deref())
            })
        })
        .collect();

    let next_batch = if media.len() as i64 == limit {
        media.last().map(|row| {
            encode_media_cursor(
                row.get::<Option<i64>, _>("created_ts").unwrap_or_default(),
                row.get::<Option<String>, _>("media_id")
                    .unwrap_or_default()
                    .as_str(),
            )
        })
    } else {
        None
    };

    Ok(Json(json!({
        "media": media_list,
        "total": media_list.len(),
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
pub async fn get_media_info(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let media = sqlx::query(
        "SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts, last_accessed_at, quarantine_status FROM media_metadata WHERE media_id = $1"
    )
    .bind(&media_id)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    match media {
        Some(row) => Ok(Json(json!({
            "media_id": row.get::<Option<String>, _>("media_id"),
            "media_type": row.get::<Option<String>, _>("content_type"),
            "upload_name": row.get::<Option<String>, _>("file_name"),
            "created_ts": row.get::<Option<i64>, _>("created_ts"),
            "last_access_ts": row.get::<Option<i64>, _>("last_accessed_at"),
            "media_length": row.get::<Option<i64>, _>("size"),
            "user_id": row.get::<Option<String>, _>("uploader_user_id"),
            "quarantined": quarantine_status_to_bool(row.get::<Option<String>, _>("quarantine_status").as_deref())
        }))),
        None => Err(ApiError::not_found("Media not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(media_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM media_metadata WHERE media_id = $1")
        .bind(&media_id)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Media not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_media_quota(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let total_size: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(size), 0)::BIGINT FROM media_metadata")
            .fetch_one(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_metadata")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    Ok(Json(json!({
        "total_size": total_size,
        "total_count": total_count,
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
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let media = sqlx::query(
        "SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts FROM media_metadata WHERE uploader_user_id = $1 ORDER BY created_ts DESC"
    )
    .bind(&user_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let media_list: Vec<Value> = media
        .iter()
        .map(|row| {
            json!({
                "media_id": row.get::<Option<String>, _>("media_id"),
                "media_type": row.get::<Option<String>, _>("content_type"),
                "upload_name": row.get::<Option<String>, _>("file_name"),
                "created_ts": row.get::<Option<i64>, _>("created_ts"),
                "media_length": row.get::<Option<i64>, _>("size")
            })
        })
        .collect();

    Ok(Json(
        json!({ "media": media_list, "total": media_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn delete_user_media(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let result = sqlx::query("DELETE FROM media_metadata WHERE uploader_user_id = $1")
        .bind(&user_id)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    Ok(Json(json!({ "deleted": result.rows_affected() })))
}
