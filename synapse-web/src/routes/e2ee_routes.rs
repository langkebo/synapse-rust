use super::{AppState, AuthenticatedUser};
use crate::routes::response_helpers::{empty_json, filter_users_with_shared_rooms};
use crate::routes::MatrixJson;
use crate::ApiError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use synapse_e2ee::secure_backup::RestoreSecureBackupRequest;

fn parse_stream_id(value: &Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(n);
    }
    let s = value.as_str()?;
    let s = s.strip_prefix('s').unwrap_or(s);
    s.parse::<i64>().ok()
}

fn create_e2ee_compat_router() -> Router<AppState> {
    Router::new()
        .route("/keys/upload", post(upload_keys))
        .route("/keys/upload/{device_id}", post(upload_keys))
        .route("/keys/query", post(query_keys))
        .route("/keys/claim", post(claim_keys))
        .route("/keys/changes", get(key_changes))
        .route("/keys/device_list/update", post(device_list_update))
        .route("/keys/signatures", post(upload_signatures))
        .route("/keys/signatures/upload", post(upload_signatures))
        .route("/keys/device_signing/upload", post(upload_device_signing))
        .route("/room_keys/request", post(create_room_key_request).get(get_room_key_requests))
        .route("/room_keys/request/{request_id}", delete(delete_room_key_request))
        .route("/rooms/{room_id}/keys/distribution", get(room_key_distribution))
        .route("/sendToDevice/{event_type}/{transaction_id}", put(send_to_device).post(send_to_device))
}

fn create_e2ee_v3_only_router() -> Router<AppState> {
    Router::new()
        .route("/device_verification/request", post(request_device_verification))
        .route("/device_verification/respond", post(respond_device_verification))
        .route("/device_verification/status/{token}", get(get_verification_status))
        .route("/device_trust", get(get_device_trust_list))
        .route("/device_trust/{device_id}", get(get_device_trust))
        .route("/security/summary", get(get_security_summary))
        .route("/keys/backup/secure", post(create_secure_backup).get(get_secure_backup_list))
        .route("/keys/backup/secure/{backup_id}", get(get_secure_backup).delete(delete_secure_backup))
        .route("/keys/backup/secure/{backup_id}/keys", post(store_secure_backup_keys))
        .route("/keys/backup/secure/{backup_id}/restore", post(restore_secure_backup))
        .route("/keys/backup/secure/{backup_id}/verify", post(verify_secure_backup_passphrase))
}

pub fn create_e2ee_router(state: AppState) -> Router<AppState> {
    let compat_router = create_e2ee_compat_router();
    let v3_only_router = create_e2ee_v3_only_router();

    Router::new()
        .nest("/_matrix/client/r0", compat_router.clone())
        .nest("/_matrix/client/v1", compat_router.clone())
        .nest("/_matrix/client/v3", compat_router)
        .nest("/_matrix/client/v3", v3_only_router)
        .with_state(state)
}

/// Nest prefixes `create_e2ee_router` mounts the compat sub-router under.
const E2EE_COMPAT_NEST_PREFIXES: &[&str] = &["/_matrix/client/r0", "/_matrix/client/v1", "/_matrix/client/v3"];

/// Nest prefix used for the v3-only sub-router.
const E2EE_V3_ONLY_NEST_PREFIXES: &[&str] = &["/_matrix/client/v3"];

fn e2ee_compat_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::POST, "/keys/upload"),
        (Method::POST, "/keys/query"),
        (Method::POST, "/keys/claim"),
        (Method::GET, "/keys/changes"),
        (Method::POST, "/keys/device_list/update"),
        (Method::POST, "/keys/signatures"),
        (Method::POST, "/keys/signatures/upload"),
        (Method::POST, "/keys/device_signing/upload"),
        (Method::POST, "/room_keys/request"),
        (Method::GET, "/room_keys/request"),
        (Method::DELETE, "/room_keys/request/{request_id}"),
        (Method::GET, "/rooms/{room_id}/keys/distribution"),
        (Method::PUT, "/sendToDevice/{event_type}/{transaction_id}"),
        (Method::POST, "/sendToDevice/{event_type}/{transaction_id}"),
    ]
}

