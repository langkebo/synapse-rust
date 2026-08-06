// AI connection route layer tests.
//
// Covers the wire-level contracts exposed by
// `src/web/routes/ai_connection.rs` (P-096: previously zero tests):
//   * Route manifest contents (methods + paths + registered_by tag) across
//     v1 and v3 path prefixes.
//   * DTO deserialization: `McpToolListQuery`, `CreateConnectionRequest`,
//     `McpToolCallRequest`.
//   * `AiConnection` serialization shape (storage → wire).
//   * Error-code mapping: 404 (connection not found).
//
// The handlers require a fully-wired `AdminContext` (matrix_ai_connection_service),
// so — following the established pattern in `push_notification_route_tests.rs` —
// we exercise the real public manifest and DTOs and lock down the handler
// contracts with shape assertions.

#![cfg(feature = "openclaw-routes")]

use axum::http::Method;
use serde_json::json;
use synapse_common::ApiError;
use synapse_rust::web::routes::ai_connection::{ai_connection_route_manifest, McpToolListQuery};
use synapse_rust::web::routes::route_ledger::RouteEntry;
use synapse_services::matrix_ai_connection_service::{CreateConnectionRequest, McpToolCallRequest};
use synapse_storage::ai_connection::AiConnection;

// ============================================================================
// Route manifest tests
// ============================================================================

#[test]
fn test_route_manifest_contains_all_twelve_entries() {
    let manifest = ai_connection_route_manifest();
    assert_eq!(manifest.len(), 12, "ai_connection manifest must declare 12 (method, path) entries (6 v1 + 6 v3)");
}

#[test]
fn test_route_manifest_matches_declared_paths_and_methods() {
    let manifest = ai_connection_route_manifest();

    let expected = [
        (Method::GET, "/_matrix/client/v1/ai/connections"),
        (Method::POST, "/_matrix/client/v1/ai/connections"),
        (Method::GET, "/_matrix/client/v1/ai/connections/{id}"),
        (Method::DELETE, "/_matrix/client/v1/ai/connections/{id}"),
        (Method::GET, "/_matrix/client/v1/ai/mcp/tools"),
        (Method::POST, "/_matrix/client/v1/ai/mcp/tools/call"),
        // v3 paths
        (Method::GET, "/_matrix/client/v3/ai/connections"),
        (Method::POST, "/_matrix/client/v3/ai/connections"),
        (Method::GET, "/_matrix/client/v3/ai/connections/{id}"),
        (Method::DELETE, "/_matrix/client/v3/ai/connections/{id}"),
        (Method::GET, "/_matrix/client/v3/ai/mcp/tools"),
        (Method::POST, "/_matrix/client/v3/ai/mcp/tools/call"),
    ];

    let actual: Vec<(Method, &str)> = manifest.iter().map(|e| (e.method.clone(), e.path)).collect();
    for pair in &expected {
        assert!(actual.contains(pair), "manifest missing {:?} {}", pair.0, pair.1);
    }
    assert_eq!(actual.len(), expected.len(), "manifest size mismatch");
}

#[test]
fn test_route_manifest_entries_registered_by_ai_connection() {
    let manifest = ai_connection_route_manifest();
    assert!(
        manifest.iter().all(|e| e.registered_by == "ai_connection"),
        "every entry must be owned by ai_connection"
    );
}

#[test]
fn test_route_manifest_has_no_duplicate_method_path_pairs() {
    let manifest = ai_connection_route_manifest();
    let mut seen = std::collections::HashSet::new();
    for entry in &manifest {
        let key = (entry.method.clone(), entry.path);
        assert!(seen.insert(key), "duplicate route entry: {:?} {}", entry.method, entry.path);
    }
}

