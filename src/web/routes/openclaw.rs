use crate::web::routes::context::AdminContext;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::common::ApiError;
use crate::web::routes::extractors::auth::AuthenticatedUser as AuthInfo;
use crate::web::routes::{AppState, PaginatedResponse};
use synapse_services::openclaw_service::OpenClawService;
use synapse_storage::openclaw::{AiChatRole, AiConversation, AiGeneration, AiMessage, OpenClawConnection};

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

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

#[derive(Debug, Serialize, Deserialize)]
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
    fn from(generation: AiGeneration) -> Self {
        Self {
            id: generation.id,
            conversation_id: generation.conversation_id,
            r#type: generation.r#type,
            prompt: generation.prompt,
            result_url: generation.result_url,
            result_mxc: generation.result_mxc,
            status: generation.status,
            error_message: generation.error_message,
            created_ts: generation.created_ts,
            completed_ts: generation.completed_ts,
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
    pub from: Option<String>,
    pub before: Option<i64>,
    pub r#type: Option<String>,
}

fn default_limit() -> i64 {
    20
}

const OPENCLAW_UNSTABLE_PREFIX: &str = "/_matrix/client/unstable/org.synapse_rust.openclaw";

pub fn create_openclaw_router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest(
            OPENCLAW_UNSTABLE_PREFIX,
            Router::new()
                .route("/connections", get(list_connections).post(create_connection))
                .route("/connections/{id}", get(get_connection).put(update_connection).delete(delete_connection))
                .route("/connections/{id}/test", post(test_connection))
                .route("/conversations", get(list_conversations).post(create_conversation))
                .route(
                    "/conversations/{id}",
                    get(get_conversation).put(update_conversation).delete(delete_conversation),
                )
                .route("/conversations/{id}/messages", get(list_messages).post(send_message))
                .route("/messages/{id}", delete(delete_message))
                .route("/generations", get(list_generations).post(create_generation))
                .route("/generations/{id}", get(get_generation).delete(delete_generation))
                .route("/roles", get(list_chat_roles).post(create_chat_role))
                .route("/roles/{id}", get(get_chat_role).put(update_chat_role).delete(delete_chat_role)),
        )
        .with_state(state)
}

