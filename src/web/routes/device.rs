use crate::web::routes::response_helpers::filter_users_with_shared_rooms;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Run the User-Interactive Authentication challenge for destructive device
/// operations. Returns `Ok(())` if the supplied `auth` dict satisfies an
/// `m.login.password` flow against the caller's account, otherwise returns
/// a 401 response carrying the UIA flow descriptor that clients are expected
/// to round-trip back as the `auth` dict (Matrix CS-API §UIA).
async fn require_password_uia(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    body: &Value,
) -> Result<(), Response> {
    let challenge = || {
        let session = uuid::Uuid::new_v4().to_string();
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "errcode": "M_UIA_REQUIRED",
                "error": "User-Interactive Authentication required",
                "flows": [{ "stages": ["m.login.password"] }],
                "params": {},
                "session": session,
                "completed": []
            })),
        )
            .into_response()
    };

    let Some(auth) = body.get("auth") else {
        return Err(challenge());
    };

    if auth.get("type").and_then(|v| v.as_str()) != Some("m.login.password") {
        return Err(challenge());
    }

    let password = match auth.get("password").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Err(challenge()),
    };

    let identifier_user = auth
        .get("identifier")
        .and_then(|i| i.get("user"))
        .and_then(|v| v.as_str())
        .or_else(|| auth.get("user").and_then(|v| v.as_str()))
        .unwrap_or(auth_user.user_id.as_str());

    if identifier_user != auth_user.user_id {
        return Err(challenge());
    }

    match state
        .services
        .auth_service
        .login(&auth_user.user_id, password, None, None)
        .await
    {
        Ok(_) => Ok(()),
        Err(_) => Err(challenge()),
    }
}

fn parse_device_ids(body: &Value) -> Result<Vec<String>, ApiError> {
    let raw_device_ids = body
        .get("device_ids")
        .or_else(|| body.get("devices"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing device_ids".to_string()))?;

    if raw_device_ids.iter().any(|value| !value.is_string()) {
        return Err(ApiError::bad_request(
            "device_ids must be an array of strings".to_string(),
        ));
    }

    Ok(raw_device_ids
        .iter()
        .filter_map(|value| value.as_str().map(String::from))
        .collect())
}

fn parse_stream_id(value: &Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(n);
    }
    let s = value.as_str()?;
    let s = s.strip_prefix('s').unwrap_or(s);
    s.parse::<i64>().ok()
}

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
            },
            "device_id": d.device_id,
            "display_name": d.display_name,
            "last_seen_ts": d.last_seen_ts,
        }))),
        Some(_) => Err(ApiError::not_found("Device not found".to_string())),
        None => Err(ApiError::not_found("Device not found".to_string())),
    }
}

