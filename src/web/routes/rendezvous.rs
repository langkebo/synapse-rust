use crate::common::ApiError;
use crate::storage::rendezvous::*;
use crate::web::routes::{AppState, OptionalAuthenticatedUser};
use axum::{
    extract::{Json, Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Router,
};
use serde_json::{json, Value};

const RENDEZVOUS_KEY_HEADER: &str = "x-matrix-rendezvous-key";

pub fn create_rendezvous_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/rendezvous", post(create_session))
        .route(
            "/_matrix/client/v1/rendezvous/{session_id}",
            get(get_session),
        )
        .route(
            "/_matrix/client/v1/rendezvous/{session_id}",
            put(update_session),
        )
        .route(
            "/_matrix/client/v1/rendezvous/{session_id}",
            delete(delete_session),
        )
        .route(
            "/_matrix/client/v1/rendezvous/{session_id}/messages",
            post(send_message),
        )
        .route(
            "/_matrix/client/v1/rendezvous/{session_id}/messages",
            get(get_messages),
        )
        .with_state(state)
}

async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let intent = body
        .get("intent")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("intent required".to_string()))?;

    let transport = body
        .get("transport")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("transport required".to_string()))?;

    let transport_data = body.get("transport_data").cloned();
    let expires_in_ms = body.get("expires_in_ms").and_then(|v| v.as_i64());

    let intent_enum = match intent {
        "login.reciprocate" => RendezvousIntent::LoginReciprocate,
        "login.start" => RendezvousIntent::LoginStart,
        _ => return Err(ApiError::bad_request(format!("Invalid intent: {}", intent))),
    };

    let transport_enum = match transport {
        "http.v1" => RendezvousTransport::HttpV1,
        "http.v2" => RendezvousTransport::HttpV2,
        _ => {
            return Err(ApiError::bad_request(format!(
                "Invalid transport: {}",
                transport
            )))
        }
    };

    let params = CreateRendezvousSessionParams {
        intent: intent_enum,
        transport: transport_enum,
        transport_data,
        expires_in_ms,
    };

    let session = state
        .services
        .rendezvous_storage
        .create_session(params)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create session: {}", e)))?;

    let rendezvous_url = format!(
        "matrix://rendezvous/{}/{}",
        &state.services.server_name, session.session_id
    );

    Ok(Json(json!({
        "url": rendezvous_url,
        "session_id": session.session_id,
        "key": session.key
    })))
}

fn extract_rendezvous_key(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(RENDEZVOUS_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
}

async fn load_rendezvous_session(
    state: &AppState,
    session_id: &str,
) -> Result<RendezvousSession, ApiError> {
    state
        .services
        .rendezvous_storage
        .get_session(session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get session: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Session not found or expired".to_string()))
}

async fn ensure_rendezvous_session_access(
    state: &AppState,
    headers: &HeaderMap,
    auth_user: &OptionalAuthenticatedUser,
    session_id: &str,
    action: &str,
) -> Result<RendezvousSession, ApiError> {
    let session = load_rendezvous_session(state, session_id).await?;

    if let Some(session_key) = extract_rendezvous_key(headers) {
        if session.key == session_key {
            return Ok(session);
        }

        return Err(ApiError::unauthorized(format!(
            "Invalid rendezvous key for {}",
            action
        )));
    }

    if auth_user.is_admin {
        return Ok(session);
    }

    if let (Some(auth_user_id), Some(bound_user_id)) =
        (auth_user.user_id.as_ref(), session.user_id.as_ref())
    {
        if auth_user_id == bound_user_id {
            return Ok(session);
        }

        return Err(ApiError::forbidden(format!(
            "You are not allowed to {} this rendezvous session",
            action
        )));
    }

    Err(ApiError::unauthorized(format!(
        "Rendezvous access to {} requires the {} header or the bound user",
        action, RENDEZVOUS_KEY_HEADER
    )))
}

async fn get_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let session =
        ensure_rendezvous_session_access(&state, &headers, &auth_user, &session_id, "read").await?;

    Ok(Json(json!({
        "session_id": session.session_id,
        "intent": session.intent,
        "transport": session.transport,
        "transport_data": session.transport_data,
        "status": session.status,
        "created_ts": session.created_ts,
        "expires_at": session.expires_at
    })))
}

async fn update_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_rendezvous_session_access(&state, &headers, &auth_user, &session_id, "update").await?;

    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("status required".to_string()))?;

    state
        .services
        .rendezvous_storage
        .update_session_status(&session_id, status)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update session: {}", e)))?;

    if status == "connected" {
        let user_id = auth_user
            .user_id
            .as_ref()
            .ok_or_else(ApiError::missing_token)?
            .clone();
        let device_id = auth_user
            .device_id
            .clone()
            .unwrap_or_else(|| "RENDEZVOUS".to_string());
        state
            .services
            .rendezvous_storage
            .bind_user_to_session(&session_id, &user_id, &device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to bind user: {}", e)))?;
    }

    if status == "completed" {
        let session = load_rendezvous_session(&state, &session_id).await?;

        if let Some(user_id) = &session.user_id {
            let device_id = session
                .device_id
                .clone()
                .unwrap_or_else(|| "RENDEZVOUS".to_string());

            let token = state
                .services
                .auth_service
                .generate_access_token(user_id, &device_id, false)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to generate token: {}", e)))?;

            return Ok(Json(json!({
                "session_id": session.session_id,
                "status": session.status,
                "login_finish": {
                    "access_token": token,
                    "device_id": device_id,
                    "user_id": user_id
                }
            })));
        }
    }

    Ok(Json(json!({
        "session_id": session_id,
        "status": status
    })))
}

async fn delete_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_rendezvous_session_access(&state, &headers, &auth_user, &session_id, "delete").await?;

    state
        .services
        .rendezvous_storage
        .delete_session(&session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete session: {}", e)))?;

    Ok(Json(json!({})))
}

async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_rendezvous_session_access(&state, &headers, &auth_user, &session_id, "send messages")
        .await?;

    let message_type = body
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("type required".to_string()))?;

    let content = body.get("content").cloned().unwrap_or(json!({}));

    let message = RendezvousMessage {
        message_type: message_type.to_string(),
        content,
    };

    let msg_storage = RendezvousMessageStorage::new(state.services.rendezvous_storage.pool.clone());

    msg_storage
        .store_message(&session_id, "outbound", &message)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send message: {}", e)))?;

    // Generate a message ID based on session and timestamp
    let message_id = format!("{}_{}", session_id, chrono::Utc::now().timestamp_millis());

    Ok(Json(json!({
        "session_id": session_id,
        "message_id": message_id,
        "sent_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn get_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_rendezvous_session_access(&state, &headers, &auth_user, &session_id, "read messages")
        .await?;

    let msg_storage = RendezvousMessageStorage::new(state.services.rendezvous_storage.pool.clone());

    let messages = msg_storage
        .get_messages(&session_id, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

    let messages_json: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "type": m.message_type,
                "content": m.content
            })
        })
        .collect();

    Ok(Json(json!({
        "messages": messages_json
    })))
}
