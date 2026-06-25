use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

pub fn create_server_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/server", get(get_admin_info_compat))
        .route("/_synapse/admin/v1/server_version", get(get_server_version))
        .route("/_synapse/admin/v1/whoami", get(get_admin_whoami))
        .route("/_synapse/admin/v1/purge_media_cache", post(purge_media_cache))
        .route("/_synapse/admin/v1/restart", post(restart_server))
        .route("/_synapse/admin/v1/statistics", get(get_statistics))
        .route("/_synapse/admin/v1/status", get(get_status))
        .route("/_synapse/admin/v1/whois/{user_id}", get(whois))
        .route("/_synapse/admin/v1/whois/{user_id}/{device_id}", get(whois_device))
        .route("/_synapse/admin/v1/health", get(get_health))
        .route("/_synapse/admin/v1/config", get(get_config))
        .route("/_synapse/admin/v1/experimental_features", get(get_experimental_features))
        .route("/_synapse/admin/v1/backups", get(get_backups))
        .route("/_synapse/admin/v1/jitsi/config", get(get_jitsi_config))
        .route("/_synapse/admin/v1/invite/blocklist", get(get_invite_blocklist_admin))
        .route("/_synapse/admin/v1/invite/allowlist", get(get_invite_allowlist_admin))
}

pub fn admin_server_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/server_version"),
        (Method::POST, "/_synapse/admin/v1/purge_media_cache"),
        (Method::POST, "/_synapse/admin/v1/restart"),
        (Method::GET, "/_synapse/admin/v1/statistics"),
        (Method::GET, "/_synapse/admin/v1/status"),
        (Method::GET, "/_synapse/admin/v1/whois/{user_id}"),
        (Method::GET, "/_synapse/admin/v1/whois/{user_id}/{device_id}"),
        (Method::GET, "/_synapse/admin/v1/health"),
        (Method::GET, "/_synapse/admin/v1/config"),
        (Method::GET, "/_synapse/admin/v1/experimental_features"),
        (Method::GET, "/_synapse/admin/v1/backups"),
        (Method::GET, "/_synapse/admin/v1/jitsi/config"),
        (Method::GET, "/_synapse/admin/v1/invite/blocklist"),
        (Method::GET, "/_synapse/admin/v1/invite/allowlist"),
        // The `/_synapse/admin/info` endpoint is registered by the
        // top-level `create_admin_module_router` with `server::get_admin_info`
        // — declared here because it shares the module's namespace.
        (Method::GET, "/_synapse/admin/info"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::server"))
    .collect()
}

#[axum::debug_handler]
pub async fn get_admin_info_compat(admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    get_admin_info(admin, State(state)).await
}

#[allow(clippy::unused_async)]
pub async fn get_admin_info(admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    // Only super_admin can access server info
    if admin.role != "super_admin" {
        return Err(ApiError::forbidden("Only super_admin can access server information".to_string()));
    }

    Ok(Json(json!({
        "server_name": state.services.core.server_name,
        "server_version": env!("CARGO_PKG_VERSION"),
        "implementation": "synapse-rust"
    })))
}

#[allow(clippy::unused_async)]
pub async fn get_admin_whoami(admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "user_id": admin.user_id,
        "name": format!("@{}:{}", admin.user_id, state.services.core.server_name),
        "is_admin": admin.role == "super_admin" || admin.role == "admin",
        "role": admin.role
    })))
}

#[allow(clippy::unused_async)]
pub async fn get_backups(
    _admin: AdminUser,
    State(_state): State<AppState>,
    axum::extract::Query(_params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    // Backups are managed by external infrastructure (pg_dump, WAL-G, etc.),
    // not by the homeserver itself. Return 501 to distinguish "endpoint
    // recognized but intentionally not implemented" from "endpoint unknown".
    Err(ApiError::not_implemented("Admin server endpoint 'backups' is not implemented in this deployment; backups are managed by external infrastructure"))
}

#[allow(clippy::unused_async)]
pub async fn get_server_version(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_version": env!("CARGO_PKG_VERSION"),
        "python_version": "Rust",
        "server_name": state.services.core.server_name
    })))
}

