use crate::common::ApiError;
use crate::storage::rendezvous::*;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Json, Path, State},
    routing::{delete, get, post, put},
    Router,
};
use serde_json::{json, Value};

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
    let expires_in_ms = body
        .get("expires_in_ms")
        .and_then(|v| v.as_i64());

    let intent_enum = match intent {
        "login.reciprocate" => RendezvousIntent::LoginReciprocate,
        "login.start" => RendezvousIntent::LoginStart,
        _ => return Err(ApiError::bad_request(format!("Invalid intent: {}", intent))),
    };

    let transport_enum = match transport {
        "http.v1" => RendezvousTransport::HttpV1,
        "http.v2" => RendezvousTransport::HttpV2,
        _ => return Err(ApiError::bad_request(format!("Invalid transport: {}", transport))),
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

async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let session = state
        .services
        .rendezvous_storage
        .get_session(&session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get session: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Session not found or expired".to_string()))?;

    Ok(Json(json!({
        "session_id": session.session_id,
        "intent": session.intent,
        "transport": session.transport,
        "transport_data": session.transport_data,
        "status": session.status,
        "created_ts": session.created_ts,
        "expires_ts": session.expires_ts
    })))
}

async fn update_session(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
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
        let device_id = auth_user.device_id.clone().unwrap_or_else(|| "RENDEZVOUS".to_string());
        state
            .services
            .rendezvous_storage
            .bind_user_to_session(&session_id, &auth_user.user_id, &device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to bind user: {}", e)))?;
    }

    if status == "completed" {
        let session = state
            .services
            .rendezvous_storage
            .get_session(&session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get session: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Session not found".to_string()))?;

        if let Some(user_id) = &session.user_id {
            let device_id = session.device_id.clone().unwrap_or_else(|| "RENDEZVOUS".to_string());
            
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
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
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
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
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

    Ok(Json(json!({})))
}

async fn get_messages(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
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