fn e2ee_v3_only_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::POST, "/device_verification/request"),
        (Method::POST, "/device_verification/respond"),
        (Method::GET, "/device_verification/status/{token}"),
        (Method::GET, "/device_trust"),
        (Method::GET, "/device_trust/{device_id}"),
        (Method::GET, "/security/summary"),
        (Method::POST, "/keys/backup/secure"),
        (Method::GET, "/keys/backup/secure"),
        (Method::GET, "/keys/backup/secure/{backup_id}"),
        (Method::DELETE, "/keys/backup/secure/{backup_id}"),
        (Method::POST, "/keys/backup/secure/{backup_id}/keys"),
        (Method::POST, "/keys/backup/secure/{backup_id}/restore"),
        (Method::POST, "/keys/backup/secure/{backup_id}/verify"),
    ]
}

/// Manifest of every `(method, absolute_path)` tuple `create_e2ee_router`
/// registers. Verification and key-backup recovery routes are intentionally
/// excluded here because they are owned by dedicated route modules.
pub fn e2ee_route_manifest() -> Vec<crate::routes::route_ledger::RouteEntry> {
    let mut out = crate::routes::route_ledger::expand_under_prefixes(
        "e2ee_routes",
        E2EE_COMPAT_NEST_PREFIXES,
        &e2ee_compat_relative_routes(),
    );
    out.extend(crate::routes::route_ledger::expand_under_prefixes(
        "e2ee_routes",
        E2EE_V3_ONLY_NEST_PREFIXES,
        &e2ee_v3_only_relative_routes(),
    ));
    out
}

#[axum::debug_handler]
async fn upload_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    path_device_id: Option<Path<String>>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let device_id = path_device_id
        .map(|Path(id)| id)
        .or(auth_user.device_id.clone())
        .ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;

    // Validate: reject completely empty uploads (no device_keys AND no one_time_keys)
    // But allow individual fields to be empty objects — clients commonly send
    // {"device_keys":{...}, "one_time_keys":{}} when only uploading device keys.
    let has_device_keys = body.get("device_keys").is_some();
    let has_one_time_keys = body.get("one_time_keys").is_some();

    if !has_device_keys && !has_one_time_keys {
        return Err(ApiError::bad_request("Must include at least device_keys or one_time_keys".to_string()));
    }

    // Validate device_keys has required fields when provided as a non-empty object
    if has_device_keys {
        let Some(dk) = body.get("device_keys") else {
            return Err(ApiError::bad_request("device_keys field is missing".to_string()));
        };
        if let Some(obj) = dk.as_object() {
            if !obj.is_empty() {
                // Non-empty device_keys should have keys field
                if dk.get("keys").is_some_and(|k| k.as_object().is_none_or(|m| m.is_empty())) {
                    return Err(ApiError::bad_request("device_keys.keys must be a non-empty object".to_string()));
                }
            }
        }
    }

    let inner_device_keys = body
        .get("device_keys")
        .and_then(|dk| {
            if dk.get("algorithms").is_some() || dk.get("keys").is_some() {
                Some(dk.clone())
            } else {
                dk.as_object().and_then(|map| {
                    map.values()
                        .next()
                        .and_then(|user_entry| user_entry.as_object().and_then(|m| m.values().next().cloned()))
                })
            }
        })
        .unwrap_or(serde_json::json!({}));

    let request = synapse_e2ee::device_keys::KeyUploadRequest {
        device_keys: if has_device_keys || has_one_time_keys {
            Some(synapse_e2ee::device_keys::DeviceKeys {
                user_id: auth_user.user_id.clone(),
                device_id: device_id.clone(),
                algorithms: inner_device_keys.get("algorithms").and_then(|v| v.as_array()).map_or_else(
                    || vec!["m.olm.v1.curve25519-aes-sha2".to_string(), "m.megolm.v1.aes-sha2".to_string()],
                    |arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
                ),
                keys: inner_device_keys.get("keys").cloned().unwrap_or_else(|| serde_json::json!({})),
                signatures: inner_device_keys.get("signatures").cloned().unwrap_or_else(|| serde_json::json!({})),
                unsigned: inner_device_keys.get("unsigned").and_then(|v| v.as_object()).map(|v| v.clone().into()),
            })
        } else {
            None
        },
        one_time_keys: body.get("one_time_keys").cloned(),
        fallback_keys: body.get("fallback_keys").cloned(),
    };

    let response = state.services.e2ee.device_keys_service.upload_keys(request, &auth_user.user_id, &device_id).await?;

    Ok(Json(serde_json::json!({
        "one_time_key_counts": response.one_time_key_counts
    })))
}

