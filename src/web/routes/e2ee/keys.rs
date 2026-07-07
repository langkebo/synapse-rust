use crate::web::routes::context::DeviceContext;
use crate::web::routes::response_helpers::filter_users_with_shared_rooms;
use crate::web::routes::{AppState, AuthenticatedUser, MatrixJson};
use crate::ApiError;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::Value;

use super::backup::*;
use super::devices::*;

pub(crate) fn parse_stream_id(s: &str) -> Option<i64> {
    s.strip_prefix('s')?.parse::<i64>().ok().filter(|&n| n >= 0)
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
        .route("/keys/history", get(get_key_history))
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
pub fn e2ee_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    let mut out = crate::web::routes::route_ledger::expand_under_prefixes(
        "e2ee",
        E2EE_COMPAT_NEST_PREFIXES,
        &e2ee_compat_relative_routes(),
    );
    out.extend(crate::web::routes::route_ledger::expand_under_prefixes(
        "e2ee",
        E2EE_V3_ONLY_NEST_PREFIXES,
        &e2ee_v3_only_relative_routes(),
    ));
    out
}

#[axum::debug_handler]
async fn upload_keys(
    State(ctx): State<DeviceContext>,
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

    let request = crate::e2ee::device_keys::KeyUploadRequest {
        device_keys: if has_device_keys || has_one_time_keys {
            Some(crate::e2ee::device_keys::DeviceKeys {
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

    let response = ctx.device_keys_service.upload_keys(request, &auth_user.user_id, &device_id).await?;

    Ok(Json(serde_json::json!({
        "one_time_key_counts": response.one_time_key_counts
    })))
}

#[axum::debug_handler]
async fn query_keys(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: crate::e2ee::device_keys::KeyQueryRequest = serde_json::from_value(body.clone())
        .map_err(|e| crate::error::ApiError::bad_request(format!("Invalid request: {e}")))?;

    let device_keys_raw = body.get("device_keys").cloned().unwrap_or(serde_json::json!({}));

    let requested_users: Vec<String> =
        device_keys_raw.as_object().map(|map| map.keys().cloned().collect()).unwrap_or_default();

    let allowed_users = filter_users_with_shared_rooms(&ctx.room_service, &auth_user.user_id, &requested_users).await;

    let device_keys = if requested_users.is_empty() {
        let mut shared = ctx
            .room_service
            .membership
            .get_shared_room_users(&auth_user.user_id)
            .await
            .map_err(|e| crate::error::ApiError::internal_with_log("Failed to load shared room users", &e))?;
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

    let response = ctx.device_keys_service.query_keys(request).await?;

    // Outbound federation: for remote users (whose server_name differs from
    // ours), query their home server via `FederationClient::query_keys` and
    // merge the results.  This is essential for cross-server E2EE — without
    // it, local users cannot obtain device keys for remote users and cannot
    // establish Olm sessions with them.
    //
    // Reference: element-hq/synapse `synapse/handlers/e2e_keys.py::E2eKeysHandler.query_devices`
    let local_server = &ctx.server_name;
    let mut merged_device_keys = response.device_keys.clone();
    let mut merged_master_keys = response.master_keys.clone();
    let mut merged_self_signing_keys = response.self_signing_keys.clone();
    let mut merged_user_signing_keys = response.user_signing_keys.clone();
    let mut merged_failures = response.failures.clone();

    // Collect remote users that have no local device keys (i.e. the local
    // DB didn't have them — we need to fetch from their home server).
    let remote_users: Vec<&str> = requested_users
        .iter()
        .filter(|uid| uid.rsplit_once(':').is_some_and(|(_, server)| server != local_server.as_str()))
        .map(String::as_str)
        .collect();

    if !remote_users.is_empty() {
        // Group remote users by their home server.
        let mut by_server: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
        for uid in &remote_users {
            if let Some((_, server)) = uid.rsplit_once(':') {
                by_server.entry(server).or_default().push(uid);
            }
        }

        // Query each remote server in parallel.
        let federation_client = &ctx.federation_client;
        let mut tasks = Vec::new();
        for (server, user_ids) in by_server {
            let mut device_keys_query = serde_json::Map::new();
            for uid in &user_ids {
                // Request all devices for each user (empty list = all).
                device_keys_query.insert((*uid).to_string(), serde_json::json!([]));
            }
            let query = serde_json::json!({ "device_keys": device_keys_query });
            let server_owned = server.to_string();
            tasks.push(tokio::spawn({
                let server_owned = server_owned.clone();
                let fc = federation_client.clone();
                async move {
                    let result = fc.query_keys(&server_owned, &query).await;
                    (server_owned, result)
                }
            }));
        }

        for task in tasks {
            match task.await {
                Ok((_server, Ok(remote_response))) => {
                    // Merge device_keys.
                    if let Some(remote_dk) = remote_response.get("device_keys").and_then(|v| v.as_object()) {
                        for (uid, devices) in remote_dk {
                            merged_device_keys[uid] = devices.clone();
                        }
                    }
                    // Merge master_keys, self_signing_keys, user_signing_keys.
                    if let Some(remote_mk) = remote_response.get("master_keys").and_then(|v| v.as_object()) {
                        for (uid, key) in remote_mk {
                            merged_master_keys[uid] = key.clone();
                        }
                    }
                    if let Some(remote_ssk) = remote_response.get("self_signing_keys").and_then(|v| v.as_object()) {
                        for (uid, key) in remote_ssk {
                            merged_self_signing_keys[uid] = key.clone();
                        }
                    }
                    if let Some(remote_usk) = remote_response.get("user_signing_keys").and_then(|v| v.as_object()) {
                        for (uid, key) in remote_usk {
                            merged_user_signing_keys[uid] = key.clone();
                        }
                    }
                    // Merge failures.
                    if let Some(remote_failures) = remote_response.get("failures").and_then(|v| v.as_object()) {
                        for (k, v) in remote_failures {
                            merged_failures[k] = v.clone();
                        }
                    }
                }
                Ok((server, Err(error))) => {
                    ::tracing::info!(
                        server = %server,
                        error = %error,
                        "Federation query_keys failed for remote server"
                    );
                    merged_failures[&server] = serde_json::json!(format!("Failed to query keys: {}", error));
                }
                Err(error) => {
                    ::tracing::warn!(error = %error, "Federation query_keys task panicked");
                }
            }
        }
    }

    let mut verified_devices = serde_json::Map::new();
    if let Some(device_keys_obj) = merged_device_keys.as_object() {
        let user_ids: Vec<String> = device_keys_obj.keys().cloned().collect();
        if !user_ids.is_empty() {
            let batch_result =
                ctx.cross_signing_service.get_verified_devices_batch(&user_ids).await.map_err(|e| {
                    crate::error::ApiError::internal_with_log("Failed to load verified devices batch", &e)
                })?;

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
        "device_keys": merged_device_keys,
        "master_keys": merged_master_keys,
        "self_signing_keys": merged_self_signing_keys,
        "user_signing_keys": merged_user_signing_keys,
        "failures": merged_failures,
        "verified_devices": serde_json::Value::Object(verified_devices)
    })))
}

#[axum::debug_handler]
async fn claim_keys(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let mut request: crate::e2ee::device_keys::KeyClaimRequest = serde_json::from_value(body)
        .map_err(|e| crate::error::ApiError::bad_request(format!("Invalid request: {e}")))?;

    let requested_users =
        request.one_time_keys.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    let allowed_users = filter_users_with_shared_rooms(&ctx.room_service, &auth_user.user_id, &requested_users).await;

    // Clone the original request before it's consumed, so we can identify
    // remote users with unclaimed devices after the local claim.
    let original_one_time_keys = request.one_time_keys.clone();

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

    let response = ctx.device_keys_service.claim_keys(request).await?;

    let mut merged_one_time_keys = response.one_time_keys.clone();
    let mut failures = if let serde_json::Value::Object(failures_map) = response.failures.clone() {
        failures_map
    } else {
        serde_json::Map::new()
    };

    // Outbound federation: for remote users whose one-time keys were not
    // found locally, claim directly from their home server via
    // `FederationClient::claim_keys`.  This is essential for establishing
    // new Olm sessions with remote users.
    //
    // Reference: element-hq/synapse `synapse/handlers/e2e_keys.py::E2eKeysHandler.claim_one_time_keys`
    let local_server = &ctx.server_name;

    // Build per-server claim requests for remote users with unclaimed devices.
    let mut remote_claims_by_server: std::collections::HashMap<String, serde_json::Map<String, Value>> =
        std::collections::HashMap::new();
    if let Some(orig) = original_one_time_keys.as_object() {
        for (uid, devices) in orig {
            let is_remote = uid.rsplit_once(':').is_some_and(|(_, server)| server != local_server.as_str());
            if !is_remote || !allowed_users.contains(uid) {
                continue;
            }
            if let Some(orig_devs) = devices.as_object() {
                let mut user_claims = serde_json::Map::new();
                for (device_id, algorithm) in orig_devs {
                    let locally_claimed =
                        merged_one_time_keys.get(uid).and_then(|v| v.get(device_id)).is_some_and(|v| !v.is_null());
                    if !locally_claimed {
                        user_claims.insert(device_id.clone(), algorithm.clone());
                    }
                }
                if !user_claims.is_empty() {
                    if let Some((_, server)) = uid.rsplit_once(':') {
                        remote_claims_by_server
                            .entry(server.to_string())
                            .or_default()
                            .insert(uid.clone(), serde_json::Value::Object(user_claims));
                    }
                }
            }
        }
    }

    if !remote_claims_by_server.is_empty() {
        let federation_client = &ctx.federation_client;
        let mut tasks = Vec::new();
        for (server, server_claims) in remote_claims_by_server {
            let claim_request = serde_json::json!({ "one_time_keys": server_claims });
            let fc = federation_client.clone();
            let server_owned = server.clone();
            tasks.push(tokio::spawn(async move {
                let result = fc.claim_keys(&server, &claim_request).await;
                (server_owned, result)
            }));
        }

        for task in tasks {
            match task.await {
                Ok((_server, Ok(remote_response))) => {
                    if let Some(remote_otk) = remote_response.get("one_time_keys").and_then(|v| v.as_object()) {
                        for (uid, devices) in remote_otk {
                            if let Some(dev_map) = devices.as_object() {
                                if let Some(local_devices) =
                                    merged_one_time_keys.get_mut(uid).and_then(|v| v.as_object_mut())
                                {
                                    for (device_id, key) in dev_map {
                                        if local_devices.get(device_id).is_none_or(|v| v.is_null()) {
                                            local_devices[device_id] = key.clone();
                                        }
                                    }
                                } else {
                                    merged_one_time_keys[uid] = devices.clone();
                                }
                            }
                        }
                    }
                    if let Some(remote_failures) = remote_response.get("failures").and_then(|v| v.as_object()) {
                        for (k, v) in remote_failures {
                            failures[k] = v.clone();
                        }
                    }
                }
                Ok((server, Err(error))) => {
                    ::tracing::info!(
                        server = %server,
                        error = %error,
                        "Federation claim_keys failed for remote server"
                    );
                    failures[&server] = serde_json::json!(format!("Failed to claim keys: {}", error));
                }
                Err(error) => {
                    ::tracing::warn!(error = %error, "Federation claim_keys task panicked");
                }
            }
        }
    }

    let claimed_device_count: usize = merged_one_time_keys.as_object().map_or(0, |map| {
        map.values().map(|v| v.as_object().map_or(0, |o| o.values().filter(|v| !v.is_null()).count())).sum()
    });

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
        "one_time_keys": merged_one_time_keys,
        "failures": serde_json::Value::Object(failures)
    })))
}

