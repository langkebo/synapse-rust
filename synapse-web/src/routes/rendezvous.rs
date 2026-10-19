use crate::routes::context::AuthContext;
use crate::routes::extractors::SessionId;
use crate::routes::MatrixJson;
use crate::routes::{AppState, OptionalAuthenticatedUser};
use crate::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_services::rendezvous_service::{
    CreateRendezvousSessionParams, RendezvousIntent, RendezvousMessage, RendezvousSession, RendezvousTransport,
    RENDEZVOUS_KEY_HEADER,
};

/// See [`create_rendezvous_router`].
pub fn create_rendezvous_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/rendezvous", post(create_session))
        .route("/_matrix/client/v1/rendezvous/{session_id}", get(get_session))
        .route("/_matrix/client/v1/rendezvous/{session_id}", put(update_session))
        .route("/_matrix/client/v1/rendezvous/{session_id}", delete(delete_session))
        .route("/_matrix/client/v1/rendezvous/{session_id}/messages", post(send_message))
        .route("/_matrix/client/v1/rendezvous/{session_id}/messages", get(get_messages))
        .with_state(state)
}

/// Request body for [`create_session`].
#[derive(Debug, Deserialize)]
pub struct CreateSessionBody {
    /// Rendezvous intent, e.g. `login.start`.
    pub intent: String,
    /// Rendezvous transport, e.g. `http.v1`.
    pub transport: String,
    /// Opaque transport-specific data.
    #[serde(default)]
    pub transport_data: Option<Value>,
    /// Requested session lifetime in milliseconds.
    #[serde(default)]
    pub expires_in_ms: Option<i64>,
}

async fn create_session(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<CreateSessionBody>,
    // Note: the router is Router<AppState> — AuthContext extracted via FromRef
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let intent = body.intent.as_str();
    let transport = body.transport.as_str();

    let transport_data = body.transport_data;
    let expires_in_ms = body.expires_in_ms;

    let intent_enum = match intent {
        "login.reciprocate" => RendezvousIntent::LoginReciprocate,
        "login.start" => RendezvousIntent::LoginStart,
        _ => return Err(ApiError::bad_request(format!("Invalid intent: {intent}"))),
    };

    let transport_enum = match transport {
        "http.v1" => RendezvousTransport::HttpV1,
        "http.v2" => RendezvousTransport::HttpV2,
        _ => return Err(ApiError::bad_request(format!("Invalid transport: {transport}"))),
    };

    let params =
        CreateRendezvousSessionParams { intent: intent_enum, transport: transport_enum, transport_data, expires_in_ms };

    let session: RendezvousSession = ctx.rendezvous_service.create_session(params).await?;

    ::tracing::info!(request_id = %request_id, session_id = %session.session_id, intent = ?session.intent, "Created rendezvous session");

    let rendezvous_url: String = format!("matrix://rendezvous://{}/{}", &ctx.server_name, session.session_id);

    Ok(Json(json!({
        "url": rendezvous_url,
        "session_id": session.session_id,
        "key": session.key
    })))
}

fn extract_rendezvous_key(headers: &HeaderMap) -> Option<&str> {
    headers.get(RENDEZVOUS_KEY_HEADER).and_then(|value| value.to_str().ok()).filter(|value| !value.is_empty())
}

/// Authorise access to a rendezvous session (session key or bound user).
async fn ensure_rendezvous_session_access(
    ctx: &AuthContext,
    request_id: &str,
    headers: &HeaderMap,
    auth_user: &OptionalAuthenticatedUser,
    session_id: &str,
    action: &str,
) -> Result<RendezvousSession, ApiError> {
    ctx.rendezvous_service
        .authorize(session_id, extract_rendezvous_key(headers), auth_user.user_id.as_deref(), request_id, action)
        .await
}

async fn get_session(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<SessionId>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let session =
        ensure_rendezvous_session_access(&ctx, &request_id, &headers, &auth_user, &session_id, "read").await?;

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

/// Request body for [`update_session`].
#[derive(Debug, Deserialize)]
pub struct UpdateSessionBody {
    /// New session status, e.g. `connected` or `completed`.
    pub status: String,
}

async fn update_session(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<SessionId>,
    MatrixJson(body): MatrixJson<UpdateSessionBody>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    ensure_rendezvous_session_access(&ctx, &request_id, &headers, &auth_user, &session_id, "update").await?;

    let status = body.status.as_str();

    ctx.rendezvous_service.update_status(&session_id, status).await?;
    ::tracing::info!(request_id = %request_id, session_id = %session_id, status, "Updated rendezvous session");

    if status == "connected" {
        let user_id = auth_user.user_id.as_ref().ok_or_else(ApiError::missing_token)?.clone();
        let device_id = auth_user.device_id.clone().unwrap_or_else(|| "RENDEZVOUS".to_string());
        ctx.rendezvous_service.bind_user(&session_id, &user_id, &device_id).await?;
    }

    if status == "completed" {
        let session: RendezvousSession = ctx.rendezvous_service.load_session(&session_id).await?;

        if let Some(user_id) = &session.user_id {
            let device_id: String = session.device_id.clone().unwrap_or_else(|| "RENDEZVOUS".to_string());

            let token: String = ctx
                .token_auth
                .generate_access_token(user_id, &device_id, false)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to generate token", e))?;

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
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<SessionId>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    ensure_rendezvous_session_access(&ctx, &request_id, &headers, &auth_user, &session_id, "delete").await?;

    ctx.rendezvous_service.delete_session(&session_id).await?;
    ::tracing::info!(request_id = %request_id, session_id = %session_id, "Deleted rendezvous session");

    Ok(Json(json!({})))
}

/// Request body for [`send_message`].
#[derive(Debug, Deserialize)]
pub struct SendMessageBody {
    /// Opaque message type.
    #[serde(rename = "type")]
    pub message_type: String,
    /// Opaque message payload.
    #[serde(default)]
    pub content: Option<Value>,
}

async fn send_message(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<SessionId>,
    MatrixJson(body): MatrixJson<SendMessageBody>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    ensure_rendezvous_session_access(&ctx, &request_id, &headers, &auth_user, &session_id, "send messages").await?;

    let message_type = body.message_type.as_str();

    let content = body.content.unwrap_or_else(|| json!({}));

    let message = RendezvousMessage { message_type: message_type.to_string(), content };

    ctx.rendezvous_service.store_message(&session_id, "outbound", &message).await?;
    ::tracing::info!(request_id = %request_id, session_id = %session_id, message_type, "Stored rendezvous message");

    // Generate a message ID based on session and timestamp
    let message_id = format!("{}_{}", session_id, current_timestamp_millis());

    Ok(Json(json!({
        "session_id": session_id,
        "message_id": message_id,
        "sent_ts": current_timestamp_millis()
    })))
}

async fn get_messages(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    auth_user: OptionalAuthenticatedUser,
    Path(session_id): Path<SessionId>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    ensure_rendezvous_session_access(&ctx, &request_id, &headers, &auth_user, &session_id, "read messages").await?;

    let messages = ctx.rendezvous_service.messages(&session_id).await?;

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