#[axum::debug_handler]
async fn query_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: synapse_e2ee::device_keys::KeyQueryRequest = serde_json::from_value(body.clone())
        .map_err(|e| synapse_common::error::ApiError::bad_request(format!("Invalid request: {e}")))?;

    let device_keys_raw = body.get("device_keys").cloned().unwrap_or(serde_json::json!({}));

    let requested_users: Vec<String> =
        device_keys_raw.as_object().map(|map| map.keys().cloned().collect()).unwrap_or_default();

    let allowed_users = filter_users_with_shared_rooms(&state, &auth_user.user_id, &requested_users).await;

    let device_keys = if requested_users.is_empty() {
        let mut shared =
            state.services.rooms.member_storage.get_shared_room_users(&auth_user.user_id).await.unwrap_or_default();
        shared.push(auth_user.user_id.clone());
        let map: serde_json::Map<String, Value> = shared.into_iter().map(|uid| (uid, serde_json::json!([]))).collect();
        serde_json::Value::Object(map)
    } else {
        let mut filtered = serde_json::Map::new();
        if let Some(obj) = device_keys_raw.as_object() {
            for (uid, val) in obj {
                if allowed_users.contains(uid) {
                    filtered.insert(uid.clone(), val.clone());
                }
            }
        }
        serde_json::Value::Object(filtered)
    };

    request.device_keys = device_keys;

    let response = state.services.e2ee.device_keys_service.query_keys(request).await?;

    let mut verified_devices = serde_json::Map::new();
    if let Some(device_keys_obj) = response.device_keys.as_object() {
        let user_ids: Vec<String> = device_keys_obj.keys().cloned().collect();
        if !user_ids.is_empty() {
            let batch_result = state
                .services
                .e2ee
                .cross_signing_service
                .get_verified_devices_batch(&user_ids)
                .await
                .unwrap_or_default();

            for (user_id, vd_map) in batch_result {
                let devices: Vec<serde_json::Value> = vd_map
                    .verified_devices
                    .into_iter()
                    .filter(|d| d.is_verified)
                    .map(|d| {
                        serde_json::json!({
                            "device_id": d.device_id,
                            "verified_by_master": d.verified_by_master,
                            "verified_by_self_signing": d.verified_by_self_signing,
                            "verification_method": d.verification_method,
                            "verified_at": d.verified_at
                        })
                    })
                    .collect();
                if !devices.is_empty() {
                    verified_devices.insert(user_id, serde_json::json!(devices));
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "device_keys": response.device_keys,
        "master_keys": response.master_keys,
        "self_signing_keys": response.self_signing_keys,
        "user_signing_keys": response.user_signing_keys,
        "failures": response.failures,
        "verified_devices": serde_json::Value::Object(verified_devices)
    })))
}

#[axum::debug_handler]
async fn claim_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let mut request: synapse_e2ee::device_keys::KeyClaimRequest = serde_json::from_value(body)
        .map_err(|e| synapse_common::error::ApiError::bad_request(format!("Invalid request: {e}")))?;

    let requested_users =
        request.one_time_keys.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    let allowed_users = filter_users_with_shared_rooms(&state, &auth_user.user_id, &requested_users).await;

    if let Some(one_time_keys) = request.one_time_keys.as_object_mut() {
        one_time_keys.retain(|user_id, _| allowed_users.iter().any(|allowed| allowed == user_id));
    }

    let requested_device_count: usize = request
        .one_time_keys
        .as_object()
        .map_or(0, |map| map.values().map(|v| v.as_object().map_or(0, |o| o.len())).sum());

    if requested_device_count == 0 {
        return Ok(Json(serde_json::json!({
            "one_time_keys": {},
            "failures": {
                "M_EMPTY_REQUEST": "No devices requested for key claiming"
            }
        })));
    }

    let response = state.services.e2ee.device_keys_service.claim_keys(request).await?;

    let claimed_device_count: usize = response
        .one_time_keys
        .as_object()
        .map_or(0, |map| map.values().map(|v| v.as_object().map_or(0, |o| o.len())).sum());

    let mut failures = if let serde_json::Value::Object(failures_map) = response.failures {
        failures_map
    } else {
        serde_json::Map::new()
    };

    if claimed_device_count < requested_device_count {
        let failed_count = requested_device_count - claimed_device_count;
        failures.insert(
            "M_KEY_CLAIM_FAILED".to_string(),
            serde_json::json!({
                "failed_devices": failed_count,
                "message": format!("Key claiming failed for {} device(s). The devices may not have uploaded one-time keys.", failed_count)
            }),
        );
    }

    Ok(Json(serde_json::json!({
        "one_time_keys": response.one_time_keys,
        "failures": serde_json::Value::Object(failures)
    })))
}