#[axum::debug_handler]
async fn key_changes(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let from = params.get("from").and_then(|v| v.as_str()).and_then(parse_stream_id).unwrap_or(0);
    let to = params.get("to").and_then(|v| v.as_str()).and_then(parse_stream_id);

    let max_stream_id = ctx.account_device_list_service.get_max_stream_id().await?;

    let to = to.unwrap_or(max_stream_id);

    let changed: Vec<String> =
        ctx.account_device_list_service.get_changed_user_ids(from, to, &auth_user.user_id).await?;
    let changed = filter_users_with_shared_rooms(&ctx.room_service, &auth_user.user_id, &changed)
        .await
        .into_iter()
        .filter(|user_id| user_id != &auth_user.user_id)
        .collect::<Vec<_>>();

    let left = ctx.account_device_list_service.get_left_user_ids(from, to, &auth_user.user_id).await?;
    let left = filter_users_with_shared_rooms(&ctx.room_service, &auth_user.user_id, &left)
        .await
        .into_iter()
        .filter(|user_id| user_id != &auth_user.user_id)
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "changed": changed,
        "left": left
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_id_with_s_prefix() {
        assert_eq!(parse_stream_id("s12345"), Some(12345));
    }

    #[test]
    fn test_parse_stream_id_zero() {
        assert_eq!(parse_stream_id("s0"), Some(0));
    }

    #[test]
    fn test_parse_stream_id_max_i64() {
        assert_eq!(parse_stream_id("s9223372036854775807"), Some(9223372036854775807));
    }

    #[test]
    fn test_parse_stream_id_empty_string() {
        assert_eq!(parse_stream_id(""), None);
    }

    #[test]
    fn test_parse_stream_id_no_s_prefix() {
        assert_eq!(parse_stream_id("12345"), None);
    }

    #[test]
    fn test_parse_stream_id_non_numeric() {
        assert_eq!(parse_stream_id("sabc"), None);
    }

    #[test]
    fn test_parse_stream_id_negative() {
        assert_eq!(parse_stream_id("s-1"), None);
    }

    #[test]
    fn test_parse_stream_id_overflow() {
        assert_eq!(parse_stream_id("s9223372036854775808"), None);
    }
}
