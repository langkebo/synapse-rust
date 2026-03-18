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
    _auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let config = &state.services.config.federation;
    
    Ok(Json(json!({
        "enabled": true,
        "interval_ms": config.key_rotation_grace_period_ms,
    })))
}

pub async fn rotate_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let key_id = format!("ed25519:{}", uuid::Uuid::new_v4());
    let now = Utc::now().timestamp_millis();
    let device_id = "default".to_string();

    sqlx::query(
        r#"
        INSERT INTO key_rotation_history (user_id, device_id, key_id, rotated_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(&auth_user.user_id)
    .bind(&device_id)
    .bind(&key_id)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to record rotation: {}", e)))?;

    Ok(Json(json!({
        "success": true,
        "key_id": key_id,
        "rotated_at": now,
    })))
}

pub async fn get_rotation_history(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT key_id, rotated_at FROM key_rotation_history 
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
                "rotated_at": row.get::<Option<i64>, _>("rotated_at"),
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
    let device_id = body
        .get("device_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing device_id".to_string()))?;

    let key_ids = body
        .get("key_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    let mut revoked_count = 0;
    for key_id in &key_ids {
        let result = sqlx::query(
            r#"
            UPDATE key_rotation_history 
            SET revoked = TRUE 
            WHERE user_id = $1 AND device_id = $2 AND key_id = $3
            "#,
        )
        .bind(&auth_user.user_id)
        .bind(device_id)
        .bind(key_id)
        .execute(&*state.services.user_storage.pool)
        .await;

        if let Ok(r) = result {
            revoked_count += r.rows_affected() as u64;
        }
    }

    Ok(Json(json!({
        "success": true,
        "revoked": revoked_count,
    })))
}

pub async fn configure_key_rotation(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let interval_days = body.get("interval_days").and_then(|v| v.as_i64()).unwrap_or(30);

    Ok(Json(json!({
        "enabled": enabled,
        "interval_days": interval_days,
    })))
}

pub async fn check_needs_rotation(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let last_rotation: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT MAX(rotated_at) FROM key_rotation_history 
        WHERE user_id = $1
        "#,
    )
    .bind(&auth_user.user_id)
    .fetch_one(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed: {}", e)))?;

    let now = Utc::now().timestamp_millis();
    let interval_ms = state.services.config.federation.key_rotation_grace_period_ms;
    
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
        .route("/_matrix/client/v1/keys/rotation/status", get(get_key_rotation_status))
        .route("/_matrix/client/v1/keys/rotation/rotate", post(rotate_keys))
        .route("/_matrix/client/v1/keys/rotation/history/{device_id}", get(get_rotation_history))
        .route("/_matrix/client/v1/keys/rotation/revoke", post(revoke_old_keys))
        .route("/_matrix/client/v1/keys/rotation/config", put(configure_key_rotation))
        .route("/_matrix/client/v1/keys/rotation/check", get(check_needs_rotation))
        .with_state(state)
}
