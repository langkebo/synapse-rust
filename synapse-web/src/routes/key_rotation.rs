use crate::routes::context::DeviceContext;
use crate::routes::MatrixJson;
use crate::routes::{AdminUser, ApiError, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::types::DeviceId;

/// Request body for [`rotate_keys`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotateKeysRequest {
    /// Optional key identifier to rotate.
    pub key_id: Option<String>,
}

/// Request body for [`revoke_old_keys`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeOldKeysRequest {
    /// Key identifier to revoke.
    pub key_id: Option<String>,
    /// Optional audit reason.
    pub reason: Option<String>,
}

/// Request body for [`configure_key_rotation`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigureKeyRotationRequest {
    /// Whether automatic key rotation is enabled.
    pub enabled: Option<bool>,
    /// Rotation interval in milliseconds.
    pub interval_ms: Option<i64>,
    /// Rotation interval in days.
    pub rotation_interval_days: Option<i64>,
    /// Rotation threshold in days.
    pub rotation_threshold_days: Option<i64>,
    /// Grace period in minutes.
    pub grace_period_minutes: Option<i64>,
    /// Olm rotation interval in days.
    pub olm_rotation_days: Option<i64>,
    /// Megolm rotation interval in messages.
    pub megolm_rotation_messages: Option<i64>,
    /// Maximum session age in days.
    pub max_session_age_days: Option<i64>,
}

/// Resolve the rotation interval (milliseconds) used by the admin key-rotation
/// surface.
///
/// Two sources, in priority order:
/// 1. the operator-set value persisted in `key_rotation_config.interval_ms`
///    (written by `PUT/POST /keys/rotation/config`), then
/// 2. the config-file default `federation.key_rotation_grace_period_ms`.
///
/// A non-positive stored value counts as unset (the federation scheduler reads
/// the same key with the same `> 0` rule).
///
/// `/config` and `/check` **must** share this rule. They used to disagree:
/// `/config` reported the stored value while `/check` silently used the
/// config-file default, so an operator who set an interval saw `/config` echo
/// it back and `/check` ignore it.
///
/// **Scope (M-05 / §11.15).** This helper only governs *which interval is in
/// force*. It does not decide what is being rotated: `key_rotation_log` is the
/// **E2EE megolm** rotation audit log, scoped per `(user_id, device_id,
/// room_id)` — not federation signing-key rotation, which is per
/// `(server_name, key_id)` and lives in `federation_signing_keys`.
pub fn resolve_rotation_interval_ms(stored: Option<i64>, config_default_ms: u64) -> i64 {
    match stored {
        Some(value) if value > 0 => value,
        _ => config_default_ms as i64,
    }
}

/// See [`get_key_rotation_status`].
pub async fn get_key_rotation_status(
    State(ctx): State<DeviceContext>,
    admin_user: AdminUser,
) -> Result<Json<Value>, ApiError> {
    let (status, last_rotation) = ctx.key_rotation_service.get_rotation_status(&admin_user.user_id).await?;

    Ok(Json(json!({
        "enabled": status.get("rotation_enabled"),
        "status": status,
        "user_last_rotation": last_rotation,
    })))
}

/// POST variant of get_key_rotation_status — some clients use POST instead of GET
pub async fn get_key_rotation_status_post(
    State(ctx): State<DeviceContext>,
    admin_user: AdminUser,
) -> Result<Json<Value>, ApiError> {
    get_key_rotation_status(State(ctx), admin_user).await
}

/// See [`rotate_keys`].
pub async fn rotate_keys(
    State(ctx): State<DeviceContext>,
    _admin: AdminUser,
    MatrixJson(body): MatrixJson<RotateKeysRequest>,
) -> Result<Json<Value>, ApiError> {
    let requested_key_id = body.key_id;

    match ctx.key_rotation_service.rotate_keys(requested_key_id).await {
        Ok(has_new_key) => Ok(Json(json!({
            "success": true,
            "message": "Keys rotated successfully",
            "has_new_key": has_new_key,
        }))),
        Err(e) => {
            tracing::error!("Key rotation failed: {e}");
            Err(ApiError::internal("Internal server error".to_string()))
        }
    }
}

/// Query parameters for [`get_rotation_history`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationHistoryQuery {
    /// Maximum number of entries to return (1-500, default 100).
    pub limit: Option<i64>,
    /// Opaque keyset cursor taken from a previous response's `next_batch`.
    pub from: Option<String>,
}

