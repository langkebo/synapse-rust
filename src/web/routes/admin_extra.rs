use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};

pub fn create_admin_extra_router() -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/server_notifications",
            get(get_server_notifications),
        )
        .route(
            "/_synapse/admin/v1/server_notifications/stats",
            get(get_server_notifications_stats),
        )
        .route("/_synapse/admin/v1/media/quota", get(get_media_quota))
        .route("/_synapse/admin/v1/media/quota/stats", get(get_media_quota_stats))
        .route("/_synapse/admin/v1/cas/config", get(get_cas_config))
        .route("/_synapse/admin/v1/saml/config", get(get_saml_config))
        .route("/_synapse/admin/v1/oidc/config", get(get_oidc_config))
        .route(
            "/_synapse/admin/v1/federation/blacklist",
            get(get_federation_blacklist),
        )
        .route("/_synapse/admin/v1/federation/cache", get(get_federation_cache))
        .route("/_synapse/admin/v1/refresh_tokens", get(get_refresh_tokens_list))
        .route(
            "/_synapse/admin/v1/push_notifications",
            get(get_push_notifications_list),
        )
        .route("/_synapse/admin/v1/rate_limits", get(get_rate_limits_config))
}

#[axum::debug_handler]
pub async fn get_server_notifications(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "notifications": [],
        "total": 0
    })))
}

#[axum::debug_handler]
pub async fn get_server_notifications_stats(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "total_sent": 0,
        "total_read": 0,
        "total_dismissed": 0,
        "pending": 0
    })))
}

#[axum::debug_handler]
pub async fn get_media_quota(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "max_storage_bytes": 1099511627776_i64,
        "max_file_size_bytes": 1073741824_i64,
        "current_storage_bytes": 0_i64,
        "quota_enabled": true
    })))
}

#[axum::debug_handler]
pub async fn get_media_quota_stats(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "total_files": 0,
        "total_bytes": 0_i64,
        "avg_file_size": 0_i64,
        "quota_usage_percent": 0
    })))
}

#[axum::debug_handler]
pub async fn get_cas_config(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "enabled": false,
        "server_url": "",
        "service_url": ""
    })))
}

#[axum::debug_handler]
pub async fn get_saml_config(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "enabled": false,
        "sp_entity_id": "",
        "idp_entity_id": "",
        "idp_sso_url": ""
    })))
}

#[axum::debug_handler]
pub async fn get_oidc_config(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "enabled": false,
        "issuer": "",
        "client_id": "",
        "scopes": []
    })))
}

#[axum::debug_handler]
pub async fn get_federation_blacklist(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "blacklist": [],
        "total": 0
    })))
}

#[axum::debug_handler]
pub async fn get_federation_cache(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "cache_size": 0,
        "cache_hits": 0,
        "cache_misses": 0,
        "hit_rate": 0
    })))
}

#[axum::debug_handler]
pub async fn get_refresh_tokens_list(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "tokens": [],
        "total": 0
    })))
}

#[axum::debug_handler]
pub async fn get_push_notifications_list(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "notifications": [],
        "total": 0,
        "pending": 0
    })))
}

#[axum::debug_handler]
pub async fn get_rate_limits_config(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "limits": {
            "message": {
                "per_second": 10,
                "burst_count": 100
            },
            "login": {
                "per_second": 3,
                "burst_count": 10
            }
        },
        "blocked_ips": []
    })))
}
