use axum::extract::FromRef;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::common::ApiError;
use crate::storage::openclaw::{
    AiChatRole, AiConversation, AiGeneration, AiMessage, OpenClawConnection, OpenClawStorage,
};
use crate::web::routes::extractors::auth::AuthenticatedUser as AuthInfo;
use crate::web::routes::AppState;

#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub id: i64,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub has_api_key: bool,
    pub config: Option<serde_json::Value>,
    pub is_default: bool,
    pub is_active: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<OpenClawConnection> for ConnectionResponse {
    fn from(conn: OpenClawConnection) -> Self {
        Self {
            id: conn.id,
            name: conn.name,
            provider: conn.provider,
            base_url: conn.base_url,
            has_api_key: conn.encrypted_api_key.is_some(),
            config: conn.config,
            is_default: conn.is_default,
            is_active: conn.is_active,
            created_ts: conn.created_ts,
            updated_ts: conn.updated_ts,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateConnectionRequest {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConnectionRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
    pub is_default: Option<bool>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ConversationResponse {
    pub id: i64,
    pub connection_id: Option<i64>,
    pub title: Option<String>,
    pub model_id: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_pinned: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<AiConversation> for ConversationResponse {
    fn from(conv: AiConversation) -> Self {
        Self {
            id: conv.id,
            connection_id: conv.connection_id,
            title: conv.title,
            model_id: conv.model_id,
            system_prompt: conv.system_prompt,
            temperature: conv.temperature,
            max_tokens: conv.max_tokens,
            is_pinned: conv.is_pinned,
            created_ts: conv.created_ts,
            updated_ts: conv.updated_ts,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub connection_id: Option<i64>,
    pub title: Option<String>,
    pub model_id: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_pinned: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    pub token_count: Option<i32>,
    pub tool_calls: Option<serde_json::Value>,
    pub created_ts: i64,
}

impl From<AiMessage> for MessageResponse {
    fn from(msg: AiMessage) -> Self {
        Self {
            id: msg.id,
            conversation_id: msg.conversation_id,
            role: msg.role,
            content: msg.content,
            token_count: msg.token_count,
            tool_calls: msg.tool_calls,
            created_ts: msg.created_ts,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    pub role: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatRoleResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub system_message: String,
    pub model_id: Option<String>,
    pub avatar_url: Option<String>,
    pub category: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_public: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<AiChatRole> for ChatRoleResponse {
    fn from(role: AiChatRole) -> Self {
        Self {
            id: role.id,
            name: role.name,
            description: role.description,
            system_message: role.system_message,
            model_id: role.model_id,
            avatar_url: role.avatar_url,
            category: role.category,
            temperature: role.temperature,
            max_tokens: role.max_tokens,
            is_public: role.is_public,
            created_ts: role.created_ts,
            updated_ts: role.updated_ts,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateChatRoleRequest {
    pub name: String,
    pub description: Option<String>,
    pub system_message: String,
    pub model_id: Option<String>,
    pub avatar_url: Option<String>,
    pub category: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub is_public: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChatRoleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_message: Option<String>,
    pub model_id: Option<String>,
    pub avatar_url: Option<String>,
    pub category: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_public: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct GenerationResponse {
    pub id: i64,
    pub conversation_id: Option<i64>,
    pub r#type: String,
    pub prompt: String,
    pub result_url: Option<String>,
    pub result_mxc: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_ts: i64,
    pub completed_ts: Option<i64>,
}

impl From<AiGeneration> for GenerationResponse {
    fn from(gen: AiGeneration) -> Self {
        Self {
            id: gen.id,
            conversation_id: gen.conversation_id,
            r#type: gen.r#type,
            prompt: gen.prompt,
            result_url: gen.result_url,
            result_mxc: gen.result_mxc,
            status: gen.status,
            error_message: gen.error_message,
            created_ts: gen.created_ts,
            completed_ts: gen.completed_ts,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateGenerationRequest {
    pub conversation_id: Option<i64>,
    pub r#type: String,
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub before: Option<i64>,
    pub r#type: Option<String>,
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

pub struct OpenClawState {
    pub storage: OpenClawStorage,
}

impl FromRef<AppState> for Arc<OpenClawState> {
    fn from_ref(state: &AppState) -> Self {
        Arc::new(OpenClawState {
            storage: OpenClawStorage::new(state.services.user_storage.pool.clone()),
        })
    }
}

pub fn create_openclaw_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/openclaw/connections",
            get(list_connections).post(create_connection),
        )
        .route(
            "/api/openclaw/connections/:id",
            get(get_connection)
                .put(update_connection)
                .delete(delete_connection),
        )
        .route("/api/openclaw/connections/:id/test", post(test_connection))
        .route(
            "/api/openclaw/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/openclaw/conversations/:id",
            get(get_conversation)
                .put(update_conversation)
                .delete(delete_conversation),
        )
        .route(
            "/api/openclaw/conversations/:id/messages",
            get(list_messages).post(send_message),
        )
        .route("/api/openclaw/messages/:id", delete(delete_message))
        .route(
            "/api/openclaw/generations",
            get(list_generations).post(create_generation),
        )
        .route(
            "/api/openclaw/generations/:id",
            get(get_generation).delete(delete_generation),
        )
        .route(
            "/api/openclaw/roles",
            get(list_chat_roles).post(create_chat_role),
        )
        .route(
            "/api/openclaw/roles/:id",
            get(get_chat_role)
                .put(update_chat_role)
                .delete(delete_chat_role),
        )
        .with_state(state)
}

async fn list_connections(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
) -> Result<Json<Vec<ConnectionResponse>>, ApiError> {
    let connections = state
        .storage
        .get_user_connections(&auth.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get connections: {}", e)))?;

    Ok(Json(
        connections
            .into_iter()
            .map(ConnectionResponse::from)
            .collect(),
    ))
}

async fn create_connection(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Json(req): Json<CreateConnectionRequest>,
) -> Result<Json<ConnectionResponse>, ApiError> {
    let encrypted_key = req.api_key.map(|k| encrypt_api_key(&k));

    let conn = state
        .storage
        .create_connection(crate::storage::openclaw::CreateConnectionParams {
            user_id: &auth.user_id,
            name: &req.name,
            provider: &req.provider,
            base_url: &req.base_url,
            encrypted_api_key: encrypted_key.as_deref(),
            config: req.config,
            is_default: req.is_default,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create connection: {}", e)))?;

    Ok(Json(ConnectionResponse::from(conn)))
}

async fn get_connection(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<ConnectionResponse>, ApiError> {
    let conn = state
        .storage
        .get_connection(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get connection: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;

    if conn.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    Ok(Json(ConnectionResponse::from(conn)))
}

async fn update_connection(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
    Json(req): Json<UpdateConnectionRequest>,
) -> Result<Json<ConnectionResponse>, ApiError> {
    let existing = state
        .storage
        .get_connection(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get connection: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;

    if existing.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    let encrypted_key = req.api_key.map(|k| encrypt_api_key(&k));

    let conn = state
        .storage
        .update_connection(crate::storage::openclaw::UpdateConnectionParams {
            id,
            name: req.name.as_deref(),
            base_url: req.base_url.as_deref(),
            encrypted_api_key: encrypted_key.as_deref(),
            config: req.config,
            is_default: req.is_default,
            is_active: req.is_active,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update connection: {}", e)))?;

    Ok(Json(ConnectionResponse::from(conn)))
}

async fn delete_connection(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .storage
        .get_connection(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get connection: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;

    if existing.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    state
        .storage
        .delete_connection(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete connection: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn test_connection(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state
        .storage
        .get_connection(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get connection: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;

    if conn.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    let start = std::time::Instant::now();
    let is_healthy = test_openclaw_health(&conn.base_url).await;
    let latency_ms = start.elapsed().as_millis() as i64;

    Ok(Json(serde_json::json!({
        "healthy": is_healthy,
        "latency_ms": latency_ms,
        "provider": conn.provider,
        "base_url": conn.base_url
    })))
}

async fn list_conversations(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<ConversationResponse>>, ApiError> {
    let conversations = state
        .storage
        .get_user_conversations(&auth.user_id, query.limit, query.offset)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversations: {}", e)))?;

    Ok(Json(PaginatedResponse {
        total: conversations.len() as i64,
        limit: query.limit,
        offset: query.offset,
        items: conversations
            .into_iter()
            .map(ConversationResponse::from)
            .collect(),
    }))
}

async fn create_conversation(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<ConversationResponse>, ApiError> {
    let conv = state
        .storage
        .create_conversation(crate::storage::openclaw::CreateConversationParams {
            user_id: &auth.user_id,
            connection_id: req.connection_id,
            title: req.title.as_deref(),
            model_id: req.model_id.as_deref(),
            system_prompt: req.system_prompt.as_deref(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create conversation: {}", e)))?;

    Ok(Json(ConversationResponse::from(conv)))
}

async fn get_conversation(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<ConversationResponse>, ApiError> {
    let conv = state
        .storage
        .get_conversation(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Conversation not found"))?;

    if conv.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    Ok(Json(ConversationResponse::from(conv)))
}

async fn update_conversation(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
    Json(req): Json<UpdateConversationRequest>,
) -> Result<Json<ConversationResponse>, ApiError> {
    let existing = state
        .storage
        .get_conversation(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Conversation not found"))?;

    if existing.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    let conv = state
        .storage
        .update_conversation(
            id,
            req.title.as_deref(),
            req.system_prompt.as_deref(),
            req.temperature,
            req.max_tokens,
            req.is_pinned,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update conversation: {}", e)))?;

    Ok(Json(ConversationResponse::from(conv)))
}

async fn delete_conversation(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .storage
        .get_conversation(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Conversation not found"))?;

    if existing.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    state
        .storage
        .delete_conversation(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete conversation: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_messages(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(conversation_id): Path<i64>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<MessageResponse>>, ApiError> {
    let conv = state
        .storage
        .get_conversation(conversation_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Conversation not found"))?;

    if conv.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    let messages = state
        .storage
        .get_conversation_messages(conversation_id, query.limit, query.before)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

    Ok(Json(PaginatedResponse {
        total: messages.len() as i64,
        limit: query.limit,
        offset: query.offset,
        items: messages.into_iter().map(MessageResponse::from).collect(),
    }))
}

async fn send_message(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(conversation_id): Path<i64>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    let conv = state
        .storage
        .get_conversation(conversation_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Conversation not found"))?;

    if conv.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    let role = req.role.unwrap_or_else(|| "user".to_string());

    let msg = state
        .storage
        .create_message(
            conversation_id,
            &role,
            &req.content,
            None,
            req.tool_calls,
            req.tool_call_id.as_deref(),
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create message: {}", e)))?;

    Ok(Json(MessageResponse::from(msg)))
}

async fn delete_message(
    State(state): State<Arc<OpenClawState>>,
    _auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state
        .storage
        .delete_message(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete message: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_generations(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<GenerationResponse>>, ApiError> {
    let generations = state
        .storage
        .get_user_generations(
            &auth.user_id,
            query.r#type.as_deref(),
            query.limit,
            query.offset,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get generations: {}", e)))?;

    Ok(Json(PaginatedResponse {
        total: generations.len() as i64,
        limit: query.limit,
        offset: query.offset,
        items: generations
            .into_iter()
            .map(GenerationResponse::from)
            .collect(),
    }))
}

async fn create_generation(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Json(req): Json<CreateGenerationRequest>,
) -> Result<Json<GenerationResponse>, ApiError> {
    let gen = state
        .storage
        .create_generation(&auth.user_id, req.conversation_id, &req.r#type, &req.prompt)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create generation: {}", e)))?;

    Ok(Json(GenerationResponse::from(gen)))
}

async fn get_generation(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<GenerationResponse>, ApiError> {
    let gen = state
        .storage
        .get_generation(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get generation: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Generation not found"))?;

    if gen.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    Ok(Json(GenerationResponse::from(gen)))
}

async fn delete_generation(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .storage
        .get_generation(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get generation: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Generation not found"))?;

    if existing.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    state
        .storage
        .delete_generation(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete generation: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_chat_roles(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
) -> Result<Json<Vec<ChatRoleResponse>>, ApiError> {
    let roles = state
        .storage
        .get_user_chat_roles(&auth.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get chat roles: {}", e)))?;

    Ok(Json(
        roles.into_iter().map(ChatRoleResponse::from).collect(),
    ))
}

async fn create_chat_role(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Json(req): Json<CreateChatRoleRequest>,
) -> Result<Json<ChatRoleResponse>, ApiError> {
    let role = state
        .storage
        .create_chat_role(crate::storage::openclaw::CreateChatRoleParams {
            user_id: &auth.user_id,
            name: &req.name,
            description: req.description.as_deref(),
            system_message: &req.system_message,
            model_id: req.model_id.as_deref(),
            avatar_url: req.avatar_url.as_deref(),
            category: req.category.as_deref(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            is_public: req.is_public,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create chat role: {}", e)))?;

    Ok(Json(ChatRoleResponse::from(role)))
}

async fn get_chat_role(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<ChatRoleResponse>, ApiError> {
    let role = state
        .storage
        .get_chat_role(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get chat role: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

    if !role.is_public && role.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    Ok(Json(ChatRoleResponse::from(role)))
}

async fn update_chat_role(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
    Json(req): Json<UpdateChatRoleRequest>,
) -> Result<Json<ChatRoleResponse>, ApiError> {
    let existing = state
        .storage
        .get_chat_role(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get chat role: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

    if existing.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    let role = state
        .storage
        .update_chat_role(crate::storage::openclaw::UpdateChatRoleParams {
            id,
            name: req.name.as_deref(),
            description: req.description.as_deref(),
            system_message: req.system_message.as_deref(),
            model_id: req.model_id.as_deref(),
            avatar_url: req.avatar_url.as_deref(),
            category: req.category.as_deref(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            is_public: req.is_public,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update chat role: {}", e)))?;

    Ok(Json(ChatRoleResponse::from(role)))
}

async fn delete_chat_role(
    State(state): State<Arc<OpenClawState>>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .storage
        .get_chat_role(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get chat role: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

    if existing.user_id != auth.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    state
        .storage
        .delete_chat_role(id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete chat role: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

fn get_encryption_key() -> [u8; 32] {
    use std::env;
    let key = env::var("API_KEY_ENCRYPTION_KEY").unwrap_or_else(|_| {
        let default = "synapse-rust-default-encryption-key-change-in-production!!";
        tracing::warn!("API_KEY_ENCRYPTION_KEY not set, using default (INSECURE for production)");
        default.to_string()
    });
    let mut key_bytes = [0u8; 32];
    let source = key.as_bytes();
    let len = std::cmp::min(source.len(), 32);
    key_bytes[..len].copy_from_slice(&source[..len]);
    key_bytes
}

fn encrypt_api_key(key: &str) -> String {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
    use rand::RngCore;

    let key_bytes = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key_bytes).expect("Valid key length");
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, key.as_bytes())
        .expect("Encryption failed");

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);

    BASE64_STANDARD.encode(&combined)
}

fn decrypt_api_key(encrypted: &str) -> Option<String> {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};

    let combined = BASE64_STANDARD.decode(encrypted).ok()?;
    if combined.len() < 12 {
        return None;
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let key_bytes = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key_bytes).ok()?;
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;
    String::from_utf8(plaintext).ok()
}

async fn test_openclaw_health(base_url: &str) -> bool {
    use reqwest::Client;
    use std::time::Duration;

    let client = match Client::builder().timeout(Duration::from_secs(5)).build() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let health_url = format!("{}/health", base_url.trim_end_matches('/'));

    match client.get(&health_url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}