fn parse_rotation_cursor(from: Option<&str>) -> Result<Option<(i64, i64)>, ApiError> {
    let Some(raw) = from.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let (ts, id) = raw.split_once('|').ok_or_else(|| ApiError::invalid_param("Invalid `from` cursor"))?;
    let ts = ts.parse::<i64>().map_err(|_| ApiError::invalid_param("Invalid `from` cursor"))?;
    let id = id.parse::<i64>().map_err(|_| ApiError::invalid_param("Invalid `from` cursor"))?;
    Ok(Some((ts, id)))
}

/// See [`get_rotation_history`].
pub async fn get_rotation_history(
    State(ctx): State<DeviceContext>,
    admin_user: AdminUser,
    Path(device_id): Path<DeviceId>,
    Query(query): Query<RotationHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let cursor = parse_rotation_cursor(query.from.as_deref())?;
    let (history_rows, next_batch) = ctx
        .key_rotation_service
        .get_rotation_history(&admin_user.user_id, device_id.as_str(), limit, cursor)
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
        "device_id": device_id.as_str(),
        "rotations": history,
        "next_batch": next_batch,
    })))
}

/// See [`revoke_old_keys`].
pub async fn revoke_old_keys(
    State(ctx): State<DeviceContext>,
    _admin: AdminUser,
    MatrixJson(body): MatrixJson<RevokeOldKeysRequest>,
) -> Result<Json<Value>, ApiError> {
    let key_id = body.key_id.unwrap_or_default();

    let reason = body.reason.as_deref();

    if key_id.is_empty() {
        return Err(ApiError::bad_request("key_id is required for key revocation".to_string()));
    }

    match ctx.key_rotation_service.revoke_key(&key_id, reason).await {
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

/// See [`configure_key_rotation`].
pub async fn configure_key_rotation(
    State(ctx): State<DeviceContext>,
    admin_user: AdminUser,
    MatrixJson(body): MatrixJson<ConfigureKeyRotationRequest>,
) -> Result<Json<Value>, ApiError> {
    let enabled = body.enabled;
    let interval_ms = body.interval_ms;
    let rotation_interval_days = body.rotation_interval_days;
    let rotation_threshold_days = body.rotation_threshold_days;
    let grace_period_minutes = body.grace_period_minutes;
    let olm_rotation_days = body.olm_rotation_days;
    let megolm_rotation_messages = body.megolm_rotation_messages;
    let max_session_age_days = body.max_session_age_days;

    if let Some(enabled_val) = enabled {
        ctx.key_rotation_service.set_rotation_enabled(enabled_val).await;
    }

    if let Some(interval) = interval_ms {
        ctx.key_rotation_service.set_rotation_interval_ms(interval).await.map_err(|e| {
            tracing::error!("Failed to persist key rotation interval_ms: {e}");
            ApiError::internal("Internal server error".to_string())
        })?;
    }

    if let Some(days) = rotation_interval_days {
        ctx.key_rotation_service.set_manager_config_value("rotation_interval_days", days).await.map_err(|e| {
            tracing::error!("Failed to persist rotation_interval_days: {e}");
            ApiError::internal("Internal server error".to_string())
        })?;
    }

    if let Some(days) = rotation_threshold_days {
        ctx.key_rotation_service.set_manager_config_value("rotation_threshold_days", days).await.map_err(|e| {
            tracing::error!("Failed to persist rotation_threshold_days: {e}");
            ApiError::internal("Internal server error".to_string())
        })?;
    }

    if let Some(minutes) = grace_period_minutes {
        ctx.key_rotation_service.set_manager_config_value("grace_period_minutes", minutes).await.map_err(|e| {
            tracing::error!("Failed to persist grace_period_minutes: {e}");
            ApiError::internal("Internal server error".to_string())
        })?;
    }

    if olm_rotation_days.is_some() || megolm_rotation_messages.is_some() || max_session_age_days.is_some() {
        if let Some(days) = olm_rotation_days {
            ctx.key_rotation_service.set_storage_config_value("olm_rotation_days", days).await.map_err(|e| {
                tracing::error!("Failed to persist olm_rotation_days: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
        }
        if let Some(msgs) = megolm_rotation_messages {
            ctx.key_rotation_service.set_storage_config_value("megolm_rotation_messages", msgs).await.map_err(|e| {
                tracing::error!("Failed to persist megolm_rotation_messages: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
        }
        if let Some(days) = max_session_age_days {
            ctx.key_rotation_service.set_storage_config_value("max_session_age_days", days).await.map_err(|e| {
                tracing::error!("Failed to persist max_session_age_days: {e}");
                ApiError::internal("Internal server error".to_string())
            })?;
        }
    }

    let (status, _) = ctx.key_rotation_service.get_rotation_status(&admin_user.user_id).await?;

    let persisted_interval_ms: Option<i64> = if interval_ms.is_some() {
        interval_ms
    } else {
        ctx.key_rotation_service.get_interval_ms().await.ok().flatten()
    };

    Ok(Json(json!({
        "enabled": status.get("rotation_enabled"),
        "interval_ms": resolve_rotation_interval_ms(
            persisted_interval_ms,
            ctx.config.federation.key_rotation_grace_period_ms,
        ),
        "rotation_interval_days": status.get("rotation_interval_days"),
        "rotation_threshold_days": status.get("rotation_threshold_days"),
        "grace_period_minutes": status.get("grace_period_minutes"),
    })))
}

/// POST variant of configure_key_rotation
pub async fn configure_key_rotation_post(
    State(ctx): State<DeviceContext>,
    admin_user: AdminUser,
    MatrixJson(body): MatrixJson<ConfigureKeyRotationRequest>,
) -> Result<Json<Value>, ApiError> {
    configure_key_rotation(State(ctx), admin_user, MatrixJson(body)).await
}

/// Query parameters shared by the `GET` and `POST` variants of
/// [`check_needs_rotation`].
///
/// Deliberately **not** `deny_unknown_fields`: both handlers used to take a
/// bare `Query<HashMap<..>>`, so unknown parameters were ignored. Keeping that
/// behaviour means a client adding `?foo=1` must keep getting a 200.
#[derive(Debug, Default, Deserialize)]
pub struct KeyRotationCheckParams {
    key_id: Option<String>,
}

/// See [`check_needs_rotation`].
pub async fn check_needs_rotation(
    State(ctx): State<DeviceContext>,
    admin_user: AdminUser,
    Query(params): Query<KeyRotationCheckParams>,
) -> Result<Json<Value>, ApiError> {
    // If key_id is provided, check if that specific key needs rotation
    let key_id_filter = params.key_id.as_deref();

    let last_rotation: Option<i64> = if let Some(key_id) = key_id_filter {
        ctx.key_rotation_service.get_last_rotation_for_key(&admin_user.user_id, key_id).await.map_err(|e| {
            tracing::error!("Failed to query rotation log by key_id: {e}");
            ApiError::internal("Internal server error".to_string())
        })?
    } else {
        let max_ts = ctx.key_rotation_service.get_max_rotation_ts(&admin_user.user_id).await.map_err(|e| {
            tracing::error!("Failed to query rotation log: {e}");
            ApiError::internal("Internal server error".to_string())
        })?;
        if max_ts == 0 {
            None
        } else {
            Some(max_ts)
        }
    };

    let stored_interval_ms = ctx.key_rotation_service.get_interval_ms().await.map_err(|e| {
        tracing::error!("Failed to read key rotation interval: {e}");
        ApiError::internal("Internal server error".to_string())
    })?;
    let interval_ms =
        resolve_rotation_interval_ms(stored_interval_ms, ctx.config.federation.key_rotation_grace_period_ms);

    let now = current_timestamp_millis();

    let needs_rotation = match last_rotation {
        Some(last) => now - last > interval_ms,
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
    State(ctx): State<DeviceContext>,
    admin_user: AdminUser,
    Query(params): Query<KeyRotationCheckParams>,
) -> Result<Json<Value>, ApiError> {
    check_needs_rotation(State(ctx), admin_user, Query(params)).await
}

/// See [`create_key_rotation_router`].
pub fn create_key_rotation_router(state: AppState) -> Router<AppState> {
    Router::new()
// ISSUE-13: 私有端点唯一规范位置是 /_matrix/vendor/v1（client 别名已按 AGENTS.md 铁律 1 删除）
        .route(
            "/_matrix/vendor/v1/keys/rotation/status",
            get(get_key_rotation_status).post(get_key_rotation_status_post),
        )
        .route("/_matrix/vendor/v1/keys/rotation/rotate", post(rotate_keys))
        .route("/_matrix/vendor/v1/keys/rotation/history/{device_id}", get(get_rotation_history))
        .route("/_matrix/vendor/v1/keys/rotation/revoke", post(revoke_old_keys))
        .route("/_matrix/vendor/v1/keys/rotation/config", put(configure_key_rotation).post(configure_key_rotation_post))
        .route("/_matrix/vendor/v1/keys/rotation/check", get(check_needs_rotation).post(check_needs_rotation_post))
        .with_state(state)
}
