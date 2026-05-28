use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde_json::{json, Value};

pub async fn get_key_rotation_status(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Key rotation management requires server admin privileges".to_string(),
        ));
    }

    let rotation_manager = &state.services.key_rotation_manager;
    let status = rotation_manager.get_rotation_status().await;

    // Add user-specific last rotation info
    let last_rotation = state
        .services
        .key_rotation_storage
        .get_user_last_rotation_ts(&auth_user.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query key rotation log: {e}");
            ApiError::internal("Internal server error".to_string())
        })?;

    Ok(Json(json!({
        "enabled": status.get("rotation_enabled"),
        "status": status,
        "user_last_rotation": last_rotation,
    })))
}

/// POST variant of get_key_rotation_status — some clients use POST instead of GET
pub async fn get_key_rotation_status_post(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    get_key_rotation_status(State(state), auth_user).await
}

pub async fn rotate_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Key rotation management requires server admin privileges".to_string(),
        ));
    }

    let requested_key_id = body
        .get("key_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let rotation_manager = &state.services.key_rotation_manager;
    match rotation_manager.rotate_keys(requested_key_id).await {
        Ok(()) => {
            let current = rotation_manager.get_current_key().await;
            Ok(Json(json!({
                "success": true,
                "message": "Keys rotated successfully",
                "has_new_key": current.ok().flatten().is_some(),
            })))
        }
        Err(e) => {
            tracing::error!("Key rotation failed: {e}");
            Err(ApiError::internal("Internal server error".to_string()))
        }
    }
}

pub async fn get_rotation_history(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Key rotation management requires server admin privileges".to_string(),
        ));
    }

    let history_rows = state
        .services
        .key_rotation_storage
        .get_device_rotation_history(&auth_user.user_id, &device_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get rotation history: {e}");
            ApiError::internal("Internal server error".to_string())
        })?;

    let history: Vec<Value> = history_rows
        .into_iter()
        .map(|(key_id, rotated_ts)| {
            json!({
                "key_id": key_id,
                "rotated_ts": rotated_ts,
            })
        })
        .collect();

    Ok(Json(json!({
        "device_id": device_id,
        "rotations": history,
    })))
}

pub async fn revoke_old_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Key revocation requires server admin privileges".to_string(),
        ));
    }

    let key_id = body
        .get("key_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let reason = body.get("reason").and_then(|v| v.as_str());

    if key_id.is_empty() {
        return Err(ApiError::bad_request(
            "key_id is required for key revocation".to_string(),
        ));
    }

    let rotation_manager = &state.services.key_rotation_manager;
    match rotation_manager.revoke_key(key_id, reason).await {
        Ok(revoked_count) => Ok(Json(json!({
            "success": true,
            "revoked": revoked_count,
            "message": if revoked_count > 0 {
                format!("Successfully revoked key {}", key_id)
            } else {
                format!("Key {} not found or already expired", key_id)
            }
        }))),
        Err(e) => {
            tracing::error!("Key revocation failed: {e}");
            Err(ApiError::internal("Internal server error".to_string()))
        }
    }
}

