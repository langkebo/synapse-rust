pub use synapse_services::matrix_ai_connection_service::*;

#[cfg(test)]
#[cfg(feature = "openclaw-routes")]
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
        let request = UpdateConnectionRequest { is_active: Some(false), config: None };

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