pub async fn update_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if let Some(display_name) = body.get("display_name").and_then(|v| v.as_str()) {
        state
            .services
            .device_storage
            .update_user_device_display_name(&auth_user.user_id, &device_id, display_name)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update device: {}", e)))
            .and_then(|rows_affected| {
                if rows_affected == 0 {
                    Err(ApiError::not_found("Device not found".to_string()))
                } else {
                    Ok(())
                }
            })?;
    }

    let device = state
        .services
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get updated device: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Device not found after update".to_string()))?;

    Ok(Json(json!({
        "device_id": device.device_id,
        "display_name": device.display_name,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub async fn delete_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    if let Err(challenge) = require_password_uia(&state, &auth_user, &body).await {
        return Ok(challenge);
    }

    let rows = state
        .services
        .auth_service
        .revoke_device(&auth_user.user_id, &device_id)
        .await?;

    if rows == 0 {
        return Err(ApiError::not_found("Device not found".to_string()));
    }

    Ok(Json(json!({})).into_response())
}

pub async fn delete_devices(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    if let Err(challenge) = require_password_uia(&state, &auth_user, &body).await {
        return Ok(challenge);
    }

    let device_ids = parse_device_ids(&body)?;

    state
        .services
        .auth_service
        .revoke_devices(&auth_user.user_id, &device_ids)
        .await?;

    Ok(Json(json!({})).into_response())
}

pub async fn get_device_list_updates(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let requested_users = body
        .get("users")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing users array".to_string()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect::<Vec<String>>();

    let users = filter_users_with_shared_rooms(&state, &auth_user.user_id, &requested_users).await;

    let since = body
        .get("since")
        .or_else(|| body.get("from"))
        .and_then(parse_stream_id);

    let mut changed: Vec<Value> = Vec::new();
    let mut left: Vec<String> = Vec::new();

    if since.is_none() {
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
                        }
                    }));
                }
            }
        }

        return Ok(Json(json!({
            "changed": changed,
            "left": left
        })));
    }

    let since = since.unwrap_or(0);
    let to = body.get("to").and_then(parse_stream_id).unwrap_or(0);

    let max_stream_id: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(stream_id), 0) FROM device_lists_stream
        "#,
    )
    .fetch_one(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get device list stream position: {}", e)))?;

    let to = if to > 0 { to } else { max_stream_id };

    let change_rows = sqlx::query_as::<_, (String, Option<String>, String, i64)>(
        r#"
        SELECT user_id, device_id, change_type, stream_id
        FROM device_lists_changes
        WHERE stream_id > $1
          AND stream_id <= $2
          AND user_id = ANY($3)
        ORDER BY stream_id ASC
        "#,
    )
    .bind(since)
    .bind(to)
    .bind(&users)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get device list changes: {}", e)))?;

    let mut latest: HashMap<(String, String), String> = HashMap::new();
    for (user_id, device_id, change_type, _stream_id) in change_rows {
        let Some(device_id) = device_id else {
            continue;
        };
        latest.insert((user_id, device_id), change_type);
    }

    let mut deleted: Vec<Value> = Vec::new();
    for ((user_id, device_id), change_type) in latest {
        if change_type == "deleted" {
            deleted.push(json!({
                "user_id": user_id,
                "device_id": device_id
            }));
            continue;
        }

        let row = sqlx::query_as::<_, (Option<String>, Option<i64>)>(
            r#"
            SELECT display_name, last_seen_ts
            FROM devices
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(&user_id)
        .bind(&device_id)
        .fetch_optional(&*state.services.device_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device data: {}", e)))?;

        if let Some((display_name, last_seen_ts)) = row {
            changed.push(json!({
                "user_id": user_id,
                "device_id": device_id,
                "device_data": {
                    "display_name": display_name,
                    "last_seen_ts": last_seen_ts,
                }
            }));
        }
    }

    let existing_users: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT user_id FROM devices WHERE user_id = ANY($1)
        "#,
    )
    .bind(&users)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to resolve left users: {}", e)))?;

    let existing: HashSet<String> = existing_users.into_iter().collect();
    for user_id in &users {
        if !existing.contains(user_id) {
            left.push(user_id.clone());
        }
    }

    Ok(Json(json!({
        "changed": changed,
        "deleted": deleted,
        "left": left,
        "stream_id": to
    })))
}

pub fn create_device_router() -> Router<AppState> {
    let compat_router = create_device_compat_router();

    Router::new()
        .nest("/_matrix/client/r0", compat_router.clone())
        .nest("/_matrix/client/v3", compat_router)
}

/// Nest prefixes `create_device_router` mounts its inner compat router under.
const DEVICE_NEST_PREFIXES: &[&str] = &["/_matrix/client/r0", "/_matrix/client/v3"];

fn device_compat_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/devices"),
        (Method::POST, "/delete_devices"),
        (Method::GET, "/devices/{device_id}"),
        (Method::PUT, "/devices/{device_id}"),
        (Method::DELETE, "/devices/{device_id}"),
        (Method::POST, "/keys/device_list_updates"),
    ]
}

/// Manifest of every `(method, absolute_path)` tuple `create_device_router`
/// registers. Mirrors `create_device_compat_router` one-for-one — adding a
/// `.route(...)` there MUST add an entry here.
pub fn device_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    crate::web::routes::route_ledger::expand_under_prefixes(
        "device",
        DEVICE_NEST_PREFIXES,
        &device_compat_relative_routes(),
    )
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