pub async fn configure_key_rotation(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Key rotation management requires server admin privileges".to_string(),
        ));
    }

    let enabled = body.get("enabled").and_then(|v| v.as_bool());
    let interval_ms = body.get("interval_ms").and_then(|v| v.as_i64());
    let rotation_interval_days = body.get("rotation_interval_days").and_then(|v| v.as_i64());
    let rotation_threshold_days = body.get("rotation_threshold_days").and_then(|v| v.as_i64());
    let grace_period_minutes = body.get("grace_period_minutes").and_then(|v| v.as_i64());
    let olm_rotation_days = body.get("olm_rotation_days").and_then(|v| v.as_i64());
    let megolm_rotation_messages = body.get("megolm_rotation_messages").and_then(|v| v.as_i64());
    let max_session_age_days = body.get("max_session_age_days").and_then(|v| v.as_i64());

    let rotation_manager = &state.services.key_rotation_manager;

    if let Some(enabled_val) = enabled {
        rotation_manager.set_rotation_enabled(enabled_val).await;
    }

    if let Some(interval) = interval_ms {
        state
            .services
            .key_rotation_storage
            .set_rotation_config("interval_ms", &interval.to_string())
            .await
            .map_err(|e| {
                tracing::error!("Failed to persist key rotation interval_ms: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
    }

    if let Some(days) = rotation_interval_days {
        rotation_manager
            .set_rotation_config_value("rotation_interval_days", &days.to_string())
            .await
            .map_err(|e| {
                tracing::error!("Failed to persist rotation_interval_days: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
    }

    if let Some(days) = rotation_threshold_days {
        rotation_manager
            .set_rotation_config_value("rotation_threshold_days", &days.to_string())
            .await
            .map_err(|e| {
                tracing::error!("Failed to persist rotation_threshold_days: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
    }

    if let Some(minutes) = grace_period_minutes {
        rotation_manager
            .set_rotation_config_value("grace_period_minutes", &minutes.to_string())
            .await
            .map_err(|e| {
                tracing::error!("Failed to persist grace_period_minutes: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
    }

    if olm_rotation_days.is_some() || megolm_rotation_messages.is_some() || max_session_age_days.is_some() {
        let storage = &state.services.key_rotation_storage;
        if let Some(days) = olm_rotation_days {
            storage
                .set_rotation_config("olm_rotation_days", &days.to_string())
                .await
                .map_err(|e| {
                    tracing::error!("Failed to persist olm_rotation_days: {e}");
                    ApiError::internal("Internal server error".to_string())
                })?;
        }
        if let Some(msgs) = megolm_rotation_messages {
            storage
                .set_rotation_config("megolm_rotation_messages", &msgs.to_string())
                .await
                .map_err(|e| {
                    tracing::error!("Failed to persist megolm_rotation_messages: {e}");
                    ApiError::internal("Internal server error".to_string())
                })?;
        }
        if let Some(days) = max_session_age_days {
            storage
                .set_rotation_config("max_session_age_days", &days.to_string())
                .await
                .map_err(|e| {
                    tracing::error!("Failed to persist max_session_age_days: {e}");
                    ApiError::internal("Internal server error".to_string())
                })?;
        }
    }

    let status = rotation_manager.get_rotation_status().await;

    let persisted_interval_ms: Option<i64> = if interval_ms.is_some() {
        interval_ms
    } else {
        state
            .services
            .key_rotation_storage
            .get_rotation_config("interval_ms")
            .await
            .ok()
            .flatten()
            .and_then(|v: String| v.parse().ok())
    };

    Ok(Json(json!({
        "enabled": status.get("rotation_enabled"),
        "interval_ms": persisted_interval_ms.unwrap_or(state.services.config.federation.key_rotation_grace_period_ms as i64),
        "rotation_interval_days": status.get("rotation_interval_days"),
        "rotation_threshold_days": status.get("rotation_threshold_days"),
        "grace_period_minutes": status.get("grace_period_minutes"),
    })))
}

/// POST variant of configure_key_rotation
pub async fn configure_key_rotation_post(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    configure_key_rotation(State(state), auth_user, Json(body)).await
}

pub async fn check_needs_rotation(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Key rotation management requires server admin privileges".to_string(),
        ));
    }

    // If key_id is provided, check if that specific key needs rotation
    let key_id_filter = params.get("key_id").map(|s| s.as_str());

    let last_rotation: Option<i64> = if let Some(key_id) = key_id_filter {
        state
            .services
            .key_rotation_storage
            .get_last_rotation_for_key(&auth_user.user_id, key_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to query rotation log by key_id: {e}");
                ApiError::internal("Internal server error".to_string())
            })?
    } else {
        let max_ts = state
            .services
            .key_rotation_storage
            .get_max_rotation_ts(&auth_user.user_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to query rotation log: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
        if max_ts == 0 {
            None
        } else {
            Some(max_ts)
        }
    };

    let now = Utc::now().timestamp_millis();
    let interval_ms = state
        .services
        .config
        .federation
        .key_rotation_grace_period_ms;

    let needs_rotation = match last_rotation {
        Some(last) => now - last > interval_ms as i64,
        None => true,
    };

    Ok(Json(json!({
        "needs_rotation": needs_rotation,
        "last_rotation": last_rotation,
        "interval_ms": interval_ms,
    })))
}

/// POST variant of check_needs_rotation — front-end MatrixEncryptionService uses POST
pub async fn check_needs_rotation_post(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    check_needs_rotation(State(state), auth_user, axum::extract::Query(params)).await
}

pub fn create_key_rotation_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v1/keys/rotation/status",
            get(get_key_rotation_status).post(get_key_rotation_status_post),
        )
        .route(
            "/_matrix/client/v1/keys/rotation/rotate",
            post(rotate_keys),
        )
        .route(
            "/_matrix/client/v1/keys/rotation/history/{device_id}",
            get(get_rotation_history),
        )
        .route(
            "/_matrix/client/v1/keys/rotation/revoke",
            post(revoke_old_keys),
        )
        .route(
            "/_matrix/client/v1/keys/rotation/config",
            put(configure_key_rotation).post(configure_key_rotation_post),
        )
        .route(
            "/_matrix/client/v1/keys/rotation/check",
            get(check_needs_rotation).post(check_needs_rotation_post),
        )
        .with_state(state)
}

pub fn key_rotation_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_matrix/client/v1/keys/rotation/status"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/status"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/rotate"),
        (
            Method::GET,
            "/_matrix/client/v1/keys/rotation/history/{device_id}",
        ),
        (Method::POST, "/_matrix/client/v1/keys/rotation/revoke"),
        (Method::PUT, "/_matrix/client/v1/keys/rotation/config"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/config"),
        (Method::GET, "/_matrix/client/v1/keys/rotation/check"),
        (Method::POST, "/_matrix/client/v1/keys/rotation/check"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "key_rotation"))
    .collect()
}
