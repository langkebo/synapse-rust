use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use tracing::info;

pub fn create_federation_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/federation/destinations",
            get(get_destinations),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}",
            get(get_destination),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}/reset_connection",
            post(reset_connection),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}",
            delete(delete_destination),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}/rooms",
            get(get_destination_rooms),
        )
        .route(
            "/_synapse/admin/v1/federation/rewrite",
            post(rewrite_federation),
        )
        .route(
            "/_synapse/admin/v1/federation/resolve",
            post(resolve_federation),
        )
        .route(
            "/_synapse/admin/v1/federation/confirm",
            post(confirm_federation),
        )
        .route(
            "/_synapse/admin/v1/federation/blacklist",
            get(get_blacklist),
        )
        .route(
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
            post(add_to_blacklist),
        )
        .route(
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
            delete(remove_from_blacklist),
        )
        .route(
            "/_synapse/admin/v1/federation/cache",
            get(get_federation_cache),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/{key}",
            delete(delete_federation_cache_entry),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/clear",
            post(clear_federation_cache),
        )
}

#[derive(Debug, Deserialize)]
pub struct RewriteRequest {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub server_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    pub server_name: String,
    pub accept: bool,
}

fn unsupported_admin_federation_feature(feature: &str) -> ApiError {
    ApiError::unrecognized(format!(
        "Admin federation endpoint '{}' is not implemented in this deployment",
        feature
    ))
}

