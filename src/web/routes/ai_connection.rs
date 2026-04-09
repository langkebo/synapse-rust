use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::common::error::{ApiError, ApiResult};
use crate::services::matrix_ai_connection_service::{CreateConnectionRequest, McpToolCallRequest};
use crate::storage::ai_connection::AiConnection;
use crate::web::routes::AuthenticatedUser;
use crate::web::AppState;

#[derive(Debug, Deserialize)]
pub struct McpToolListQuery {
    pub provider: String,
}

pub fn create_ai_connection_router() -> Router<AppState> {
    Router::new()
        // AI 连接配置管理
        .route("/connections", get(get_connections).post(create_connection))
        .route(
            "/connections/{id}",
            get(get_connection).delete(delete_connection),
        )
        // MCP 代理调用
        .route("/mcp/tools", get(list_tools))
        .route("/mcp/tools/call", post(call_tool))
}

async fn get_connections(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> ApiResult<Json<Vec<AiConnection>>> {
    let connections = state
        .matrix_ai_connection_service
        .get_user_connections(&user.user_id)
        .await?;
    Ok(Json(connections))
}

async fn create_connection(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<CreateConnectionRequest>,
) -> ApiResult<Json<AiConnection>> {
    let conn = state
        .matrix_ai_connection_service
        .create_connection(&user.user_id, req)
        .await?;
    Ok(Json(conn))
}

async fn get_connection(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> ApiResult<Json<AiConnection>> {
    let conn = state
        .matrix_ai_connection_service
        .get_connection(&id, &user.user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;
    Ok(Json(conn))
}

async fn delete_connection(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> ApiResult<Json<()>> {
    state
        .matrix_ai_connection_service
        .delete_connection(&id, &user.user_id)
        .await?;
    Ok(Json(()))
}

// MCP 代理调用实现
async fn list_tools(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<McpToolListQuery>,
) -> ApiResult<Json<Value>> {
    let result = state
        .matrix_ai_connection_service
        .list_mcp_tools(&user.user_id, &params.provider)
        .await?;
    Ok(Json(result))
}

async fn call_tool(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<McpToolCallRequest>,
) -> ApiResult<Json<Value>> {
    let result = state
        .matrix_ai_connection_service
        .call_mcp_tool(&user.user_id, req)
        .await?;
    Ok(Json(result))
}
