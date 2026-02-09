use super::{AppState, AuthenticatedUser};
use crate::common::ApiError;
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

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
    let api_sessions = sessions.into_iter().map(|s| {
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
            unread_count: 0, // 暂未实现已读计数
        }
    }).collect();

    Ok(Json(api_sessions))
}

#[axum::debug_handler]
async fn get_session(
    auth_user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<PrivateSession>, ApiError> {
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
    // 暂未实现分页参数解析
    let messages = state
        .services
        .private_chat_service
        .get_messages(&auth_user.user_id, &session_id, 50, None)
        .await?;

    // 转换为 API 响应格式
    let api_messages = messages.into_iter().map(|m| {
        serde_json::json!({
            "message_id": m.message_id,
            "sender_id": m.sender_id,
            "content": m.content,
            "created_at": chrono::DateTime::from_timestamp_millis(m.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
            "is_read": m.is_read
        })
    }).collect();

    Ok(Json(api_messages))
}

#[axum::debug_handler]
async fn delete_session(
    _auth_user: AuthenticatedUser,
    State(_state): State<AppState>,
    Path(_session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 删除会话
    Ok(Json(serde_json::json!({})))
}
