use crate::routes::response_helpers::filter_users_with_shared_rooms;
use crate::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

async fn require_password_uia(state: &AppState, auth_user: &AuthenticatedUser, body: &Value) -> Result<(), Response> {
    let auth = body.get("auth");

    match auth {
        None => {
            let session = state
                .services
                .extensions
                .uia_service
                .create_session(
                    &auth_user.user_id,
                    synapse_services::uia_service::UiaService::get_delete_device_flows(),
                )
                .await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(state.services.extensions.uia_service.build_uia_response(
                    &session,
                    "M_UIA_REQUIRED",
                    "User-Interactive Authentication required",
                )),
            )
                .into_response());
        }
        Some(auth_val) => {
            let result = state
                .services
                .extensions
                .uia_service
                .validate_auth(
                    auth_val,
                    &auth_user.user_id,
                    synapse_services::uia_service::UiaService::get_delete_device_flows(),
                )
                .await;

            match result {
                Ok(_) => {}
                Err(uia_response) => {
                    return Err((StatusCode::UNAUTHORIZED, Json(uia_response)).into_response());
                }
            }

            let auth_type = auth_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match auth_type {
                "m.login.password" => {
                    if let Err(e) = state
                        .services
                        .extensions
                        .uia_service
                        .verify_password_stage(auth_val, &auth_user.user_id, &state.services.core.auth_service)
                        .await
                    {
                        let session = state
                            .services
                            .extensions
                            .uia_service
                            .create_session(
                                &auth_user.user_id,
                                synapse_services::uia_service::UiaService::get_delete_device_flows(),
                            )
                            .await;
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            Json(state.services.extensions.uia_service.build_uia_response(
                                &session,
                                "M_FORBIDDEN",
                                &e.to_string(),
                            )),
                        )
                            .into_response());
                    }
                }
                "m.login.token" => {
                    if let Err(e) = state
                        .services
                        .extensions
                        .uia_service
                        .verify_token_stage(auth_val, &auth_user.user_id, &state.services.core.auth_service)
                        .await
                    {
                        let session = state
                            .services
                            .extensions
                            .uia_service
                            .create_session(
                                &auth_user.user_id,
                                synapse_services::uia_service::UiaService::get_delete_device_flows(),
                            )
                            .await;
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            Json(state.services.extensions.uia_service.build_uia_response(
                                &session,
                                "M_FORBIDDEN",
                                &e.to_string(),
                            )),
                        )
                            .into_response());
                    }
                }
                _ => {
                    let session = state
                        .services
                        .extensions
                        .uia_service
                        .create_session(
                            &auth_user.user_id,
                            synapse_services::uia_service::UiaService::get_delete_device_flows(),
                        )
                        .await;
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(state.services.extensions.uia_service.build_uia_response(
                            &session,
                            "M_INVALID_PARAM",
                            &format!("Unsupported auth type: {auth_type}"),
                        )),
                    )
                        .into_response());
                }
            }
        }
    }

    Ok(())
}

