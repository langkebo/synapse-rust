use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

pub async fn get_devices(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let devices = state.services.device_storage.get_user_devices(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get devices: {}", e)))?;

    let device_list: Vec<Value> = devices
        .into_iter()
        .map(|d| json!({
            "device_id": d.device_id,
            "display_name": d.display_name,
            "last_seen_ts": d.last_seen_ts,
            "last_seen_ip": d.last_seen_ip,
        }))
        .collect();

    Ok(Json(json!({
        "devices": device_list
    })))
}

pub async fn get_device(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let device = state.services.device_storage.get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device: {}", e)))?;

    match device {
        Some(d) => Ok(Json(json!({
            "device_id": d.device_id,
            "display_name": d.display_name,
            "last_seen_ts": d.last_seen_ts,
            "last_seen_ip": d.last_seen_ip,
        }))),
        None => Err(ApiError::not_found("Device not found".to_string())),
    }
}

pub async fn update_device(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let display_name = body
        .get("display_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing display_name".to_string()))?;

    state.services.device_storage.update_device_display_name(&device_id, display_name)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update device: {}", e)))?;

    Ok(Json(json!({})))
}

pub async fn delete_device(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.services.device_storage.delete_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete device: {}", e)))?;

    Ok(Json(json!({})))
}

pub async fn delete_devices(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let device_ids = body
        .get("device_ids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing device_ids".to_string()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect::<Vec<String>>();

    for device_id in &device_ids {
        let _ = state.services.device_storage.delete_device(device_id).await;
    }

    Ok(Json(json!({})))
}

pub fn create_device_router() -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/r0/devices", get(get_devices))
        .route("/_matrix/client/v3/devices", get(get_devices))
        .route("/_matrix/client/r0/delete_devices", post(delete_devices))
        .route("/_matrix/client/v3/delete_devices", post(delete_devices))
        .route("/_matrix/client/r0/devices/{device_id}", get(get_device).put(update_device).delete(delete_device))
        .route("/_matrix/client/v3/devices/{device_id}", get(get_device).put(update_device).delete(delete_device))
}
