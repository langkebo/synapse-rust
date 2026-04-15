use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_server_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/info", get(get_admin_info))
        .route("/_synapse/admin/v1/server_version", get(get_server_version))
        .route(
            "/_synapse/admin/v1/purge_media_cache",
            post(purge_media_cache),
        )
        .route("/_synapse/admin/v1/restart", post(restart_server))
        .route("/_synapse/admin/v1/statistics", get(get_statistics))
        .route("/_synapse/admin/v1/status", get(get_status))
        .route("/_synapse/admin/v1/whois/{user_id}", get(whois))
        .route(
            "/_synapse/admin/v1/whois/{user_id}/{device_id}",
            get(whois_device),
        )
        .route("/_synapse/admin/v1/health", get(get_health))
        .route("/_synapse/admin/v1/config", get(get_config))
        .route(
            "/_synapse/admin/v1/experimental_features",
            get(get_experimental_features),
        )
        .route("/_synapse/admin/v1/backups", get(get_backups))
}

#[axum::debug_handler]
pub async fn get_admin_info(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_name": state.services.config.server.name,
        "server_version": env!("CARGO_PKG_VERSION"),
        "implementation": "synapse-rust"
    })))
}

#[axum::debug_handler]
pub async fn get_backups(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Err(unsupported_admin_server_feature("backups"))
}

fn unsupported_admin_server_feature(feature: &str) -> ApiError {
    ApiError::unrecognized(format!(
        "Admin server endpoint '{}' is not implemented in this deployment",
        feature
    ))
}

#[axum::debug_handler]
pub async fn get_server_version(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_version": env!("CARGO_PKG_VERSION"),
        "python_version": "Rust",
        "server_name": state.services.config.server.name
    })))
}

#[axum::debug_handler]
pub async fn purge_media_cache(
    _admin: AdminUser,
    State(_state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _before_ts = body
        .get("before_ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() - (30 * 24 * 60 * 60 * 1000));

    Err(unsupported_admin_server_feature("purge_media_cache"))
}

#[axum::debug_handler]
pub async fn restart_server(
    _admin: AdminUser,
    State(_state): State<AppState>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Err(unsupported_admin_server_feature("restart"))
}

#[axum::debug_handler]
pub async fn get_statistics(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let total_users = state
        .services
        .user_storage
        .get_user_count()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    let total_rooms = state
        .services
        .room_storage
        .get_room_count()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Update Prometheus metrics directly using the collector
    if let Some(gauge) = state.services.metrics.get_gauge("synapse_total_users") {
        gauge.set(total_users as f64);
    }
    if let Some(gauge) = state.services.metrics.get_gauge("synapse_total_rooms") {
        gauge.set(total_rooms as f64);
    }

    Ok(Json(json!({
        "total_users": total_users,
        "total_rooms": total_rooms,
        "daily_active_users": total_users,
        "monthly_active_users": total_users,
        "r30_users": total_users,
        "r30v2_users": total_users
    })))
}

#[axum::debug_handler]
pub async fn get_status(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .is_ok();

    Ok(Json(json!({
        "db_ok": db_ok,
        "server_ok": db_ok,
        "up": db_ok
    })))
}

#[axum::debug_handler]
pub async fn whois(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let devices = sqlx::query(
        "SELECT device_id, display_name, last_seen_ts, last_seen_ip FROM devices WHERE user_id = $1"
    )
    .bind(&user_id)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let connections: Vec<Value> = devices
        .iter()
        .map(|row| {
            json!({
                "device_id": row.get::<Option<String>, _>("device_id"),
                "display_name": row.get::<Option<String>, _>("display_name"),
                "last_seen": row.get::<Option<i64>, _>("last_seen_ts"),
                "ip": row.get::<Option<String>, _>("last_seen_ip")
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user_id,
        "devices": connections
    })))
}

#[axum::debug_handler]
pub async fn whois_device(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let device = sqlx::query(
        "SELECT device_id, display_name, last_seen_ts, last_seen_ip FROM devices WHERE user_id = $1 AND device_id = $2"
    )
    .bind(&user_id)
    .bind(&device_id)
    .fetch_optional(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match device {
        Some(row) => Ok(Json(json!({
            "user_id": user_id,
            "device_id": row.get::<Option<String>, _>("device_id"),
            "display_name": row.get::<Option<String>, _>("display_name"),
            "last_seen": row.get::<Option<i64>, _>("last_seen_ts"),
            "ip": row.get::<Option<String>, _>("last_seen_ip")
        }))),
        None => Err(ApiError::not_found("Device not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn get_health(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .is_ok();

    Ok(Json(json!({
        "status": if db_ok { "ok" } else { "error" },
        "database": if db_ok { "ok" } else { "error" }
    })))
}

#[axum::debug_handler]
pub async fn get_config(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_name": state.services.config.server.name,
        "public_baseurl": state.services.config.server.public_baseurl,
        "registration_enabled": state.services.config.server.enable_registration,
        "max_upload_size": state.services.config.server.max_upload_size
    })))
}

#[axum::debug_handler]
pub async fn get_experimental_features(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Err(unsupported_admin_server_feature("experimental_features"))
}
