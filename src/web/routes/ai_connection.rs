use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::common::error::{ApiError, ApiResult};
use crate::storage::ai_connection::AiConnection;
use crate::web::routes::AuthenticatedUser;
use crate::web::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateConnectionReq {
    pub provider: String,
    pub config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct McpToolCallReq {
    pub provider: String,
    pub tool_name: String,
    pub arguments: Value,
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
        .ai_connection_storage
        .get_user_connections(&user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get AI connections: {}", e)))?;
    Ok(Json(connections))
}

async fn create_connection(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<CreateConnectionReq>,
) -> ApiResult<Json<AiConnection>> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    let conn = AiConnection {
        id,
        user_id: user.user_id.clone(),
        provider: req.provider,
        config: req.config,
        is_active: true,
        created_ts: now,
        updated_ts: Some(now),
    };

    state
        .ai_connection_storage
        .create_connection(&conn)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create AI connection: {}", e)))?;

    Ok(Json(conn))
}

async fn get_connection(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> ApiResult<Json<AiConnection>> {
    let conn = state
        .ai_connection_storage
        .get_connection(&id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;

    if conn.user_id != user.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    Ok(Json(conn))
}

async fn delete_connection(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> ApiResult<Json<()>> {
    let conn = state
        .ai_connection_storage
        .get_connection(&id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;

    if conn.user_id != user.user_id {
        return Err(ApiError::forbidden("Access denied"));
    }

    state
        .ai_connection_storage
        .delete_connection(&id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete connection: {}", e)))?;

    Ok(Json(()))
}

// MCP 代理调用实现
async fn list_tools(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let provider = params
        .get("provider")
        .ok_or_else(|| ApiError::bad_request("Missing provider parameter"))?;

    // 获取用户的连接配置
    let conn = state
        .ai_connection_storage
        .get_user_provider_connection(&user.user_id, provider)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "Active connection for provider {} not found",
                provider
            ))
        })?;

    // 提取 MCP URL
    let mcp_url = extract_mcp_url(&conn)?;

    // 代理调用 MCP
    let result = state.mcp_proxy_service.list_tools(&mcp_url).await?;
    Ok(Json(result))
}

async fn call_tool(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<McpToolCallReq>,
) -> ApiResult<Json<Value>> {
    // 获取用户的连接配置
    let conn = state
        .ai_connection_storage
        .get_user_provider_connection(&user.user_id, &req.provider)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "Active connection for provider {} not found",
                req.provider
            ))
        })?;

    // 提取 MCP URL
    let mcp_url = extract_mcp_url(&conn)?;

    // 代理调用 MCP，内部已封装缓存逻辑
    let result = state
        .mcp_proxy_service
        .call_tool(
            &mcp_url,
            &req.tool_name,
            req.arguments,
            &req.provider,
            &user.user_id,
        )
        .await?;

    Ok(Json(result))
}

fn extract_mcp_url(conn: &AiConnection) -> ApiResult<String> {
    conn.config
        .as_ref()
        .and_then(|c| c.get("mcp_url"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::bad_request("mcp_url not found in connection config"))
}