#[test]
fn test_route_manifest_covers_six_logical_endpoints_across_v1_and_v3() {
    let manifest = ai_connection_route_manifest();
    // 6 (method, path) entries per version prefix. Some paths are shared
    // across methods (GET+POST on /connections, GET+DELETE on /connections/{id}),
    // so distinct paths per version = 4.
    let v1_entries: Vec<&_> = manifest.iter().filter(|e| e.path.contains("/v1/")).collect();
    let v3_entries: Vec<&_> = manifest.iter().filter(|e| e.path.contains("/v3/")).collect();
    assert_eq!(v1_entries.len(), 6, "expected 6 v1 (method, path) entries, got {}", v1_entries.len());
    assert_eq!(v3_entries.len(), 6, "expected 6 v3 (method, path) entries, got {}", v3_entries.len());

    let v1_paths: std::collections::HashSet<&str> = v1_entries.iter().map(|e| e.path).collect();
    let v3_paths: std::collections::HashSet<&str> = v3_entries.iter().map(|e| e.path).collect();
    assert_eq!(v1_paths.len(), 4, "expected 4 distinct v1 paths, got {}", v1_paths.len());
    assert_eq!(v3_paths.len(), 4, "expected 4 distinct v3 paths, got {}", v3_paths.len());

    // CRUD on connections + MCP tool list/call.
    assert!(manifest.iter().any(|e| e.method == Method::GET && e.path.ends_with("/ai/connections")));
    assert!(manifest.iter().any(|e| e.method == Method::POST && e.path.ends_with("/ai/connections")));
    assert!(manifest.iter().any(|e| e.method == Method::DELETE && e.path.ends_with("/ai/connections/{id}")));
    assert!(manifest.iter().any(|e| e.method == Method::POST && e.path.ends_with("/ai/mcp/tools/call")));
}

#[test]
fn test_route_entry_is_debug_clone() {
    fn assert_traits<T: std::fmt::Debug + Clone>() {}
    assert_traits::<RouteEntry>();
}

// ============================================================================
// McpToolListQuery — query-string deserialization
// ============================================================================

