use super::{AppState, AuthenticatedUser};
use crate::common::ApiError;
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static SQL_INJECTION_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\b(SELECT|INSERT|UPDATE|DELETE|DROP|UNION|ALTER|CREATE|TRUNCATE)\b|--|;|' OR '|' AND ')").unwrap()
});

static XSS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(<script>|</script>|<iframe>|javascript:|onload=|onerror=|onmouseover=|alert\(|eval\()").unwrap()
});

fn contains_sql_injection(input: &str) -> bool {
    SQL_INJECTION_PATTERN.is_match(input)
}

fn contains_xss(input: &str) -> bool {
    XSS_PATTERN.is_match(input)
}

fn validate_matrix_user_id(user_id: &str) -> Result<(), String> {
    if !user_id.starts_with('@') {
        return Err("User ID must start with @".to_string());
    }
    if !user_id.contains(':') {
        return Err("User ID must contain : to specify the server".to_string());
    }
    let parts: Vec<&str> = user_id.splitn(2, ':').collect();
    if parts[0].len() < 2 {
        return Err("User ID localpart must be at least 1 character".to_string());
    }
    if parts[1].is_empty() {
        return Err("User ID server name cannot be empty".to_string());
    }
    if contains_sql_injection(user_id) || contains_xss(user_id) {
        return Err("User ID contains invalid characters".to_string());
    }
    Ok(())
}

fn validate_session_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Session ID cannot be empty".to_string());
    }
    if id.len() > 100 {
        return Err("Session ID must not exceed 100 characters".to_string());
    }
    if contains_sql_injection(id) || contains_xss(id) {
        return Err("Session ID contains invalid characters".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ':')
    {
        return Err("Session ID contains invalid characters".to_string());
    }
    Ok(())
}

fn validate_message_content(content: &str) -> Result<(), String> {
    if content.len() > 10000 {
        return Err("Message must not exceed 10000 characters".to_string());
    }
    if contains_sql_injection(content) {
        return Err("Message contains invalid characters".to_string());
    }
    if contains_xss(content) {
        return Err("Message contains invalid characters".to_string());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub participant: String,
    pub initial_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub participant: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct PrivateSession {
    pub session_id: String,
    pub participant: String,
    pub created_at: String,
    pub last_message: Option<String>,
    pub unread_count: i32,
}

pub fn create_private_chat_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/enhanced/private/sessions",
            get(get_sessions).post(create_session),
        )
        .route(
            "/_synapse/enhanced/private/sessions/{session_id}",
            get(get_session).delete(delete_session),
        )
        .route(
            "/_synapse/enhanced/private/sessions/{session_id}/messages",
            get(get_messages).post(send_message),
        )
        .with_state(state)
}

#[axum::debug_handler]
async fn create_session(
    auth_user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, ApiError> {
    if let Err(e) = validate_matrix_user_id(&body.participant) {
        return Err(ApiError::bad_request(e));
    }

    if let Some(ref initial_message) = body.initial_message {
        if let Err(e) = validate_message_content(initial_message) {
            return Err(ApiError::bad_request(e));
        }
    }

    let session = state
        .services
        .private_chat_service
        .create_session(&auth_user.user_id, &body.participant)
        .await?;

    Ok(Json(CreateSessionResponse {
        session_id: session.session_id,
        participant: body.participant,
        created_at: chrono::DateTime::from_timestamp_millis(session.created_at)
            .unwrap_or_default()
            .to_rfc3339(),
    }))
}

#[axum::debug_handler]
async fn get_sessions(
    auth_user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PrivateSession>>, ApiError> {
    let sessions = state
        .services
        .private_chat_service
        .get_sessions(&auth_user.user_id)
        .await?;

    // 转换为 API 响应格式 (这里暂时直接返回内部结构，实际应有 DTO 转换)
    // 注意：PrivateChatService 返回的是 storage::PrivateSession，
    // 而路由定义的 PrivateSession 是 API DTO。需要做映射。
    let api_sessions = sessions
        .into_iter()
        .map(|s| {
            // 确定对方 ID
            let participant = if s.user_id_1 == auth_user.user_id {
                s.user_id_2
            } else {
                s.user_id_1
            };

            PrivateSession {
                session_id: s.session_id,
                participant,
                created_at: chrono::DateTime::from_timestamp_millis(s.created_at)
                    .unwrap_or_default()
                    .to_rfc3339(),
                last_message: s.last_message_id, // 暂用 ID 代替内容，后续需查询内容
                unread_count: 0,                 // 暂未实现已读计数
            }
        })
        .collect();

    Ok(Json(api_sessions))
}

#[axum::debug_handler]
async fn get_session(
    auth_user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<PrivateSession>, ApiError> {
    if let Err(e) = validate_session_id(&session_id) {
        return Err(ApiError::bad_request(e));
    }

    let s = state
        .services
        .private_chat_service
        .get_session(&auth_user.user_id, &session_id)
        .await?;

    let participant = if s.user_id_1 == auth_user.user_id {
        s.user_id_2
    } else {
        s.user_id_1
    };

    Ok(Json(PrivateSession {
        session_id: s.session_id,
        participant,
        created_at: chrono::DateTime::from_timestamp_millis(s.created_at)
            .unwrap_or_default()
            .to_rfc3339(),
        last_message: s.last_message_id,
        unread_count: 0,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub message_id: String,
    pub created_at: String,
}

#[axum::debug_handler]
async fn send_message(
    auth_user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, ApiError> {
    if let Err(e) = validate_session_id(&session_id) {
        return Err(ApiError::bad_request(e));
    }

    match &body.content {
        serde_json::Value::String(text_content) => {
            if let Err(e) = validate_message_content(text_content) {
                return Err(ApiError::bad_request(e));
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(text_content)) = map.get("body") {
                if let Err(e) = validate_message_content(text_content) {
                    return Err(ApiError::bad_request(e));
                }
            }
        }
        _ => {}
    }

    let message = state
        .services
        .private_chat_service
        .send_message(&auth_user.user_id, &session_id, body.content)
        .await?;

    Ok(Json(SendMessageResponse {
        message_id: message.message_id,
        created_at: chrono::DateTime::from_timestamp_millis(message.created_at)
            .unwrap_or_default()
            .to_rfc3339(),
    }))
}

#[axum::debug_handler]
async fn get_messages(
    auth_user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    if let Err(e) = validate_session_id(&session_id) {
        return Err(ApiError::bad_request(e));
    }

    let messages = state
        .services
        .private_chat_service
        .get_messages(&auth_user.user_id, &session_id, 50, None)
        .await?;

    let api_messages = messages
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "message_id": m.message_id,
                "sender_id": m.sender_id,
                "content": m.content,
                "created_at": chrono::DateTime::from_timestamp_millis(m.created_at)
                    .unwrap_or_default()
                    .to_rfc3339(),
                "is_read": m.is_read
            })
        })
        .collect();

    Ok(Json(api_messages))
}

#[axum::debug_handler]
async fn delete_session(
    auth_user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Err(e) = validate_session_id(&session_id) {
        return Err(ApiError::bad_request(e));
    }

    let deleted_count = state
        .services
        .private_chat_service
        .delete_session(&auth_user.user_id, &session_id)
        .await?;

    Ok(Json(serde_json::json!({
        "deleted": true,
        "session_id": session_id,
        "messages_deleted": deleted_count
    })))
}