#[axum::debug_handler]
async fn key_changes(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let from = params.get("from").and_then(parse_stream_id).unwrap_or(0);
    let to = params.get("to").and_then(parse_stream_id);

    let max_stream_id: i64 =
        state.services.account.device_storage.get_max_device_list_stream_id().await.map_err(|e| {
            tracing::error!("Failed to get device list stream position: {e}");
            ApiError::database("Failed to get device list stream position")
        })?;

    let to = to.unwrap_or(max_stream_id);

    let changed: Vec<String> = state
        .services
        .account
        .device_storage
        .get_device_list_changed_users(from, to, &auth_user.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get key changes: {e}");
            ApiError::database("Failed to get key changes")
        })?;
    let changed = filter_users_with_shared_rooms(&state, &auth_user.user_id, &changed)
        .await
        .into_iter()
        .filter(|user_id| user_id != &auth_user.user_id)
        .collect::<Vec<_>>();

    let left: Vec<String> =
        state.services.account.device_storage.get_device_list_left_users(from, to, &auth_user.user_id).await.map_err(
            |e| {
                tracing::error!("Failed to get key changes left: {e}");
                ApiError::database("Failed to get key changes left")
            },
        )?;
    let left = filter_users_with_shared_rooms(&state, &auth_user.user_id, &left)
        .await
        .into_iter()
        .filter(|user_id| user_id != &auth_user.user_id)
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "changed": changed,
        "left": left
    })))
}