#[axum::debug_handler]
pub async fn purge_media_cache(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let before_ts = body
        .get("before_ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() - (30 * 24 * 60 * 60 * 1000));

    let deleted = state
        .services
        .core
        .media_service
        .purge_media_cache(before_ts)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to purge media cache", &e))?;

    Ok(Json(json!({
        "deleted": deleted
    })))
}

#[allow(clippy::unused_async)] // axum handlers must be async even when the await is inside spawn
pub async fn restart_server(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // Optional graceful shutdown delay (ms). Defaults to 100ms to let the
    // HTTP response flush before the process exits.
    let delay_ms = body
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(100)
        .min(10_000);

    let shutdown_tx = state.shutdown_signal.ok_or_else(|| {
        ApiError::internal("Shutdown signal is not wired into AppState; restart is unavailable")
    })?;

    // Send the shutdown signal in a background task after a short delay so
    // this handler can return a success response first.
    tokio::spawn(async move {
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        ::tracing::info!("Admin triggered server restart via POST /_synapse/admin/v1/restart");
        let _ = shutdown_tx.send(());
    });

    Ok(Json(json!({
        "restart_pending": true,
        "message": "Graceful shutdown initiated; the process manager will restart the server"
    })))
}

#[axum::debug_handler]
pub async fn get_statistics(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let total_users = state.services.account.account_identity_service.get_user_count().await?;
    let total_rooms = state.services.rooms.room_service.get_room_count().await?;

    // Real active-user metrics based on device last_seen_ts.
    let daily_active_users = state
        .services
        .account
        .user_storage
        .get_daily_active_users()
        .await
        .unwrap_or(0);
    let monthly_active_users = state
        .services
        .account
        .user_storage
        .get_monthly_active_users()
        .await
        .unwrap_or(0);
    let r30_users = state.services.account.user_storage.get_r30_users().await.unwrap_or(0);

    // Room activity and message-volume metrics.
    let room_stats = state.services.rooms.room_storage.get_room_stats_overview().await.unwrap_or_else(|e| {
        ::tracing::warn!(error = %e, "Failed to fetch room stats overview for /statistics");
        json!({})
    });
    let total_messages = room_stats.get("total_messages").and_then(|v| v.as_i64()).unwrap_or(0);
    let active_rooms = room_stats.get("active_rooms").and_then(|v| v.as_i64()).unwrap_or(0);
    let total_members = room_stats.get("total_members").and_then(|v| v.as_i64()).unwrap_or(0);
    let encrypted_rooms = room_stats.get("encrypted_rooms").and_then(|v| v.as_i64()).unwrap_or(0);

    let daily_messages = state
        .services
        .rooms
        .event_storage
        .get_daily_message_count()
        .await
        .unwrap_or(0);

    // Update Prometheus metrics directly using the collector
    if let Some(gauge) = state.services.core.metrics.get_gauge("synapse_total_users") {
        gauge.set(total_users as f64);
    }
    if let Some(gauge) = state.services.core.metrics.get_gauge("synapse_total_rooms") {
        gauge.set(total_rooms as f64);
    }
    if let Some(gauge) = state.services.core.metrics.get_gauge("synapse_daily_active_users") {
        gauge.set(daily_active_users as f64);
    }
    if let Some(gauge) = state.services.core.metrics.get_gauge("synapse_monthly_active_users") {
        gauge.set(monthly_active_users as f64);
    }
    if let Some(gauge) = state.services.core.metrics.get_gauge("synapse_total_messages") {
        gauge.set(total_messages as f64);
    }
    if let Some(gauge) = state.services.core.metrics.get_gauge("synapse_active_rooms_7d") {
        gauge.set(active_rooms as f64);
    }

    Ok(Json(json!({
        "total_users": total_users,
        "total_rooms": total_rooms,
        "daily_active_users": daily_active_users,
        "monthly_active_users": monthly_active_users,
        "r30_users": r30_users,
        "r30v2_users": r30_users,
        "total_messages": total_messages,
        "daily_messages": daily_messages,
        "active_rooms_7d": active_rooms,
        "total_members": total_members,
        "encrypted_rooms": encrypted_rooms,
        "average_messages_per_room": if total_rooms > 0 { total_messages / total_rooms } else { 0 }
    })))
}

#[axum::debug_handler]
pub async fn get_status(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let db_ok = state.services.admin.admin_server_service.is_database_healthy().await;

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
    let user = state
        .services
        .account
        .account_identity_service
        .get_user_by_identifier(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

    let devices = state.services.account.account_device_list_service.get_user_devices(&user.user_id).await?;

    let connections: Vec<Value> = devices
        .iter()
        .map(|d| {
            json!({
                "device_id": d.device_id,
                "display_name": d.display_name,
                "last_seen": d.last_seen_ts,
                "ip": d.last_seen_ip
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user.user_id,
        "devices": connections
    })))
}

#[axum::debug_handler]
pub async fn whois_device(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .account
        .account_identity_service
        .get_user_by_identifier(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

    let device = state
        .services
        .account
        .account_device_list_service
        .get_device(&device_id)
        .await?
        .filter(|d| d.user_id == user.user_id);

    match device {
        Some(d) => Ok(Json(json!({
            "user_id": user.user_id,
            "device_id": d.device_id,
            "display_name": d.display_name,
            "last_seen": d.last_seen_ts,
            "ip": d.last_seen_ip
        }))),
        None => Err(ApiError::not_found("Device not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn get_health(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let db_ok = state.services.admin.admin_server_service.is_database_healthy().await;

    Ok(Json(json!({
        "status": if db_ok { "ok" } else { "error" },
        "database": if db_ok { "ok" } else { "error" }
    })))
}

#[allow(clippy::unused_async)]
pub async fn get_config(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_name": state.services.core.config.server.name,
        "public_baseurl": state.services.core.config.server.public_baseurl,
        "registration_enabled": state.services.core.config.server.enable_registration,
        "max_upload_size": state.services.core.config.server.max_upload_size
    })))
}

#[axum::debug_handler]
pub async fn get_experimental_features(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    // Bridge the DB-backed FeatureFlagService to Synapse's experimental_features
    // surface. List all flags and expose their effective enabled state.
    let filters = crate::storage::FeatureFlagFilters {
        target_scope: None,
        status: None,
        limit: 200,
        cursor_updated_ts: None,
        cursor_flag_key: None,
    };
    let (flags, total) = state
        .services
        .admin
        .feature_flag_service
        .list_flags(filters)
        .await?;

    let features: serde_json::Map<String, Value> = flags
        .iter()
        .map(|flag| {
            let enabled = matches!(
                flag.status.as_str(),
                "active" | "fully_enabled" | "ramping"
            ) && flag.rollout_percent > 0;
            (flag.flag_key.clone(), Value::Bool(enabled))
        })
        .collect();

    Ok(Json(json!({
        "features": features,
        "total": total,
    })))
}

#[allow(clippy::unused_async)]
pub async fn get_jitsi_config(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    // Jitsi domain is not hardcoded to a third-party service.  Deployments
    // should configure their own Jitsi instance; null domain signals "not
    // configured" to clients.
    Ok(Json(json!({
        "domain": null,
        "app_id": null,
        "jwt_enabled": false,
        "jwt_asap_enabled": false,
        "jwt_auth_type": "none",
        "server_name": state.services.core.server_name
    })))
}

#[axum::debug_handler]
pub async fn get_invite_blocklist_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let blocklist = state
        .services
        .account
        .invite_blocklist_storage
        .get_global_invite_blocklist()
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get global blocklist", &e))?;

    Ok(Json(json!({
        "blocklist": blocklist
    })))
}

#[axum::debug_handler]
pub async fn get_invite_allowlist_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let allowlist = state
        .services
        .account
        .invite_blocklist_storage
        .get_global_invite_allowlist()
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get global allowlist", &e))?;

    Ok(Json(json!({
        "allowlist": allowlist
    })))
}