pub fn openclaw_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/connections"),
        (Method::POST, "/_matrix/client/unstable/org.synapse_rust.openclaw/connections"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/connections/{id}"),
        (Method::PUT, "/_matrix/client/unstable/org.synapse_rust.openclaw/connections/{id}"),
        (Method::DELETE, "/_matrix/client/unstable/org.synapse_rust.openclaw/connections/{id}"),
        (Method::POST, "/_matrix/client/unstable/org.synapse_rust.openclaw/connections/{id}/test"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/conversations"),
        (Method::POST, "/_matrix/client/unstable/org.synapse_rust.openclaw/conversations"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}"),
        (Method::PUT, "/_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}"),
        (Method::DELETE, "/_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}/messages"),
        (Method::POST, "/_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}/messages"),
        (Method::DELETE, "/_matrix/client/unstable/org.synapse_rust.openclaw/messages/{id}"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/generations"),
        (Method::POST, "/_matrix/client/unstable/org.synapse_rust.openclaw/generations"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/generations/{id}"),
        (Method::DELETE, "/_matrix/client/unstable/org.synapse_rust.openclaw/generations/{id}"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/roles"),
        (Method::POST, "/_matrix/client/unstable/org.synapse_rust.openclaw/roles"),
        (Method::GET, "/_matrix/client/unstable/org.synapse_rust.openclaw/roles/{id}"),
        (Method::PUT, "/_matrix/client/unstable/org.synapse_rust.openclaw/roles/{id}"),
        (Method::DELETE, "/_matrix/client/unstable/org.synapse_rust.openclaw/roles/{id}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "openclaw"))
    .collect()
}

// ---------------------------------------------------------------------------
// Route handlers — thin HTTP adapters delegating to OpenClawService
// ---------------------------------------------------------------------------

fn svc(ctx: &AdminContext) -> &Arc<OpenClawService> {
    &ctx.openclaw_service
}

async fn list_connections(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
) -> Result<Json<Vec<ConnectionResponse>>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let connections: Vec<synapse_storage::openclaw::OpenClawConnection> =
        svc(&ctx).list_connections(&auth.user_id).await?;
    Ok(Json(connections.into_iter().map(ConnectionResponse::from).collect()))
}

async fn create_connection(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Json(req): Json<CreateConnectionRequest>,
) -> Result<Json<ConnectionResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let conn: synapse_storage::openclaw::OpenClawConnection = svc(&ctx)
        .create_connection(
            &auth.user_id,
            &req.name,
            &req.provider,
            &req.base_url,
            req.api_key,
            req.config,
            req.is_default,
        )
        .await?;
    Ok(Json(ConnectionResponse::from(conn)))
}

async fn get_connection(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<ConnectionResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let conn: synapse_storage::openclaw::OpenClawConnection =
        svc(&ctx).get_connection_for_user(id, &auth.user_id).await?;
    Ok(Json(ConnectionResponse::from(conn)))
}

async fn update_connection(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
    Json(req): Json<UpdateConnectionRequest>,
) -> Result<Json<ConnectionResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let conn: synapse_storage::openclaw::OpenClawConnection = svc(&ctx)
        .update_connection(
            id,
            &auth.user_id,
            req.name,
            req.base_url,
            req.api_key,
            req.config,
            req.is_default,
            req.is_active,
        )
        .await?;
    Ok(Json(ConnectionResponse::from(conn)))
}

async fn delete_connection(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    svc(&ctx).delete_connection(id, &auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_connection(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let (conn, is_healthy, latency_ms): (synapse_storage::openclaw::OpenClawConnection, bool, i64) =
        svc(&ctx).test_connection(id, &auth.user_id).await?;
    Ok(Json(serde_json::json!({
        "healthy": is_healthy,
        "latency_ms": latency_ms,
        "provider": conn.provider,
        "base_url": conn.base_url
    })))
}

async fn list_conversations(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<ConversationResponse>>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let (conversations, next_batch): (Vec<synapse_storage::openclaw::AiConversation>, Option<String>) =
        svc(&ctx).list_conversations(&auth.user_id, query.limit, query.from).await?;
    Ok(Json(PaginatedResponse {
        total: Some(conversations.len() as i64),
        limit: query.limit,
        next_batch,
        items: conversations.into_iter().map(ConversationResponse::from).collect(),
    }))
}

async fn create_conversation(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<ConversationResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let conv = svc(&ctx)
        .create_conversation(
            &auth.user_id,
            req.connection_id,
            req.title.as_deref(),
            req.model_id.as_deref(),
            req.system_prompt.as_deref(),
            req.temperature,
            req.max_tokens,
        )
        .await?;
    Ok(Json(ConversationResponse::from(conv)))
}

async fn get_conversation(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<ConversationResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let conv = svc(&ctx).get_conversation_for_user(id, &auth.user_id).await?;
    Ok(Json(ConversationResponse::from(conv)))
}

async fn update_conversation(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
    Json(req): Json<UpdateConversationRequest>,
) -> Result<Json<ConversationResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let conv = svc(&ctx)
        .update_conversation(
            id,
            &auth.user_id,
            req.title.as_deref(),
            req.system_prompt.as_deref(),
            req.temperature,
            req.max_tokens,
            req.is_pinned,
        )
        .await?;
    Ok(Json(ConversationResponse::from(conv)))
}

async fn delete_conversation(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    svc(&ctx).delete_conversation(id, &auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_messages(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(conversation_id): Path<i64>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<MessageResponse>>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let (messages, next_batch): (Vec<synapse_storage::openclaw::AiMessage>, Option<String>) =
        svc(&ctx).list_messages(conversation_id, &auth.user_id, query.limit, query.from, query.before).await?;
    Ok(Json(PaginatedResponse {
        total: Some(messages.len() as i64),
        limit: query.limit,
        next_batch,
        items: messages.into_iter().map(MessageResponse::from).collect(),
    }))
}

async fn send_message(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(conversation_id): Path<i64>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let msg = svc(&ctx)
        .send_message(
            conversation_id,
            &auth.user_id,
            &req.content,
            req.role.as_deref(),
            req.tool_calls,
            req.tool_call_id.as_deref(),
        )
        .await?;
    Ok(Json(MessageResponse::from(msg)))
}

async fn delete_message(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    svc(&ctx).delete_message(id, &auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_generations(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<GenerationResponse>>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let (generations, next_batch): (Vec<synapse_storage::openclaw::AiGeneration>, Option<String>) =
        svc(&ctx).list_generations(&auth.user_id, query.r#type.as_deref(), query.limit, query.from).await?;
    Ok(Json(PaginatedResponse {
        total: Some(generations.len() as i64),
        limit: query.limit,
        next_batch,
        items: generations.into_iter().map(GenerationResponse::from).collect(),
    }))
}

async fn create_generation(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Json(req): Json<CreateGenerationRequest>,
) -> Result<Json<GenerationResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let generation = svc(&ctx).create_generation(&auth.user_id, req.conversation_id, &req.r#type, &req.prompt).await?;
    Ok(Json(GenerationResponse::from(generation)))
}

async fn get_generation(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<GenerationResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let generation = svc(&ctx).get_generation_for_user(id, &auth.user_id).await?;
    Ok(Json(GenerationResponse::from(generation)))
}

async fn delete_generation(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    svc(&ctx).delete_generation(id, &auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_chat_roles(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
) -> Result<Json<Vec<ChatRoleResponse>>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let roles: Vec<synapse_storage::openclaw::AiChatRole> = svc(&ctx).list_chat_roles(&auth.user_id).await?;
    Ok(Json(roles.into_iter().map(ChatRoleResponse::from).collect()))
}

async fn create_chat_role(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Json(req): Json<CreateChatRoleRequest>,
) -> Result<Json<ChatRoleResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let role: synapse_storage::openclaw::AiChatRole = svc(&ctx)
        .create_chat_role(
            &auth.user_id,
            &req.name,
            req.description.as_deref(),
            &req.system_message,
            req.model_id.as_deref(),
            req.avatar_url.as_deref(),
            req.category.as_deref(),
            req.temperature,
            req.max_tokens,
            req.is_public,
        )
        .await?;
    Ok(Json(ChatRoleResponse::from(role)))
}

async fn get_chat_role(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<Json<ChatRoleResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let role: synapse_storage::openclaw::AiChatRole = svc(&ctx).get_chat_role_for_user(id, &auth.user_id).await?;
    Ok(Json(ChatRoleResponse::from(role)))
}

async fn update_chat_role(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
    Json(req): Json<UpdateChatRoleRequest>,
) -> Result<Json<ChatRoleResponse>, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    let role: synapse_storage::openclaw::AiChatRole = svc(&ctx)
        .update_chat_role(
            id,
            &auth.user_id,
            req.name.as_deref(),
            req.description.as_deref(),
            req.system_message.as_deref(),
            req.model_id.as_deref(),
            req.avatar_url.as_deref(),
            req.category.as_deref(),
            req.temperature,
            req.max_tokens,
            req.is_public,
        )
        .await?;
    Ok(Json(ChatRoleResponse::from(role)))
}

async fn delete_chat_role(
    State(ctx): State<AdminContext>,
    auth: AuthInfo,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    svc(&ctx).ensure_user_allowed(auth.is_guest)?;
    svc(&ctx).delete_chat_role(id, &auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
