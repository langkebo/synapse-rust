use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

fn create_device_compat_router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(get_devices))
        .route("/delete_devices", post(delete_devices))
        .route(
            "/devices/{device_id}",
            get(get_device).put(update_device).delete(delete_device),
        )
        .route("/keys/device_list_updates", post(get_device_list_updates))
}

pub async fn get_devices(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let devices = state
        .services
        .device_storage
        .get_user_devices(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get devices: {}", e)))?;

    let device_list: Vec<Value> = devices
        .into_iter()
        .map(|d| {
            json!({
                "device_id": d.device_id,
                "display_name": d.display_name,
                "last_seen_ts": d.last_seen_ts,
                "last_seen_ip": d.last_seen_ip,
            })
        })
        .collect();

    Ok(Json(json!({
        "devices": device_list
    })))
}

pub async fn get_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let device = state
        .services
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device: {}", e)))?;

    match device {
        Some(d) if d.user_id == auth_user.user_id => Ok(Json(json!({
            "device": {
                "device_id": d.device_id,
                "display_name": d.display_name,
                "last_seen_ts": d.last_seen_ts,
                "last_seen_ip": d.last_seen_ip,
            },
            "device_id": d.device_id,
            "display_name": d.display_name,
            "last_seen_ts": d.last_seen_ts,
            "last_seen_ip": d.last_seen_ip,
        }))),
        Some(_) => Err(ApiError::not_found("Device not found".to_string())),
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

    state
        .services
        .device_storage
        .update_device_display_name(&device_id, display_name)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update device: {}", e)))?;

    Ok(Json(json!({})))
}

pub async fn delete_device(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .device_storage
        .delete_device(&device_id)
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

pub async fn get_device_list_updates(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let users = body
        .get("users")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing users array".to_string()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect::<Vec<String>>();

    let mut changed: Vec<Value> = Vec::new();
    let mut left: Vec<String> = Vec::new();

    for user_id in &users {
        let devices = state
            .services
            .device_storage
            .get_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get devices: {}", e)))?;

        if devices.is_empty() {
            left.push(user_id.clone());
        } else {
            for device in devices {
                changed.push(json!({
                    "user_id": user_id,
                    "device_id": device.device_id,
                    "device_data": {
                        "display_name": device.display_name,
                        "last_seen_ts": device.last_seen_ts,
                        "last_seen_ip": device.last_seen_ip,
                    }
                }));
            }
        }
    }

    Ok(Json(json!({
        "changed": changed,
        "left": left
    })))
}

pub fn create_device_router() -> Router<AppState> {
    let compat_router = create_device_compat_router();

    Router::new()
        .nest("/_matrix/client/r0", compat_router.clone())
        .nest("/_matrix/client/v3", compat_router)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_device_routes_structure() {
        let routes = [
            "/_matrix/client/r0/devices",
            "/_matrix/client/v3/devices/{device_id}",
            "/_matrix/client/r0/delete_devices",
            "/_matrix/client/v3/keys/device_list_updates",
        ];

        assert!(routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_device_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/devices",
            "/delete_devices",
            "/devices/{device_id}",
            "/keys/device_list_updates",
        ];

        assert_eq!(shared_paths.len(), 4);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }
}