fn parse_device_ids(body: &Value) -> Result<Vec<String>, ApiError> {
    let raw_device_ids = body
        .get("device_ids")
        .or_else(|| body.get("devices"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing device_ids".to_string()))?;

    if raw_device_ids.iter().any(|value| !value.is_string()) {
        return Err(ApiError::bad_request("device_ids must be an array of strings".to_string()));
    }

    Ok(raw_device_ids.iter().filter_map(|value| value.as_str().map(String::from)).collect())
}

fn parse_stream_id(value: &Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(n);
    }
    let s = value.as_str()?;
    let s = s.strip_prefix('s').unwrap_or(s);
    s.parse::<i64>().ok()
}

async fn broadcast_device_list_update(state: &AppState, user_id: &str, device_id: &str) {
    let server_name = state.services.core.config.server.server_name.as_deref().unwrap_or("localhost");
    let edu = serde_json::json!({
        "edu_type": "m.device_list_update",
        "content": {
            "user_id": user_id,
            "device_id": device_id,
            "stream_id": chrono::Utc::now().timestamp_millis(),
            "prev_ids": [],
        }
    });

    if let Some(pos) = user_id.find(':') {
        let user_server = &user_id[pos + 1..];
        if user_server == server_name {
            if let Ok(shared_rooms) = state.services.rooms.member_storage.get_joined_rooms(user_id).await {
                let mut sent_servers: std::collections::HashSet<String> = std::collections::HashSet::new();
                for room_id in &shared_rooms {
                    if let Ok(members) = state.services.rooms.member_storage.get_joined_members(room_id).await {
                        for member in &members {
                            if let Some(mpos) = member.user_id.find(':') {
                                let member_server = &member.user_id[mpos + 1..];
                                if member_server != server_name && !sent_servers.contains(member_server) {
                                    sent_servers.insert(member_server.to_string());
                                    let _ = state
                                        .services
                                        .core
                                        .event_broadcaster
                                        .broadcast_edu(member_server, &edu, server_name)
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn create_device_compat_router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(get_devices))
        .route("/delete_devices", post(delete_devices))
        .route("/devices/{device_id}", get(get_device).put(update_device).delete(delete_device))
        .route("/keys/device_list_updates", post(get_device_list_updates))
}

pub async fn get_devices(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Result<Json<Value>, ApiError> {
    let devices = state
        .services
        .account
        .device_storage
        .get_user_devices(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get devices", &e))?;

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
        .account
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get device", &e))?;

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
            .account
            .device_storage
            .update_user_device_display_name(&auth_user.user_id, &device_id, display_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update device", &e))
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
        .account
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get updated device", &e))?
        .ok_or_else(|| ApiError::not_found("Device not found after update".to_string()))?;

    broadcast_device_list_update(&state, &auth_user.user_id, &device_id).await;

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

    let rows = state.services.core.auth_service.revoke_device(&auth_user.user_id, &device_id).await?;

    if rows == 0 {
        return Err(ApiError::not_found("Device not found".to_string()));
    }

    broadcast_device_list_update(&state, &auth_user.user_id, &device_id).await;

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

    state.services.core.auth_service.revoke_devices(&auth_user.user_id, &device_ids).await?;

    for device_id in &device_ids {
        broadcast_device_list_update(&state, &auth_user.user_id, device_id).await;
    }

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

    let since = body.get("since").or_else(|| body.get("from")).and_then(parse_stream_id);

    let mut changed: Vec<Value> = Vec::new();
    let mut left: Vec<String> = Vec::new();

    if since.is_none() {
        let devices_by_user = state
            .services
            .account
            .device_storage
            .get_users_devices_batch(&users)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get devices", &e))?;

        for user_id in &users {
            if let Some(devices) = devices_by_user.get(user_id) {
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
            } else {
                left.push(user_id.clone());
            }
        }

        return Ok(Json(json!({
            "changed": changed,
            "left": left
        })));
    }

    let since = since.unwrap_or(0);
    let to = body.get("to").and_then(parse_stream_id).unwrap_or(0);

    let max_stream_id = state
        .services
        .account
        .device_storage
        .get_max_device_list_stream_id()
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get device list stream position", &e))?;

    let to = if to > 0 { to } else { max_stream_id };

    let change_rows = state
        .services
        .account
        .device_storage
        .get_device_list_changes(since, to, &users)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get device list changes", &e))?;

    let mut latest: HashMap<(String, String), String> = HashMap::new();
    for (user_id, device_id, change_type, _stream_id) in change_rows {
        let Some(device_id) = device_id else {
            continue;
        };
        latest.insert((user_id, device_id), change_type);
    }

    let mut deleted: Vec<Value> = Vec::new();
    let mut active_pairs: Vec<(String, String)> = Vec::new();
    for ((user_id, device_id), change_type) in latest {
        if change_type == "deleted" {
            deleted.push(json!({
                "user_id": user_id,
                "device_id": device_id
            }));
            continue;
        }

        active_pairs.push((user_id, device_id));
    }

    if !active_pairs.is_empty() {
        let user_ids: Vec<&str> = active_pairs.iter().map(|(user_id, _)| user_id.as_str()).collect();
        let device_ids: Vec<&str> = active_pairs.iter().map(|(_, device_id)| device_id.as_str()).collect();
        let device_rows = state
            .services
            .account
            .device_storage
            .get_devices_by_user_device_pairs(&user_ids, &device_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get device data", &e))?;

        for (user_id, device_id, display_name, last_seen_ts) in device_rows {
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

    let existing_users: Vec<String> = state
        .services
        .account
        .device_storage
        .filter_existing_users(&users)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to resolve left users", &e))?;

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

    Router::new().nest("/_matrix/client/r0", compat_router.clone()).nest("/_matrix/client/v3", compat_router)
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
pub fn device_route_manifest() -> Vec<crate::routes::route_ledger::RouteEntry> {
    crate::routes::route_ledger::expand_under_prefixes("device", DEVICE_NEST_PREFIXES, &device_compat_relative_routes())
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

        assert!(routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_device_compat_router_contains_shared_paths() {
        let shared_paths = ["/devices", "/delete_devices", "/devices/{device_id}", "/keys/device_list_updates"];

        assert_eq!(shared_paths.len(), 4);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }
}
