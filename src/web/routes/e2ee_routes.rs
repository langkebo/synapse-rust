use super::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::Value;

pub fn create_e2ee_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/r0/keys/upload/{device_id}",
            post(upload_keys),
        )
        .route("/_matrix/client/r0/keys/query", post(query_keys))
        .route("/_matrix/client/r0/keys/claim", post(claim_keys))
        .route("/_matrix/client/v3/keys/changes", get(key_changes))
        .route(
            "/_matrix/client/r0/rooms/{room_id}/keys/distribution",
            get(room_key_distribution),
        )
        .route(
            "/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}",
            put(send_to_device),
        )
}

#[axum::debug_handler]
async fn upload_keys(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let request = crate::e2ee::device_keys::KeyUploadRequest {
        device_keys: if body.get("device_keys").is_some() {
            Some(crate::e2ee::device_keys::DeviceKeys {
                user_id: auth_user.user_id.clone(),
                device_id: device_id.clone(),
                algorithms: vec!["m.olm.v1.curve25519-aes-sha2".to_string()],
                keys: body["device_keys"]["keys"].clone(),
                signatures: body["device_keys"]["signatures"].clone(),
                unsigned: body["device_keys"]["unsigned"]
                    .as_object()
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
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
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
    Json(body): Json<Value>,
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
    _auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let from = params.get("from").and_then(|v| v.as_str()).unwrap_or("0");
    let to = params.get("to").and_then(|v| v.as_str()).unwrap_or("");

    let (changed, left) = state
        .services
        .device_keys_service
        .get_key_changes(from, to)
        .await?;

    Ok(Json(serde_json::json!({
        "changed": changed,
        "left": left
    })))
}

#[axum::debug_handler]
async fn room_key_distribution(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
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
    Path((_event_type, transaction_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let messages = body.get("messages").ok_or_else(|| {
        crate::error::ApiError::bad_request("Missing 'messages' field".to_string())
    })?;

    state
        .services
        .to_device_service
        .send_messages(&auth_user.user_id, messages)
        .await?;

    Ok(Json(serde_json::json!({
        "txn_id": transaction_id
    })))
}
