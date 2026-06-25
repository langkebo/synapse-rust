use crate::web::routes::{AppState, AuthenticatedUser, MatrixJson};
use super::keys::parse_stream_id;
use crate::web::routes::response_helpers::{empty_json, filter_users_with_shared_rooms};
use crate::ApiError;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{extract::{Path, Query, State}, Json};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Deserialize;
use serde_json::{json, Value};

#[axum::debug_handler]
pub(crate) async fn device_list_update(
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

#[allow(clippy::unused_async)]
pub(crate) async fn room_key_distribution(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(_room_id): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    Err(crate::error::ApiError::forbidden(
        "Room key distribution is a server-internal endpoint and is not available via the client API".to_string(),
    ))
}

#[axum::debug_handler]
pub(crate) async fn send_to_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((event_type, transaction_id)): Path<(String, String)>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let sender_device_id =
        auth_user.device_id.as_deref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    let messages = body
        .get("messages")
        .ok_or_else(|| crate::error::ApiError::bad_request("Missing 'messages' field".to_string()))?;

    // Enforce to-device message limits to prevent oversized payloads from
    // blocking the to-device queue and federation transaction dispatch.
    // Inspired by Synapse v1.155 (#19617) which limits to-device EDU size.
    const MAX_TO_DEVICE_RECIPIENTS: usize = 5000; // per request, across all users+devices
    const MAX_TO_DEVICE_PAYLOAD_BYTES: usize = 64 * 1024; // 64 KiB per request body
    let mut recipient_count: usize = 0;
    if let Some(msg_obj) = messages.as_object() {
        for (_user_id, user_devices) in msg_obj {
            if let Some(devices) = user_devices.as_object() {
                recipient_count += devices.len();
                if recipient_count > MAX_TO_DEVICE_RECIPIENTS {
                    return Err(ApiError::bad_request(format!(
                        "Too many to-device recipients: {recipient_count} exceeds limit of {MAX_TO_DEVICE_RECIPIENTS}"
                    )));
                }
                for (_device_id, device_msg) in devices {
                    // Reject individual messages larger than 64 KiB to protect
                    // downstream storage and federation queues.
                    if serde_json::to_string(device_msg).map(|s| s.len()).unwrap_or(0) > MAX_TO_DEVICE_PAYLOAD_BYTES {
                        return Err(ApiError::bad_request(format!(
                            "Individual to-device message exceeds {MAX_TO_DEVICE_PAYLOAD_BYTES} byte limit"
                        )));
                    }
                }
            }
        }
    }

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
pub(crate) async fn upload_signatures(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let response = state.services.e2ee.device_keys_service.upload_signatures(&auth_user.user_id, body).await?;

    Ok(Json(response))
}

#[axum::debug_handler]
pub(crate) async fn upload_device_signing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<axum::response::Response, ApiError> {
    // UIA (User-Interactive Authentication) is required for cross-signing key upload
    // per Matrix spec: POST /_matrix/client/v3/keys/device_signing/upload requires UIA
    let auth = body.get("auth");
    if let Err(uia_response) = state
        .services
        .account
        .account_identity_service
        .require_cross_signing_uia(
            &state.services.extensions.uia_service,
            auth,
            &auth_user.user_id,
            &state.services.core.auth_service,
        )
        .await
    {
        return Ok((StatusCode::UNAUTHORIZED, Json(uia_response)).into_response());
    }

    // UIA passed, proceed with business logic
    let device_id =
        auth_user.device_id.as_ref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    if !has_upload_device_signing_keys(&body) {
        return Err(ApiError::bad_request(
            "At least one of master_key, self_signing_key, or user_signing_key is required".to_string(),
        ));
    }

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

    // Wake long-polling sync/sliding-sync requests so the client sees the
    // updated cross-signing state immediately after a successful upload.
    state.services.core.event_notifier.notify_user(&auth_user.user_id);

    Ok(Json(json!({})).into_response())
}

pub(crate) fn has_upload_device_signing_keys(body: &Value) -> bool {
    ["master_key", "self_signing_key", "user_signing_key"]
        .iter()
        .any(|field| body.get(*field).and_then(Value::as_object).is_some_and(|key_obj| !key_obj.is_empty()))
}

#[axum::debug_handler]
pub(crate) async fn create_room_key_request(
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

pub(crate) fn encode_key_request_cursor(ts: i64, id: &str) -> String {
    BASE64.encode(format!("{}:{}", ts, id))
}

pub(crate) fn decode_key_request_cursor(cursor: &str) -> Option<(i64, String)> {
    let decoded = BASE64.decode(cursor).ok()?;
    let s = String::from_utf8(decoded).ok()?;
    let mut parts = s.splitn(2, ':');
    let ts = parts.next()?.parse().ok()?;
    let id = parts.next()?.to_string();
    Some((ts, id))
}

pub(crate) async fn get_room_key_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<GetRoomKeyRequestsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let cursor = params.from.as_deref().and_then(decode_key_request_cursor);

    let requests = state
        .services
        .e2ee
        .key_request_service
        .get_requests_paginated(crate::e2ee::key_request::KeyRequestPagination {
            user_id: &auth_user.user_id,
            limit: limit as i64,
            from_ts: cursor.as_ref().map(|c| c.0),
            from_id: cursor.as_ref().map(|c| c.1.as_str()),
            status: params.status.as_deref(),
            room_id: params.room_id.as_deref(),
            session_id: params.session_id.as_deref(),
        })
        .await?;

    let next_batch = if requests.len() == limit {
        requests.last().map(|r| encode_key_request_cursor(r.created_ts, &r.request_id))
    } else {
        None
    };

    Ok(Json(serde_json::json!({
        "requests": requests
            .into_iter()
            .map(serialize_room_key_request)
            .collect::<Vec<_>>(),
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
pub(crate) async fn delete_room_key_request(
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
pub(crate) struct GetRoomKeyRequestsQuery {
    status: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    limit: Option<usize>,
    from: Option<String>,
}

pub(crate) fn serialize_room_key_request(request: crate::e2ee::key_request::KeyRequestInfo) -> Value {
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
pub(crate) async fn request_device_verification(
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
pub(crate) async fn respond_device_verification(
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
pub(crate) async fn get_verification_status(
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
pub(crate) async fn get_device_trust_list(
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
pub(crate) async fn get_device_trust(
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
pub(crate) async fn get_security_summary(
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