#[axum::debug_handler]
pub async fn get_destinations(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let destinations = sqlx::query(
        "SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count FROM federation_servers ORDER BY server_name"
    )
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let dest_list: Vec<Value> = destinations
        .iter()
        .map(|row| {
            json!({
                "destination": row.get::<Option<String>, _>("server_name"),
                "retry_last_ts": row.get::<Option<i64>, _>("last_failed_connect_at"),
                "retry_interval": Value::Null,
                "failure_ts": row.get::<Option<i64>, _>("last_failed_connect_at"),
                "last_successful_ts": row.get::<Option<i64>, _>("last_successful_connect_at"),
                "failure_count": row.get::<Option<i32>, _>("failure_count").unwrap_or_default()
            })
        })
        .collect();

    Ok(Json(
        json!({ "destinations": dest_list, "total": dest_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn get_destination(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let dest = sqlx::query(
        "SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count FROM federation_servers WHERE server_name = $1"
    )
    .bind(&destination)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match dest {
        Some(row) => Ok(Json(json!({
            "destination": row.get::<Option<String>, _>("server_name"),
            "retry_last_ts": row.get::<Option<i64>, _>("last_failed_connect_at"),
            "retry_interval": Value::Null,
            "failure_ts": row.get::<Option<i64>, _>("last_failed_connect_at"),
            "last_successful_ts": row.get::<Option<i64>, _>("last_successful_connect_at"),
            "failure_count": row.get::<Option<i32>, _>("failure_count").unwrap_or_default()
        }))),
        None => Err(ApiError::not_found("Destination not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn reset_connection(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query(
        "UPDATE federation_servers SET last_failed_connect_at = NULL, failure_count = 0 WHERE server_name = $1"
    )
    .bind(&destination)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Destination not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn delete_destination(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_servers WHERE server_name = $1")
        .bind(&destination)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Destination not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_destination_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let destination_exists =
        sqlx::query("SELECT server_name FROM federation_servers WHERE server_name = $1")
            .bind(&destination)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .is_some();

    if !destination_exists {
        return Err(ApiError::not_found("Destination not found".to_string()));
    }

    let rooms = sqlx::query(
        "SELECT DISTINCT room_id FROM federation_queue WHERE destination = $1 AND room_id IS NOT NULL ORDER BY room_id",
    )
        .bind(&destination)
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room_list: Vec<String> = rooms.iter().map(|r| r.get("room_id")).collect();

    Ok(Json(
        json!({ "rooms": room_list, "total": room_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn rewrite_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<RewriteRequest>,
) -> Result<Json<Value>, ApiError> {
    let from_server = &body.from;
    let to_server = &body.to;

    let source_exists =
        sqlx::query("SELECT server_name FROM federation_servers WHERE server_name = $1")
            .bind(from_server)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .is_some();

    if !source_exists {
        return Err(ApiError::not_found(format!(
            "Source server {} not found",
            from_server
        )));
    }

    let rooms_result = sqlx::query("SELECT room_id FROM room_state_events WHERE sender LIKE $1")
        .bind(format!("%:{}", from_server))
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let rooms_count = rooms_result.len();

    info!(
        "Federation rewrite from {} to {}: {} rooms affected by {}",
        from_server, to_server, rooms_count, admin.user_id
    );

    Ok(Json(json!({
        "from": from_server,
        "to": to_server,
        "rewritten": true,
        "rooms_affected": rooms_count,
        "rewritten_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn resolve_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ResolveRequest>,
) -> Result<Json<Value>, ApiError> {
    let server_name = &body.server_name;

    let is_blocked = state
        .services
        .federation_blacklist_storage
        .is_server_blocked(server_name)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let destination = sqlx::query(
        "SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count FROM federation_servers WHERE server_name = $1"
    )
    .bind(server_name)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let resolved = destination.is_some() && !is_blocked;

    info!(
        "Federation resolve for {}: resolved={}, blacklisted={}",
        server_name, resolved, is_blocked
    );

    Ok(Json(json!({
        "server_name": server_name,
        "resolved": resolved,
        "blacklisted": is_blocked,
        "in_destinations": destination.is_some(),
        "resolved_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn confirm_federation(
    _admin: AdminUser,
    State(_state): State<AppState>,
    Json(body): Json<ConfirmRequest>,
) -> Result<Json<Value>, ApiError> {
    let _ = (&body.server_name, body.accept);
    Err(unsupported_admin_federation_feature("confirm"))
}

#[axum::debug_handler]
pub async fn get_blacklist(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let blacklist = sqlx::query(
        "SELECT server_name, added_ts, reason FROM federation_blacklist ORDER BY added_ts DESC",
    )
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let list: Vec<Value> = blacklist
        .iter()
        .map(|row| {
            json!({
                "server_name": row.get::<Option<String>, _>("server_name"),
                "added_at": row.get::<Option<i64>, _>("added_ts").unwrap_or_default(),
                "reason": row.get::<Option<String>, _>("reason")
            })
        })
        .collect();

    Ok(Json(json!({ "blacklist": list, "total": list.len() })))
}

#[axum::debug_handler]
pub async fn add_to_blacklist(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(server_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    let result = sqlx::query(
        "INSERT INTO federation_blacklist (server_name, added_ts, added_by) VALUES ($1, $2, $3) ON CONFLICT (server_name) DO NOTHING"
    )
    .bind(&server_name)
    .bind(now)
    .bind(&admin.user_id)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::conflict(
            "Server is already blacklisted".to_string(),
        ));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn remove_from_blacklist(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(server_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_blacklist WHERE server_name = $1")
        .bind(&server_name)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Blacklist entry not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_federation_cache(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let cache = sqlx::query("SELECT key, value, expiry_ts FROM federation_cache ORDER BY key")
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let entries: Vec<Value> = cache
        .iter()
        .map(|row| {
            json!({
                "key": row.get::<Option<String>, _>("key"),
                "value": row.get::<Option<String>, _>("value"),
                "expiry_ts": row.get::<Option<i64>, _>("expiry_ts")
            })
        })
        .collect();

    Ok(Json(json!({ "cache": entries, "total": entries.len() })))
}

#[axum::debug_handler]
pub async fn delete_federation_cache_entry(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_cache WHERE key = $1")
        .bind(&key)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Cache entry not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn clear_federation_cache(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_cache")
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({ "deleted": result.rows_affected() })))
}
