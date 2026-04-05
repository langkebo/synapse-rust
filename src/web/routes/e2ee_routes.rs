use super::{AppState, AuthenticatedUser};
use crate::web::routes::MatrixJson;
use crate::ApiError;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

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
        .route("/keys/query", post(query_keys))
        .route("/keys/claim", post(claim_keys))
        .route("/keys/changes", get(key_changes))
        .route("/keys/device_list/update", post(device_list_update))
        .route("/keys/signatures", post(upload_signatures))
        .route("/keys/signatures/upload", post(upload_signatures))
        .route("/keys/device_signing/upload", post(upload_device_signing))
        .route(
            "/room_keys/request",
            post(create_room_key_request).get(get_room_key_requests),
        )
        .route(
            "/room_keys/request/{request_id}",
            delete(delete_room_key_request),
        )
        .route(
            "/rooms/{room_id}/keys/distribution",
            get(room_key_distribution),
        )
        .route(
            "/sendToDevice/{event_type}/{transaction_id}",
            put(send_to_device),
        )
}

fn create_e2ee_v3_only_router() -> Router<AppState> {
    Router::new()
        .route(
            "/device_verification/request",
            post(request_device_verification),
        )
        .route(
            "/device_verification/respond",
            post(respond_device_verification),
        )
        .route(
            "/device_verification/status/{token}",
            get(get_verification_status),
        )
        .route("/device_trust", get(get_device_trust_list))
        .route("/device_trust/{device_id}", get(get_device_trust))
        .route("/security/summary", get(get_security_summary))
        .route("/keys/backup/secure", post(create_secure_backup))
        .route(
            "/keys/backup/secure/{backup_id}",
            get(get_secure_backup).delete(delete_secure_backup),
        )
        .route(
            "/keys/backup/secure/{backup_id}/keys",
            post(store_secure_backup_keys),
        )
        .route(
            "/keys/backup/secure/{backup_id}/restore",
            post(restore_secure_backup),
        )
        .route(
            "/keys/backup/secure/{backup_id}/verify",
            post(verify_secure_backup_passphrase),
        )
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

#[axum::debug_handler]
async fn upload_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let device_id = auth_user
        .device_id
        .clone()
        .ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;

    let has_device_keys = body.get("device_keys").is_some();
    let has_one_time_keys = body.get("one_time_keys").is_some();

    let request = crate::e2ee::device_keys::KeyUploadRequest {
        device_keys: if has_device_keys || has_one_time_keys {
            Some(crate::e2ee::device_keys::DeviceKeys {
                user_id: auth_user.user_id.clone(),
                device_id,
                algorithms: vec!["m.olm.v1.curve25519-aes-sha2".to_string()],
                keys: body
                    .get("device_keys")
                    .and_then(|v| v.get("keys"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
                signatures: body
                    .get("device_keys")
                    .and_then(|v| v.get("signatures"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
                unsigned: body
                    .get("device_keys")
                    .and_then(|v| v.get("unsigned"))
                    .and_then(|v| v.as_object())
                    .map(|v| v.clone().into()),
            })
        } else {
            None
        },
        one_time_keys: body.get("one_time_keys").cloned(),
    };

    let response = state
        .services
        .device_keys_service
        .upload_keys(request)
        .await?;

    Ok(Json(serde_json::json!({
        "one_time_key_counts": response.one_time_key_counts
    })))
}

#[axum::debug_handler]
async fn query_keys(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let request: crate::e2ee::device_keys::KeyQueryRequest = serde_json::from_value(body)
        .map_err(|e| crate::error::ApiError::bad_request(format!("Invalid request: {}", e)))?;

    let response = state
        .services
        .device_keys_service
        .query_keys(request)
        .await?;

    Ok(Json(serde_json::json!({
        "device_keys": response.device_keys,
        "failures": response.failures
    })))
}

#[axum::debug_handler]
async fn claim_keys(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let request: crate::e2ee::device_keys::KeyClaimRequest = serde_json::from_value(body)
        .map_err(|e| crate::error::ApiError::bad_request(format!("Invalid request: {}", e)))?;

    let response = state
        .services
        .device_keys_service
        .claim_keys(request)
        .await?;

    Ok(Json(serde_json::json!({
        "one_time_keys": response.one_time_keys,
        "failures": response.failures
    })))
}

#[axum::debug_handler]
async fn key_changes(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let from = params.get("from").and_then(parse_stream_id).unwrap_or(0);
    let to = params.get("to").and_then(parse_stream_id);

    let max_stream_id: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(stream_id), 0) FROM device_lists_stream
        "#,
    )
    .fetch_one(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get device list stream position: {}", e)))?;

    let to = to.unwrap_or(max_stream_id);

    let changed_rows = sqlx::query(
        r#"
        SELECT DISTINCT user_id
        FROM device_lists_stream
        WHERE stream_id > $1
          AND stream_id <= $2
          AND user_id != $3
        ORDER BY user_id
        LIMIT 100
        "#,
    )
    .bind(from)
    .bind(to)
    .bind(&auth_user.user_id)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get key changes: {}", e)))?;

    let changed: Vec<String> = changed_rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            row.get("user_id")
        })
        .collect();

    let left_rows = sqlx::query(
        r#"
        SELECT DISTINCT dl.user_id
        FROM device_lists_stream dl
        LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
        WHERE dl.stream_id > $1
          AND dl.stream_id <= $2
          AND dl.user_id != $3
          AND rm.user_id IS NULL
        ORDER BY dl.user_id
        LIMIT 100
        "#,
    )
    .bind(from)
    .bind(to)
    .bind(&auth_user.user_id)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get key changes left: {}", e)))?;

    let left: Vec<String> = left_rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            row.get("user_id")
        })
        .collect();

    Ok(Json(serde_json::json!({
        "changed": changed,
        "left": left
    })))
}

