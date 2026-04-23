use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde_json::{json, Value};

pub async fn get_key_rotation_status(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::forbidden(
        "Key rotation status is not available via the client API".to_string(),
    ))
}

pub async fn rotate_keys(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::forbidden(
        "Key rotation is not available via the client API".to_string(),
    ))
}

pub async fn get_rotation_history(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT new_key_id AS key_id, rotated_at AS rotated_ts FROM key_rotation_log
        WHERE user_id = $1 AND device_id = $2
        ORDER BY rotated_at DESC
        LIMIT 10
        "#,
    )
    .bind(&auth_user.user_id)
    .bind(&device_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get history: {}", e)))?;

    let history: Vec<Value> = rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            json!({
                "key_id": row.get::<Option<String>, _>("key_id"),
                "rotated_ts": row.get::<Option<i64>, _>("rotated_ts"),
            })
        })
        .collect();

    Ok(Json(json!({
        "device_id": device_id,
        "rotations": history,
    })))
}

pub async fn revoke_old_keys(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "success": true,
        "revoked": 0,
        "message": "Key revocation is handled automatically by key rotation"
    })))
}

pub async fn configure_key_rotation(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::forbidden(
        "Key rotation configuration is not available via the client API".to_string(),
    ))
}

pub async fn check_needs_rotation(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let last_rotation: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT MAX(rotated_at) FROM key_rotation_log
        WHERE user_id = $1
        "#,
    )
    .bind(&auth_user.user_id)
    .fetch_one(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed: {}", e)))?;

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

pub fn create_key_rotation_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v1/keys/rotation/status",
            get(get_key_rotation_status),
        )
        .route("/_matrix/client/v1/keys/rotation/rotate", post(rotate_keys))
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
            put(configure_key_rotation),
        )
        .route(
            "/_matrix/client/v1/keys/rotation/check",
            get(check_needs_rotation),
        )
        .with_state(state)
}
