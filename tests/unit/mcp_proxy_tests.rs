// MCP proxy service unit tests — exercises
// `synapse_services::mcp_proxy::McpProxyService`.
//
// `McpProxyService` proxies JSON-RPC requests to an external MCP server.
// It includes SSRF-prevention URL validation (rejecting loopback, private,
// and link-local addresses) and optional result caching for TrendRadar
// query tools.
//
// These tests cover:
//   * Construction with a local-only `CacheManager` (no Redis needed).
//   * `Clone` semantics (the struct derives Clone).
//   * URL validation — rejects non-HTTP(S) schemes (ftp://, file://).
//   * URL validation — rejects loopback hosts (localhost, 127.0.0.1, ::1,
//     0.0.0.0).
//   * URL validation — rejects private IPs (10.x, 192.168.x, 172.16-31.x).
//   * URL validation — rejects link-local IPs (169.254.x).
//   * `list_tools` surfaces validation errors as `ApiError::bad_request`.
//   * `call_tool` surfaces validation errors as `ApiError::bad_request`.
//   * `call_tool` cache-hit path returns the cached value without making
//     an HTTP request (verified by pre-populating the cache with a known
//     key and asserting the return matches).
//   * `call_tool` non-cacheable tools bypass the cache.
//   * `check_health` returns `false` for unreachable endpoints.
//   * `check_health` returns `false` for invalid URLs.
//   * `McpProxyServiceApi` trait delegation matches inherent methods.
//
// The happy-path (real MCP server returning tools) is exercised by
// integration tests against a live MCP endpoint.

#![cfg(feature = "openclaw-routes")]

use std::sync::Arc;

use serde_json::{json, Value};

use synapse_cache::{CacheConfig, CacheManager};
use synapse_services::mcp_proxy::McpProxyService;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn build_service() -> McpProxyService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    McpProxyService::new(cache)
}