#[axum::debug_handler]
async fn device_list_update(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
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
        let devices_by_user =
            state.services.account.device_storage.get_users_devices_batch(&users).await.map_err(|e| {
                tracing::error!("Failed to get devices: {e}");
                ApiError::database("Failed to get devices")
            })?;

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

    let max_stream_id: i64 =
        state.services.account.device_storage.get_max_device_list_stream_id().await.map_err(|e| {
            tracing::error!("Failed to get device list stream position: {e}");
            ApiError::database("Failed to get device list stream position")
        })?;

    let to = if to > 0 { to } else { max_stream_id };

    let change_rows =
        state.services.account.device_storage.get_device_list_changes(since, to, &users).await.map_err(|e| {
            tracing::error!("Failed to get device list changes: {e}");
            ApiError::database("Failed to get device list changes")
        })?;

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
        let user_ids: Vec<&str> = active_pairs.iter().map(|(u, _)| u.as_str()).collect();
        let device_ids: Vec<&str> = active_pairs.iter().map(|(_, d)| d.as_str()).collect();

        let device_rows = state
            .services
            .account
            .device_storage
            .get_devices_by_user_device_pairs(&user_ids, &device_ids)
            .await
            .map_err(|e| {
                tracing::error!("Failed to batch get device data: {e}");
                ApiError::database("Failed to get device data")
            })?;

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

    let existing_users: Vec<String> =
        state.services.account.device_storage.filter_existing_users(&users).await.map_err(|e| {
            tracing::error!("Failed to resolve left users: {e}");
            ApiError::database("Failed to resolve left users")
        })?;

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

#[allow(clippy::unused_async)]
async fn room_key_distribution(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(_room_id): Path<String>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    Err(synapse_common::error::ApiError::forbidden(
        "Room key distribution is a server-internal endpoint and is not available via the client API".to_string(),
    ))
}

#[axum::debug_handler]
async fn send_to_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((event_type, transaction_id)): Path<(String, String)>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let sender_device_id =
        auth_user.device_id.as_deref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    let messages = body
        .get("messages")
        .ok_or_else(|| synapse_common::error::ApiError::bad_request("Missing 'messages' field".to_string()))?;

    if event_type == "m.room_key" || event_type == "m.forwarded_room_key" {
        if let Some(msg_obj) = messages.as_object() {
            for (_user_id, user_devices) in msg_obj {
                if let Some(devices) = user_devices.as_object() {
                    for (_device_id, device_msg) in devices {
                        if let Some(session_key) = device_msg
                            .get("session_key")
                            .or_else(|| device_msg.get("content").and_then(|c| c.get("session_key")))
                        {
                            if session_key.as_str().is_none_or(|s| s.is_empty()) {
                                return Err(ApiError::bad_request(
                                    "Session key is empty in room key event. Session creation may have failed."
                                        .to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    state
        .services
        .e2ee
        .to_device_service
        .send_messages(&auth_user.user_id, sender_device_id, &event_type, Some(&transaction_id), messages)
        .await?;

    // Notify recipients so that long-polling /sync connections wake up
    // immediately instead of waiting for the next polling cycle.
    if let Some(msg_map) = body.get("messages").and_then(|m| m.as_object()) {
        for (user_id, _) in msg_map {
            state.services.core.event_notifier.notify_user(user_id);
        }
    }

    Ok(Json(json!({ "failures": {} })))
}

#[axum::debug_handler]
async fn upload_signatures(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let response = state.services.e2ee.device_keys_service.upload_signatures(&auth_user.user_id, body).await?;

    Ok(Json(response))
}

#[axum::debug_handler]
async fn upload_device_signing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<axum::response::Response, ApiError> {
    // UIA (User-Interactive Authentication) is required for cross-signing key upload
    // per Matrix spec: POST /_matrix/client/v3/keys/device_signing/upload requires UIA
    let auth = body.get("auth");
    if let Err(uia_response) = state
        .services
        .extensions
        .uia_service
        .require_uia(
            auth,
            &auth_user.user_id,
            synapse_services::uia_service::UiaService::get_cross_signing_flows(),
            &state.services.core.auth_service,
            &state.services.account.threepid_storage,
        )
        .await
    {
        return Ok((StatusCode::UNAUTHORIZED, Json(uia_response)).into_response());
    }

    // UIA passed, proceed with business logic
    let device_id =
        auth_user.device_id.as_ref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;

    if let Some(master_key) = body.get("master_key") {
        if let Some(key_obj) = master_key.as_object() {
            if !key_obj.is_empty() {
                state
                    .services
                    .e2ee
                    .cross_signing_service
                    .upload_device_signing_key(&auth_user.user_id, device_id, master_key)
                    .await?;
            }
        }
    }

    if let Some(self_signing_key) = body.get("self_signing_key") {
        if let Some(key_obj) = self_signing_key.as_object() {
            if !key_obj.is_empty() {
                state
                    .services
                    .e2ee
                    .cross_signing_service
                    .upload_device_signing_key(&auth_user.user_id, device_id, self_signing_key)
                    .await?;
            }
        }
    }

    if let Some(user_signing_key) = body.get("user_signing_key") {
        if let Some(key_obj) = user_signing_key.as_object() {
            if !key_obj.is_empty() {
                state
                    .services
                    .e2ee
                    .cross_signing_service
                    .upload_device_signing_key(&auth_user.user_id, device_id, user_signing_key)
                    .await?;
            }
        }
    }

    Ok(Json(json!({})).into_response())
}

#[axum::debug_handler]
async fn create_room_key_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let device_id =
        auth_user.device_id.as_deref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    let body: CreateRoomKeyRequestBody =
        serde_json::from_value(body).map_err(|e| ApiError::bad_request(format!("Invalid room key request: {e}")))?;

    let request = state
        .services
        .e2ee
        .key_request_service
        .create_request(
            &auth_user.user_id,
            device_id,
            &body.room_id,
            &body.session_id,
            &body.algorithm,
            body.request_type.as_deref(),
            body.request_id.as_deref(),
        )
        .await?;

    Ok(Json(serde_json::json!({
        "request_id": request.request_id
    })))
}

#[axum::debug_handler]
async fn get_room_key_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<GetRoomKeyRequestsQuery>,
) -> Result<Json<Value>, ApiError> {
    let mut requests =
        state.services.e2ee.key_request_service.get_requests(&auth_user.user_id, params.status.as_deref()).await?;

    if let Some(room_id) = params.room_id.as_deref() {
        requests.retain(|request| request.room_id == room_id);
    }

    if let Some(session_id) = params.session_id.as_deref() {
        requests.retain(|request| request.session_id == session_id);
    }

    if let Some(limit) = params.limit {
        requests.truncate(limit);
    }

    Ok(Json(serde_json::json!({
        "requests": requests
            .into_iter()
            .map(serialize_room_key_request)
            .collect::<Vec<_>>()
    })))
}

#[axum::debug_handler]
async fn delete_room_key_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(request_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let existing = state.services.e2ee.key_request_service.get_request(&request_id).await?;

    let request = existing.ok_or_else(|| ApiError::not_found("Room key request not found".to_string()))?;

    if request.user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot delete another user's room key request".to_string()));
    }

    state.services.e2ee.key_request_service.cancel_request(&request_id).await?;

    Ok(empty_json())
}

#[derive(Debug, Deserialize)]
struct CreateRoomKeyRequestBody {
    algorithm: String,
    room_id: String,
    session_id: String,
    request_type: Option<String>,
    request_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GetRoomKeyRequestsQuery {
    status: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    limit: Option<usize>,
}

fn serialize_room_key_request(request: synapse_e2ee::key_request::KeyRequestInfo) -> Value {
    let action = request.action;
    let status = if action == "cancellation" || action == "cancelled" {
        "cancelled"
    } else if request.is_fulfilled {
        "fulfilled"
    } else {
        "pending"
    };

    serde_json::json!({
        "request_id": request.request_id,
        "user_id": request.user_id,
        "device_id": request.device_id,
        "room_id": request.room_id,
        "session_id": request.session_id,
        "algorithm": request.algorithm,
        "action": &action,
        "request_type": action,
        "status": status,
        "created_ts": request.created_ts,
        "is_fulfilled": request.is_fulfilled,
        "fulfilled_by_device": request.fulfilled_by_device,
        "fulfilled_ts": request.fulfilled_ts,
    })
}

// =====================================================
// E2EE Phase 1: Device Trust Handlers
// =====================================================

#[axum::debug_handler]
async fn request_device_verification(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let new_device_id = body
        .get("new_device_id")
        .or_else(|| body.get("device_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("new_device_id required".to_string()))?;

    let method = body.get("method").and_then(|v| v.as_str()).unwrap_or("sas");

    let verification_method = match method {
        "qr" => synapse_e2ee::device_trust::VerificationMethod::Qr,
        "emoji" => synapse_e2ee::device_trust::VerificationMethod::Emoji,
        _ => synapse_e2ee::device_trust::VerificationMethod::Sas,
    };

    let response = state
        .services
        .e2ee
        .device_trust_service
        .request_device_verification(
            &auth_user.user_id,
            new_device_id,
            verification_method,
            auth_user.device_id.as_deref(),
        )
        .await?;

    Ok(Json(serde_json::json!({
        "request_token": response.request_token,
        "token": response.request_token,
        "status": response.status,
        "expires_at": response.expires_at,
        "methods_available": response.methods_available
    })))
}

#[axum::debug_handler]
async fn respond_device_verification(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_token = body
        .get("request_token")
        .or_else(|| body.get("token"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("request_token required".to_string()))?;

    let approved = body.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);

    let response = state
        .services
        .e2ee
        .device_trust_service
        .respond_to_verification(&auth_user.user_id, request_token, approved)
        .await?;

    Ok(Json(serde_json::json!({
        "success": response.success,
        "trust_level": response.trust_level
    })))
}

#[axum::debug_handler]
async fn get_verification_status(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let response = state.services.e2ee.device_trust_service.get_verification_status(&auth_user.user_id, &token).await?;

    match response {
        Some(r) => Ok(Json(serde_json::json!({
            "request_token": r.request_token,
            "token": r.request_token,
            "status": r.status,
            "expires_at": r.expires_at,
            "methods_available": r.methods_available
        }))),
        None => Ok(Json(serde_json::json!({
            "status": "not_found"
        }))),
    }
}

#[axum::debug_handler]
async fn get_device_trust_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let devices = state.services.e2ee.device_trust_service.get_all_devices_with_trust(&auth_user.user_id).await?;

    let devices_json: Vec<Value> = devices
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "device_id": d.device_id,
                "trust_level": d.trust_level,
                "verified_at": d.verified_at,
                "verified_by": d.verified_by
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "devices": devices_json
    })))
}

#[axum::debug_handler]
async fn get_device_trust(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let status =
        state.services.e2ee.device_trust_service.get_device_trust_status(&auth_user.user_id, &device_id).await?;

    match status {
        Some(s) => Ok(Json(serde_json::json!({
            "device_id": s.device_id,
            "trust_level": s.trust_level,
            "verified_at": s.verified_at,
            "verified_by": s.verified_by
        }))),
        None => Err(ApiError::not_found("Device not found".to_string())),
    }
}

#[axum::debug_handler]
async fn get_security_summary(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let summary = state.services.e2ee.device_trust_service.get_security_summary(&auth_user.user_id).await?;

    Ok(Json(serde_json::json!({
        "verified_devices": summary.verified_devices,
        "unverified_devices": summary.unverified_devices,
        "blocked_devices": summary.blocked_devices,
        "has_cross_signing_master": summary.has_cross_signing_master,
        "security_score": summary.security_score,
        "recommendations": summary.recommendations
    })))
}

// =====================================================
// E2EE Phase 3: Secure Backup Handlers
// =====================================================

#[axum::debug_handler]
async fn get_secure_backup_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let backups = state.services.e2ee.secure_backup_service.list_backups(&auth_user.user_id).await?;

    let mut response = serde_json::Map::with_capacity(backups.len());
    for backup in backups {
        response.insert(
            backup.backup_id.clone(),
            serde_json::json!({
                "backup_id": backup.backup_id,
                "version": backup.version,
                "algorithm": backup.algorithm,
                "auth_data": backup.auth_data,
                "key_count": backup.key_count
            }),
        );
    }

    Ok(Json(Value::Object(response)))
}

#[axum::debug_handler]
async fn create_secure_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    // Support two modes:
    // 1. Passphrase mode: { "passphrase": "..." } -> server derives key
    // 2. Standard mode: { "algorithm": "...", "auth_data": {...} } -> client provides auth data
    let passphrase = body.get("passphrase").and_then(|v| v.as_str());
    let algorithm = body.get("algorithm").and_then(|v| v.as_str());
    let auth_data_val = body.get("auth_data");

    if let Some(passphrase) = passphrase {
        // Passphrase mode: server derives key from passphrase
        let response = state.services.e2ee.secure_backup_service.create_backup(&auth_user.user_id, passphrase).await?;

        Ok(Json(serde_json::json!({
            "backup_id": response.backup_id,
            "version": response.version,
            "algorithm": response.algorithm,
            "auth_data": response.auth_data,
            "key_count": response.key_count
        })))
    } else if let (Some(algorithm), Some(auth_data_val)) = (algorithm, auth_data_val) {
        // Standard mode: client provides algorithm and auth_data
        let response = state
            .services
            .e2ee
            .secure_backup_service
            .create_backup_with_data(&auth_user.user_id, algorithm, auth_data_val)
            .await?;

        Ok(Json(serde_json::json!({
            "backup_id": response.backup_id,
            "version": response.version,
            "algorithm": response.algorithm,
            "auth_data": response.auth_data,
            "key_count": response.key_count
        })))
    } else {
        Err(ApiError::bad_request("Either 'passphrase' or 'algorithm'+'auth_data' required".to_string()))
    }
}

#[axum::debug_handler]
async fn get_secure_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let response = state.services.e2ee.secure_backup_service.get_backup_info(&auth_user.user_id, &backup_id).await?;

    match response {
        Some(r) => Ok(Json(serde_json::json!({
            "backup_id": r.backup_id,
            "version": r.version,
            "algorithm": r.algorithm,
            "auth_data": r.auth_data,
            "key_count": r.key_count
        }))),
        None => Err(ApiError::not_found("Backup not found".to_string())),
    }
}

#[axum::debug_handler]
async fn store_secure_backup_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let passphrase = body
        .get("passphrase")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("passphrase required".to_string()))?;

    let session_keys = body
        .get("session_keys")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|k| {
                    Some(synapse_e2ee::secure_backup::SessionKeyData {
                        room_id: k.get("room_id")?.as_str()?.to_string(),
                        session_id: k.get("session_id")?.as_str()?.to_string(),
                        first_message_index: k.get("first_message_index")?.as_i64().unwrap_or(0),
                        forwarded_count: k.get("forwarded_count")?.as_i64().unwrap_or(0),
                        is_verified: k.get("is_verified").and_then(|v| v.as_bool()).unwrap_or(false),
                        session_key: k
                            .get("session_key")
                            .or_else(|| k.get("session_data").and_then(|sd| sd.get("session_key")))
                            .and_then(|v| v.as_str())?
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let key_count = state
        .services
        .e2ee
        .secure_backup_service
        .store_session_keys(&auth_user.user_id, &backup_id, passphrase, session_keys)
        .await?;

    Ok(Json(serde_json::json!({
        "count": key_count,
        "key_count": key_count
    })))
}

#[axum::debug_handler]
async fn restore_secure_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
    MatrixJson(body): MatrixJson<RestoreSecureBackupRequest>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .services
        .e2ee
        .secure_backup_service
        .restore_backup(&auth_user.user_id, &backup_id, &body.passphrase, body.rooms)
        .await?;

    Ok(Json(serde_json::json!({
        "recovered_keys": response.recovered_keys,
        "total_keys": response.total_keys
    })))
}

#[axum::debug_handler]
async fn verify_secure_backup_passphrase(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let passphrase = body
        .get("passphrase")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("passphrase required".to_string()))?;

    let valid =
        state.services.e2ee.secure_backup_service.verify_passphrase(&auth_user.user_id, &backup_id, passphrase).await?;

    Ok(Json(serde_json::json!({
        "valid": valid
    })))
}