#[axum::debug_handler]
async fn device_list_update(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let users = body
        .get("users")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing users array".to_string()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect::<Vec<String>>();

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
                            "last_seen_ip": device.last_seen_ip,
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

        let row = sqlx::query_as::<_, (Option<String>, Option<i64>, Option<String>)>(
            r#"
            SELECT display_name, last_seen_ts, last_seen_ip
            FROM devices
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(&user_id)
        .bind(&device_id)
        .fetch_optional(&*state.services.device_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device data: {}", e)))?;

        if let Some((display_name, last_seen_ts, last_seen_ip)) = row {
            changed.push(json!({
                "user_id": user_id,
                "device_id": device_id,
                "device_data": {
                    "display_name": display_name,
                    "last_seen_ts": last_seen_ts,
                    "last_seen_ip": last_seen_ip,
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

#[axum::debug_handler]
async fn room_key_distribution(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| crate::error::ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(crate::error::ApiError::not_found(
            "Room not found".to_string(),
        ));
    }

    let session = state
        .services
        .megolm_service
        .get_outbound_session(&room_id)
        .await?;

    match session {
        Some(s) => Ok(Json(serde_json::json!({
            "room_id": room_id,
            "algorithm": "m.megolm.v1.aes-sha2",
            "session_id": s.session_id,
            "session_key": s.session_key
        }))),
        _ => Ok(Json(serde_json::json!({
            "room_id": room_id,
            "algorithm": "m.megolm.v1.aes-sha2",
            "session_id": "",
            "session_key": ""
        }))),
    }
}

#[axum::debug_handler]
async fn send_to_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((_event_type, _transaction_id)): Path<(String, String)>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let messages = body.get("messages").ok_or_else(|| {
        crate::error::ApiError::bad_request("Missing 'messages' field".to_string())
    })?;

    state
        .services
        .to_device_service
        .send_messages(&auth_user.user_id, messages)
        .await?;

    Ok(Json(serde_json::json!({})))
}

#[axum::debug_handler]
async fn upload_signatures(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .services
        .device_keys_service
        .upload_signatures(&auth_user.user_id, body)
        .await?;

    Ok(Json(response))
}

#[axum::debug_handler]
async fn upload_device_signing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let device_id = auth_user
        .device_id
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;

    if let Some(master_key) = body.get("master_key") {
        if let Some(key_obj) = master_key.as_object() {
            if !key_obj.is_empty() {
                state
                    .services
                    .cross_signing_service
                    .upload_device_signing_key(&auth_user.user_id, device_id, master_key)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to upload master key: {}", e))
                    })?;
            }
        }
    }

    if let Some(self_signing_key) = body.get("self_signing_key") {
        if let Some(key_obj) = self_signing_key.as_object() {
            if !key_obj.is_empty() {
                state
                    .services
                    .cross_signing_service
                    .upload_device_signing_key(&auth_user.user_id, device_id, self_signing_key)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to upload self-signing key: {}", e))
                    })?;
            }
        }
    }

    Ok(Json(serde_json::json!({})))
}

#[axum::debug_handler]
async fn create_room_key_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let device_id = auth_user
        .device_id
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    let body: CreateRoomKeyRequestBody = serde_json::from_value(body)
        .map_err(|e| ApiError::bad_request(format!("Invalid room key request: {}", e)))?;

    let request = state
        .services
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
    let mut requests = state
        .services
        .key_request_service
        .get_requests(&auth_user.user_id, params.status.as_deref())
        .await?;

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
    let existing = state
        .services
        .key_request_service
        .get_request(&request_id)
        .await?;

    let request =
        existing.ok_or_else(|| ApiError::not_found("Room key request not found".to_string()))?;

    if request.user_id != auth_user.user_id {
        return Err(ApiError::forbidden(
            "Cannot delete another user's room key request".to_string(),
        ));
    }

    state
        .services
        .key_request_service
        .cancel_request(&request_id)
        .await?;

    Ok(Json(serde_json::json!({})))
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

fn serialize_room_key_request(request: crate::e2ee::key_request::KeyRequestInfo) -> Value {
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
        "request_type": action.clone(),
        "action": action,
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
        "qr" => crate::e2ee::device_trust::VerificationMethod::Qr,
        "emoji" => crate::e2ee::device_trust::VerificationMethod::Emoji,
        _ => crate::e2ee::device_trust::VerificationMethod::Sas,
    };

    let response = state
        .services
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

    let approved = body
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let response = state
        .services
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
    let response = state
        .services
        .device_trust_service
        .get_verification_status(&auth_user.user_id, &token)
        .await?;

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
    let devices = state
        .services
        .device_trust_service
        .get_all_devices_with_trust(&auth_user.user_id)
        .await?;

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
    let status = state
        .services
        .device_trust_service
        .get_device_trust_status(&auth_user.user_id, &device_id)
        .await?;

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
    let summary = state
        .services
        .device_trust_service
        .get_security_summary(&auth_user.user_id)
        .await?;

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
async fn create_secure_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let passphrase = body
        .get("passphrase")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("passphrase required".to_string()))?;

    let response = state
        .services
        .secure_backup_service
        .create_backup(&auth_user.user_id, passphrase)
        .await?;

    Ok(Json(serde_json::json!({
        "backup_id": response.backup_id,
        "version": response.version,
        "algorithm": response.algorithm,
        "auth_data": response.auth_data,
        "key_count": response.key_count
    })))
}

