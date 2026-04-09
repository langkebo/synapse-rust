use crate::common::error::ApiError;
use crate::services::mcp_proxy::McpProxyService;
use crate::storage::ai_connection::{AiConnection, AiConnectionStorage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConnectionRequest {
    pub provider: String,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConnectionRequest {
    pub is_active: Option<bool>,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallRequest {
    pub provider: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionResponse {
    pub connection: AiConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionListResponse {
    pub connections: Vec<AiConnection>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolListResponse {
    pub tools: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallResponse {
    pub result: Value,
}

pub struct MatrixAiConnectionService {
    storage: Arc<AiConnectionStorage>,
    mcp_proxy: Arc<McpProxyService>,
}

impl MatrixAiConnectionService {
    pub fn new(storage: Arc<AiConnectionStorage>, mcp_proxy: Arc<McpProxyService>) -> Self {
        Self { storage, mcp_proxy }
    }

    /// Get all AI connections for a user
    pub async fn get_user_connections(&self, user_id: &str) -> Result<Vec<AiConnection>, ApiError> {
        self.storage
            .get_user_connections(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user connections: {}", e)))
    }

    /// Get a specific AI connection by ID, checking ownership
    pub async fn get_connection(
        &self,
        id: &str,
        user_id: &str,
    ) -> Result<Option<AiConnection>, ApiError> {
        let conn = self
            .storage
            .get_connection(id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(ref c) = conn {
            if c.user_id != user_id {
                return Err(ApiError::forbidden("Access denied"));
            }
        }

        Ok(conn)
    }

    /// Create a new AI connection for a user
    pub async fn create_connection(
        &self,
        user_id: &str,
        request: CreateConnectionRequest,
    ) -> Result<AiConnection, ApiError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();

        let conn = AiConnection {
            id: id.clone(),
            user_id: user_id.to_string(),
            provider: request.provider,
            config: request.config,
            is_active: true,
            created_ts: now,
            updated_ts: Some(now),
        };

        self.storage
            .create_connection(&conn)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create connection: {}", e)))?;

        info!("Created AI connection {} for user {}", id, user_id);
        Ok(conn)
    }

    /// Update an existing AI connection (status and/or config)
    pub async fn update_connection(
        &self,
        id: &str,
        user_id: &str,
        request: UpdateConnectionRequest,
    ) -> Result<Option<AiConnection>, ApiError> {
        // First check ownership
        let existing = self
            .storage
            .get_connection(id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let existing = match existing {
            Some(c) => c,
            None => return Ok(None),
        };

        if existing.user_id != user_id {
            return Err(ApiError::forbidden("Access denied"));
        }

        // Update status if provided
        if let Some(is_active) = request.is_active {
            self.storage
                .update_connection_status(id, is_active)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to update connection status: {}", e))
                })?;
        }

        // Fetch and return updated connection
        let updated = self
            .storage
            .get_connection(id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if updated.is_some() {
            info!("Updated AI connection {}", id);
        }

        Ok(updated)
    }

    /// Delete an AI connection (with ownership check)
    pub async fn delete_connection(&self, id: &str, user_id: &str) -> Result<bool, ApiError> {
        // First check ownership
        let conn = self
            .storage
            .get_connection(id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Connection not found"))?;

        if conn.user_id != user_id {
            return Err(ApiError::forbidden("Access denied"));
        }

        self.storage
            .delete_connection(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete connection: {}", e)))?;

        info!("Deleted AI connection {} for user {}", id, user_id);
        Ok(true)
    }

    /// Get the active MCP URL for a user's provider
    fn extract_mcp_url(&self, conn: &AiConnection) -> Result<String, ApiError> {
        conn.config
            .as_ref()
            .and_then(|c| c.get("mcp_url"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::bad_request("mcp_url not found in connection config"))
    }

    /// List available MCP tools for a user's provider
    pub async fn list_mcp_tools(&self, user_id: &str, provider: &str) -> Result<Value, ApiError> {
        let conn = self
            .storage
            .get_user_provider_connection(user_id, provider)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "Active connection for provider {} not found",
                    provider
                ))
            })?;

        let mcp_url = self.extract_mcp_url(&conn)?;

        let result = self.mcp_proxy.list_tools(&mcp_url).await?;
        Ok(result)
    }

    /// Call an MCP tool for a user
    pub async fn call_mcp_tool(
        &self,
        user_id: &str,
        request: McpToolCallRequest,
    ) -> Result<Value, ApiError> {
        let conn = self
            .storage
            .get_user_provider_connection(user_id, &request.provider)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "Active connection for provider {} not found",
                    request.provider
                ))
            })?;

        let mcp_url = self.extract_mcp_url(&conn)?;

        let result = self
            .mcp_proxy
            .call_tool(
                &mcp_url,
                &request.tool_name,
                request.arguments,
                &request.provider,
                user_id,
            )
            .await?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_connection_request() {
        let request = CreateConnectionRequest {
            provider: "openai".to_string(),
            config: Some(serde_json::json!({
                "mcp_url": "https://example.com/mcp"
            })),
        };

        assert_eq!(request.provider, "openai");
        assert!(request.config.is_some());
    }

    #[test]
    fn test_update_connection_request() {
        let request = UpdateConnectionRequest {
            is_active: Some(false),
            config: None,
        };

        assert_eq!(request.is_active, Some(false));
        assert!(request.config.is_none());
    }

    #[test]
    fn test_mcp_tool_call_request() {
        let request = McpToolCallRequest {
            provider: "openai".to_string(),
            tool_name: "search".to_string(),
            arguments: serde_json::json!({"query": "test"}),
        };

        assert_eq!(request.tool_name, "search");
    }
}