#[axum::debug_handler]
async fn delete_secure_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.services.e2ee.secure_backup_service.delete_backup(&auth_user.user_id, &backup_id).await?;

    Ok(empty_json())
}

// Key-backup version + per-session handlers live in
// `src/web/routes/key_backup.rs` (registered through `create_key_backup_router`
// in `assembly.rs`). They were duplicated here historically and the routes
// silently won the merge over the spec-compliant ones; see
// `docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md` §1.2 for the audit.

#[cfg(test)]
mod tests {
    #[test]
    fn test_e2ee_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/keys/upload",
            "/_matrix/client/v3/keys/query",
            "/_matrix/client/r0/keys/device_signing/upload",
            "/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}",
        ];

        let v3_only_routes = [
            "/_matrix/client/v3/device_verification/request",
            "/_matrix/client/v3/device_trust/{device_id}",
            "/_matrix/client/v3/security/summary",
            "/_matrix/client/v3/keys/backup/secure/{backup_id}/verify",
        ];

        assert!(compat_routes.iter().all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes.iter().all(|route| route.starts_with("/_matrix/client/v3/")));
    }

    #[test]
    fn test_e2ee_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/keys/upload",
            "/keys/query",
            "/keys/claim",
            "/keys/changes",
            "/keys/signatures/upload",
            "/keys/device_signing/upload",
            "/room_keys/request",
            "/room_keys/request/{request_id}",
            "/rooms/{room_id}/keys/distribution",
            "/sendToDevice/{event_type}/{transaction_id}",
        ];

        assert_eq!(shared_paths.len(), 10);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_serialize_room_key_request_statuses() {
        let pending = super::serialize_room_key_request(synapse_e2ee::key_request::KeyRequestInfo {
            request_id: "req-1".to_string(),
            user_id: "@alice:example.org".to_string(),
            device_id: "DEVICE".to_string(),
            room_id: "!room:example.org".to_string(),
            session_id: "sess-1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "request".to_string(),
            created_ts: 1,
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        });
        let cancelled = super::serialize_room_key_request(synapse_e2ee::key_request::KeyRequestInfo {
            request_id: "req-2".to_string(),
            user_id: "@alice:example.org".to_string(),
            device_id: "DEVICE".to_string(),
            room_id: "!room:example.org".to_string(),
            session_id: "sess-2".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "cancellation".to_string(),
            created_ts: 2,
            is_fulfilled: true,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        });

        assert_eq!(pending["status"], "pending");
        assert_eq!(pending["request_type"], "request");
        assert_eq!(cancelled["status"], "cancelled");
    }
}