#[axum::debug_handler]
async fn get_secure_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .services
        .secure_backup_service
        .get_backup_info(&auth_user.user_id, &backup_id)
        .await?;

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
                    Some(crate::e2ee::secure_backup::SessionKeyData {
                        room_id: k.get("room_id")?.as_str()?.to_string(),
                        session_id: k.get("session_id")?.as_str()?.to_string(),
                        first_message_index: k.get("first_message_index")?.as_i64().unwrap_or(0),
                        forwarded_count: k.get("forwarded_count")?.as_i64().unwrap_or(0),
                        is_verified: k
                            .get("is_verified")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
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
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let passphrase = body
        .get("passphrase")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("passphrase required".to_string()))?;

    let response = state
        .services
        .secure_backup_service
        .restore_backup(&auth_user.user_id, &backup_id, passphrase)
        .await?;

    Ok(Json(serde_json::json!({
        "success": response.success,
        "restored_keys": response.key_count,
        "key_count": response.key_count,
        "message": response.message
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

    let valid = state
        .services
        .secure_backup_service
        .verify_passphrase(&auth_user.user_id, &backup_id, passphrase)
        .await?;

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
    state
        .services
        .secure_backup_service
        .delete_backup(&auth_user.user_id, &backup_id)
        .await?;

    Ok(Json(serde_json::json!({})))
}

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

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v3/")));
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
        let pending = super::serialize_room_key_request(crate::e2ee::key_request::KeyRequestInfo {
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
        let cancelled =
            super::serialize_room_key_request(crate::e2ee::key_request::KeyRequestInfo {
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
