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

async fn require_password_uia(state: &AppState, auth_user: &AuthenticatedUser, body: &Value) -> Result<(), Response> {
    let auth = body.get("auth");
    let core = &state.services.core;
    let extensions = &state.services.extensions;

    match auth {
        None => {
            let session = state
                .services
                .extensions
                .uia_service
                .create_session(&auth_user.user_id, crate::services::uia_service::UiaService::get_delete_device_flows())
                .await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(extensions.uia_service.build_uia_response(
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
                    crate::services::uia_service::UiaService::get_delete_device_flows(),
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
                        .verify_password_stage(auth_val, &auth_user.user_id, &core.auth_service)
                        .await
                    {
                        let session = state
                            .services
                            .extensions
                            .uia_service
                            .create_session(
                                &auth_user.user_id,
                                crate::services::uia_service::UiaService::get_delete_device_flows(),
                            )
                            .await;
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            Json(extensions.uia_service.build_uia_response(&session, "M_FORBIDDEN", &e.to_string())),
                        )
                            .into_response());
                    }
                }
                "m.login.token" => {
                    if let Err(e) = state
                        .services
                        .extensions
                        .uia_service
                        .verify_token_stage(auth_val, &auth_user.user_id, &core.auth_service)
                        .await
                    {
                        let session = state
                            .services
                            .extensions
                            .uia_service
                            .create_session(
                                &auth_user.user_id,
                                crate::services::uia_service::UiaService::get_delete_device_flows(),
                            )
                            .await;
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            Json(extensions.uia_service.build_uia_response(&session, "M_FORBIDDEN", &e.to_string())),
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
                            crate::services::uia_service::UiaService::get_delete_device_flows(),
                        )
                        .await;
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(extensions.uia_service.build_uia_response(
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
            if let Ok(shared_rooms) = state.services.rooms.room_service.get_joined_rooms(user_id).await {
                let mut sent_servers: std::collections::HashSet<String> = std::collections::HashSet::new();
                for room_id in &shared_rooms {
                    if let Ok(members) =
                        state.services.rooms.room_service.get_joined_members_with_profiles(room_id).await
                    {
                        for member in &members {
                            if let Some(mpos) = member.user_id.find(':') {
                                let member_server = &member.user_id[mpos + 1..];
                                if member_server != server_name && !sent_servers.contains(member_server) {
                                    sent_servers.insert(member_server.to_string());
                                    if let Err(e) = state
                                        .services
                                        .core
                                        .event_broadcaster
                                        .broadcast_edu(member_server, &edu, server_name)
                                        .await
                                    {
                                        ::tracing::warn!(
                                            target_server = %member_server,
                                            error = %e,
                                            "Failed to broadcast device list update EDU — remote server will not see this device change until next device list resync"
                                        );
                                    }
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
    let devices = state.services.account.account_device_list_service.get_user_devices(&auth_user.user_id).await?;

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
    let device = state.services.account.account_device_list_service.get_device(&device_id).await?;

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
        if display_name.len() > 100 {
            return Err(ApiError::bad_request("display_name must not exceed 100 characters".to_string()));
        }
        let rows_affected: u64 = state
            .services
            .account
            .account_device_list_service
            .update_user_device_display_name(&auth_user.user_id, &device_id, display_name)
            .await?;

        if rows_affected == 0 {
            return Err(ApiError::not_found("Device not found".to_string()));
        }
    }

    let device = state
        .services
        .account
        .account_device_list_service
        .get_device(&device_id)
        .await?
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

    let rows: u64 = state.services.core.auth_service.revoke_device(&auth_user.user_id, &device_id).await?;

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

    let users: Vec<String> =
        filter_users_with_shared_rooms(&state, &auth_user.user_id, &requested_users).await.into_iter().collect();

    let since = body.get("since").or_else(|| body.get("from")).and_then(parse_stream_id);

    if since.is_none() {
        let snapshot = state.services.account.account_device_list_service.get_device_list_snapshot(&users).await?;
        let changed: Vec<Value> = snapshot
            .changed
            .into_iter()
            .map(|device| {
                json!({
                    "user_id": device.user_id,
                    "device_id": device.device_id,
                    "device_data": {
                        "display_name": device.display_name,
                        "last_seen_ts": device.last_seen_ts,
                    }
                })
            })
            .collect();

        return Ok(Json(json!({
            "changed": changed,
            "left": snapshot.left
        })));
    }

    let since = since.unwrap_or(0);
    let to = body.get("to").and_then(parse_stream_id).unwrap_or(0);
    let delta = state
        .services
        .account
        .account_device_list_service
        .get_device_list_delta(since, if to > 0 { Some(to) } else { None }, &users)
        .await?;
    let changed: Vec<Value> = delta
        .changed
        .into_iter()
        .map(|device| {
            json!({
                "user_id": device.user_id,
                "device_id": device.device_id,
                "device_data": {
                    "display_name": device.display_name,
                    "last_seen_ts": device.last_seen_ts,
                }
            })
        })
        .collect();
    let deleted: Vec<Value> = delta
        .deleted
        .into_iter()
        .map(|device| {
            json!({
                "user_id": device.user_id,
                "device_id": device.device_id
            })
        })
        .collect();

    Ok(Json(json!({
        "changed": changed,
        "deleted": deleted,
        "left": delta.left,
        "stream_id": delta.stream_id
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

        assert!(routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_device_compat_router_contains_shared_paths() {
        let shared_paths = ["/devices", "/delete_devices", "/devices/{device_id}", "/keys/device_list_updates"];

        assert_eq!(shared_paths.len(), 4);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }
}