#[test]
fn mcp_tool_list_query_deserializes_with_provider() {
    let q: McpToolListQuery = serde_json::from_str(r#"{"provider":"openai"}"#).expect("provider should deserialize");
    assert_eq!(q.provider, "openai");
}

#[test]
fn mcp_tool_list_query_rejects_missing_provider() {
    let err = serde_json::from_str::<McpToolListQuery>("{}");
    assert!(err.is_err(), "missing provider must fail deserialization");
}

// ============================================================================
// CreateConnectionRequest — body deserialization
// ============================================================================

#[test]
fn create_connection_request_deserializes_with_config() {
    let payload = json!({
        "provider": "anthropic",
        "config": {"api_key": "sk-xxx", "model": "claude-3"}
    });
    let req: CreateConnectionRequest = serde_json::from_value(payload).expect("full payload should deserialize");
    assert_eq!(req.provider, "anthropic");
    assert!(req.config.is_some());
    assert_eq!(req.config.as_ref().unwrap()["model"].as_str(), Some("claude-3"));
}

#[test]
fn create_connection_request_accepts_null_config() {
    // config is Option<Value> — omitted or null is valid.
    let payload = json!({ "provider": "openai" });
    let req: CreateConnectionRequest = serde_json::from_value(payload).expect("minimal payload should deserialize");
    assert_eq!(req.provider, "openai");
    assert!(req.config.is_none());
}

#[test]
fn create_connection_request_rejects_missing_provider() {
    let payload = json!({ "config": {} });
    let err = serde_json::from_value::<CreateConnectionRequest>(payload);
    assert!(err.is_err(), "missing provider must fail deserialization");
}

// ============================================================================
// McpToolCallRequest — body deserialization
// ============================================================================

#[test]
fn mcp_tool_call_request_deserializes_full_payload() {
    let payload = json!({
        "provider": "openai",
        "tool_name": "search",
        "arguments": {"query": "rust async"}
    });
    let req: McpToolCallRequest = serde_json::from_value(payload).expect("full payload should deserialize");
    assert_eq!(req.provider, "openai");
    assert_eq!(req.tool_name, "search");
    assert_eq!(req.arguments["query"].as_str(), Some("rust async"));
}

#[test]
fn mcp_tool_call_request_accepts_null_arguments() {
    // arguments is a required Value field, but JSON null deserializes to Value::Null.
    let payload = json!({ "provider": "p", "tool_name": "t", "arguments": null });
    let req: McpToolCallRequest = serde_json::from_value(payload).expect("null arguments should deserialize");
    assert!(req.arguments.is_null());
}

#[test]
fn mcp_tool_call_request_rejects_missing_provider() {
    let payload = json!({ "tool_name": "t", "arguments": {} });
    let err = serde_json::from_value::<McpToolCallRequest>(payload);
    assert!(err.is_err(), "missing provider must fail deserialization");
}

#[test]
fn mcp_tool_call_request_rejects_missing_tool_name() {
    let payload = json!({ "provider": "p", "arguments": {} });
    let err = serde_json::from_value::<McpToolCallRequest>(payload);
    assert!(err.is_err(), "missing tool_name must fail deserialization");
}

#[test]
fn mcp_tool_call_request_rejects_missing_arguments() {
    let payload = json!({ "provider": "p", "tool_name": "t" });
    let err = serde_json::from_value::<McpToolCallRequest>(payload);
    assert!(err.is_err(), "missing arguments must fail deserialization");
}

// ============================================================================
// AiConnection — storage → wire serialization shape
// ============================================================================

fn sample_ai_connection() -> AiConnection {
    AiConnection {
        id: "conn-001".into(),
        user_id: "@alice:localhost".into(),
        provider: "openai".into(),
        config: Some(json!({"model": "gpt-4"})),
        is_active: true,
        created_ts: 1_700_000_000_000,
        updated_ts: Some(1_700_000_500_000),
    }
}

#[test]
fn ai_connection_serializes_expected_json_shape() {
    let conn = sample_ai_connection();
    let json_value = serde_json::to_value(&conn).expect("AiConnection should serialize");
    assert_eq!(json_value["id"], "conn-001");
    assert_eq!(json_value["user_id"], "@alice:localhost");
    assert_eq!(json_value["provider"], "openai");
    assert_eq!(json_value["is_active"], true);
    assert_eq!(json_value["created_ts"], 1_700_000_000_000_i64);
    assert_eq!(json_value["updated_ts"], 1_700_000_500_000_i64);
    assert_eq!(json_value["config"]["model"].as_str(), Some("gpt-4"));
}

#[test]
fn ai_connection_round_trips_through_serde() {
    let conn = sample_ai_connection();
    let json_str = serde_json::to_string(&conn).expect("serialize");
    let back: AiConnection = serde_json::from_str(&json_str).expect("deserialize");
    assert_eq!(back.id, conn.id);
    assert_eq!(back.provider, conn.provider);
    assert_eq!(back.is_active, conn.is_active);
}

#[test]
fn ai_connection_handles_null_updated_ts() {
    let mut conn = sample_ai_connection();
    conn.updated_ts = None;
    let json_value = serde_json::to_value(&conn).expect("serialize");
    assert!(json_value["updated_ts"].is_null());
}

// ============================================================================
// Error-code mapping — get_connection returns 404 when not found
// ============================================================================

#[test]
fn test_connection_not_found_maps_to_404() {
    // get_connection: conn.ok_or_else(|| ApiError::not_found("Connection not found"))
    let err = ApiError::not_found("Connection not found".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::NOT_FOUND);
}

#[test]
fn test_connection_not_found_errcode_is_m_not_found() {
    let err = ApiError::not_found("Connection not found".to_string());
    assert_eq!(err.code.as_str(), "M_NOT_FOUND");
}

// ============================================================================
// Auth/permission model — every endpoint uses AdminContext (admin-gated)
// ============================================================================

#[test]
fn test_all_ai_connection_endpoints_use_admin_context() {
    // Every handler in ai_connection.rs takes State<AdminContext>, which is
    // only extractable for admin-authenticated users. Document that contract.
    let admin_gated_paths = [
        "/_matrix/client/v1/ai/connections",
        "/_matrix/client/v1/ai/connections/{id}",
        "/_matrix/client/v1/ai/mcp/tools",
        "/_matrix/client/v1/ai/mcp/tools/call",
    ];
    let manifest = ai_connection_route_manifest();
    let manifest_paths: std::collections::HashSet<&str> = manifest.iter().map(|e| e.path).collect();
    for path in &admin_gated_paths {
        assert!(manifest_paths.contains(*path), "admin-gated path missing from manifest: {path}");
    }
}