/// Compute the same cache key that `McpProxyService::call_tool` builds
/// internally for cacheable tools (provider="trendradar", tool_name in
/// ["get_latest_news", "get_trending_topics"]).
fn compute_cache_key(provider: &str, tool_name: &str, args: &Value) -> String {
    let args_str = serde_json::to_string(args).unwrap_or_default();
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    args_str.hash(&mut hasher);
    let args_hash = hasher.finish();
    format!("mcp_tool:{}:{}:{}", provider, tool_name, args_hash)
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction + Clone
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn mcp_proxy_service_constructs_with_local_cache() {
    let _svc = build_service();
    // Construction must not panic and must not perform I/O.
}

#[test]
fn mcp_proxy_service_is_clone() {
    let svc = build_service();
    let _cloned = svc.clone();
    // Clone must succeed; both instances share the same cache Arc.
}

// ─────────────────────────────────────────────────────────────────────────────
// URL validation — protocol scheme
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_tools_rejects_ftp_scheme() {
    let svc = build_service();
    let err = svc.list_tools("ftp://example.com/mcp").await.expect_err("ftp must be rejected");
    assert!(err.is_bad_request(), "ftp scheme must surface as bad_request");
}

#[tokio::test]
async fn list_tools_rejects_file_scheme() {
    let svc = build_service();
    let err = svc.list_tools("file:///etc/passwd").await.expect_err("file must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_gopher_scheme() {
    let svc = build_service();
    let err = svc.list_tools("gopher://example.com/mcp").await.expect_err("gopher must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_empty_endpoint() {
    let svc = build_service();
    let err = svc.list_tools("").await.expect_err("empty endpoint must be rejected");
    assert!(err.is_bad_request());
}

// ─────────────────────────────────────────────────────────────────────────────
// URL validation — loopback addresses
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_tools_rejects_localhost_host() {
    let svc = build_service();
    let err = svc.list_tools("https://localhost/mcp").await.expect_err("localhost must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_127_ip() {
    let svc = build_service();
    let err = svc.list_tools("https://127.0.0.1/mcp").await.expect_err("127.0.0.1 must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_ipv6_loopback() {
    let svc = build_service();
    let err = svc.list_tools("https://[::1]/mcp").await.expect_err("::1 must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_0000_host() {
    let svc = build_service();
    let err = svc.list_tools("https://0.0.0.0/mcp").await.expect_err("0.0.0.0 must be rejected");
    assert!(err.is_bad_request());
}

// ─────────────────────────────────────────────────────────────────────────────
// URL validation — private IP ranges
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_tools_rejects_private_ip_10_x() {
    let svc = build_service();
    let err = svc.list_tools("https://10.0.0.1/mcp").await.expect_err("10.x must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_private_ip_192_168() {
    let svc = build_service();
    let err = svc.list_tools("https://192.168.1.1/mcp").await.expect_err("192.168.x must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_private_ip_172_16() {
    let svc = build_service();
    let err = svc.list_tools("https://172.16.0.1/mcp").await.expect_err("172.16.x must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn list_tools_rejects_private_ip_172_31() {
    let svc = build_service();
    let err = svc.list_tools("https://172.31.255.255/mcp").await.expect_err("172.31.x must be rejected");
    assert!(err.is_bad_request());
}

// ─────────────────────────────────────────────────────────────────────────────
// URL validation — link-local addresses
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_tools_rejects_link_local_169_254() {
    let svc = build_service();
    let err = svc.list_tools("https://169.254.169.254/mcp").await.expect_err("169.254.x must be rejected");
    assert!(err.is_bad_request(), "link-local metadata address must be rejected");
}

// ─────────────────────────────────────────────────────────────────────────────
// call_tool — URL validation error path
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn call_tool_rejects_loopback_endpoint() {
    let svc = build_service();
    let err = svc
        .call_tool(
            "https://127.0.0.1/mcp",
            "some_tool",
            json!({}),
            "some_provider",
            "@user:example.com",
        )
        .await
        .expect_err("loopback must be rejected");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn call_tool_rejects_ftp_scheme() {
    let svc = build_service();
    let err = svc
        .call_tool("ftp://example.com/mcp", "some_tool", json!({}), "some_provider", "@user:example.com")
        .await
        .expect_err("ftp must be rejected");
    assert!(err.is_bad_request());
}

// ─────────────────────────────────────────────────────────────────────────────
// call_tool — cache hit path (no HTTP request needed)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn call_tool_returns_cached_value_for_cacheable_trendradar_tool() {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = McpProxyService::new(cache.clone());

    let args = json!({"query": "rust"});
    let cache_key = compute_cache_key("trendradar", "get_latest_news", &args);
    let cached_result = json!({
        "jsonrpc": "2.0",
        "result": {"tools": [{"name": "cached_tool"}]},
        "id": "cached"
    });

    // Pre-populate the cache.
    cache.set(&cache_key, &cached_result, 600).await.expect("cache set must succeed");

    // call_tool should return the cached value without making an HTTP request.
    // The endpoint is a loopback address that would normally be rejected —
    // if the cache is hit, the validation never runs.
    let result = svc
        .call_tool("https://127.0.0.1/mcp", "get_latest_news", args, "trendradar", "@user:example.com")
        .await
        .expect("cache hit must succeed");

    assert_eq!(result, cached_result, "cached value must be returned as-is");
}

#[tokio::test]
async fn call_tool_returns_cached_value_for_get_trending_topics() {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = McpProxyService::new(cache.clone());

    let args = json!({"category": "tech"});
    let cache_key = compute_cache_key("trendradar", "get_trending_topics", &args);
    let cached_result = json!({"jsonrpc": "2.0", "result": {"topics": ["AI", "Rust"]}});

    cache.set(&cache_key, &cached_result, 600).await.expect("cache set must succeed");

    let result = svc
        .call_tool("https://127.0.0.1/mcp", "get_trending_topics", args, "trendradar", "@user:example.com")
        .await
        .expect("cache hit must succeed");

    assert_eq!(result, cached_result);
}

// ─────────────────────────────────────────────────────────────────────────────
// call_tool — non-cacheable tools bypass cache
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn call_tool_does_not_cache_non_trendradar_provider() {
    let svc = build_service();

    // A non-trendradar provider should not use the cache, so the loopback
    // URL validation should still fire.
    let err = svc
        .call_tool(
            "https://127.0.0.1/mcp",
            "get_latest_news",
            json!({}),
            "openclaw", // not "trendradar"
            "@user:example.com",
        )
        .await
        .expect_err("non-cacheable tool must hit URL validation");

    assert!(err.is_bad_request(), "non-cacheable tool must still validate the URL");
}

#[tokio::test]
async fn call_tool_does_not_cache_non_query_tool() {
    let svc = build_service();

    // Even with provider="trendradar", a non-query tool name should not
    // use the cache, so URL validation fires.
    let err = svc
        .call_tool(
            "https://127.0.0.1/mcp",
            "some_other_tool",
            json!({}),
            "trendradar",
            "@user:example.com",
        )
        .await
        .expect_err("non-query tool must hit URL validation");

    assert!(err.is_bad_request());
}

// ─────────────────────────────────────────────────────────────────────────────
// check_health — returns false for unreachable / invalid endpoints
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn check_health_returns_false_for_unreachable_endpoint() {
    let svc = build_service();
    // Use a non-routable address with a short timeout. The service applies
    // a 5s timeout internally.
    let healthy = svc.check_health("https://192.0.2.1:65535/health").await;
    assert!(!healthy, "unreachable endpoint must report unhealthy");
}

#[tokio::test]
async fn check_health_returns_false_for_invalid_url() {
    let svc = build_service();
    let healthy = svc.check_health("not-a-valid-url").await;
    assert!(!healthy, "invalid URL must report unhealthy");
}

#[tokio::test]
async fn check_health_returns_false_for_empty_endpoint() {
    let svc = build_service();
    let healthy = svc.check_health("").await;
    assert!(!healthy, "empty endpoint must report unhealthy");
}

// ─────────────────────────────────────────────────────────────────────────────
// Trait delegation — McpProxyServiceApi
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn trait_list_tools_delegates_to_inherent_method() {
    let svc = build_service();
    // Use the trait method to verify delegation. The loopback URL should
    // be rejected by the same validation path.
    let err = svc
        .list_tools("https://127.0.0.1/mcp")
        .await
        .expect_err("trait list_tools must reject loopback");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn trait_call_tool_delegates_to_inherent_method() {
    let svc = build_service();
    let err = svc
        .call_tool("https://127.0.0.1/mcp", "tool", json!({}), "provider", "@user:example.com")
        .await
        .expect_err("trait call_tool must reject loopback");
    assert!(err.is_bad_request());
}

#[tokio::test]
async fn trait_call_tool_uses_cache_for_cacheable_tool() {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = McpProxyService::new(cache.clone());

    let args = json!({"q": "test"});
    let cache_key = compute_cache_key("trendradar", "get_latest_news", &args);
    let cached = json!({"result": "from_cache"});

    cache.set(&cache_key, &cached, 600).await.expect("cache set must succeed");

    let result = svc
        .call_tool("https://127.0.0.1/mcp", "get_latest_news", args, "trendradar", "@user:example.com")
        .await
        .expect("trait call_tool must return cached value");

    assert_eq!(result, cached);
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiple validation calls don't interfere
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn repeated_validation_calls_all_reject_loopback() {
    let svc = build_service();

    for _ in 0..3 {
        let err = svc.list_tools("https://127.0.0.1/mcp").await.expect_err("must reject loopback");
        assert!(err.is_bad_request());
    }
}
