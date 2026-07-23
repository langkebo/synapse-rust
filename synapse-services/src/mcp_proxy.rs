use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use synapse_cache::CacheManager;
use synapse_common::error::{ApiError, ApiResult};

/// Trait abstraction over [`McpProxyService`] for testability.
///
/// Exposes the MCP operations consumed by `MatrixAiConnectionService` so
/// that the AI-connection layer can be unit-tested without spinning up a
/// real MCP HTTP endpoint.
#[async_trait::async_trait]
pub trait McpProxyServiceApi: Send + Sync {
    /// 发现 TrendRadar/OpenClaw 等支持的工具列表
    async fn list_tools(&self, mcp_url: &str) -> Result<Value, ApiError>;

    /// 调用 MCP 的具体工具（如获取热榜、搜索新闻）
    async fn call_tool(
        &self,
        mcp_url: &str,
        tool_name: &str,
        arguments: Value,
        provider: &str,
        user_id: &str,
    ) -> Result<Value, ApiError>;
}

/// MCP 请求代理服务
#[derive(Clone)]
pub struct McpProxyService {
    client: Client,
    cache: Arc<CacheManager>,
}

impl McpProxyService {
    pub fn new(cache: Arc<CacheManager>) -> Self {
        // 创建带有超时控制的 HTTP Client
        let client = Client::builder()
            .timeout(Duration::from_secs(30)) // 增加超时时间以适应大模型生成或爬虫
            .build()
            .unwrap_or_else(|e| {
                error!(error = %e, timeout_secs = 30_u64, "Failed to build HTTP client for McpProxyService");
                Client::new()
            });

        Self { client, cache }
    }

    /// 发现 TrendRadar/OpenClaw 等支持的工具列表
    pub async fn list_tools(&self, endpoint: &str) -> ApiResult<Value> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": uuid::Uuid::new_v4().to_string()
        });

        self.send_mcp_request(endpoint, payload).await
    }

    /// 调用 MCP 的具体工具（如获取热榜、搜索新闻）
    pub async fn call_tool(
        &self,
        endpoint: &str,
        tool_name: &str,
        args: Value,
        provider: &str,
        _user_id: &str,
    ) -> ApiResult<Value> {
        // 只有 TrendRadar 的查询类工具才需要缓存 (比如获取热榜/新闻)
        // 使用 SHA-256 或字符串哈希对 args 建立唯一键
        let args_str = serde_json::to_string(&args).unwrap_or_default();
        let is_cacheable =
            provider == "trendradar" && (tool_name == "get_latest_news" || tool_name == "get_trending_topics");

        let cache_key = if is_cacheable {
            // 对 args_str 简单 hash 避免 key 过长
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            args_str.hash(&mut hasher);
            let args_hash = hasher.finish();

            Some(format!("mcp_tool:{}:{}:{}", provider, tool_name, args_hash))
        } else {
            None
        };

        // 尝试命中缓存
        if let Some(ref key) = cache_key {
            match self.cache.get::<Value>(key).await {
                Ok(Some(cached_val)) => {
                    info!(tool_name = %tool_name, provider = %provider, cache_hit = true, "MCP tool call hit cache");
                    return Ok(cached_val);
                }
                Ok(None) => {} // Cache miss
                Err(e) => warn!(
                    error = %e,
                    provider = %provider,
                    tool_name = %tool_name,
                    cache_key = cache_key.as_deref(),
                    "Failed to read cache for MCP tool call"
                ),
            }
        }

        // 发起实际请求
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args
            },
            "id": uuid::Uuid::new_v4().to_string()
        });

        let result = self.send_mcp_request(endpoint, payload).await?;

        // 写入缓存 (10分钟 / 600秒 过期)
        if let Some(ref key) = cache_key {
            if let Err(e) = self.cache.set(key, &result, 600).await {
                warn!(
                    error = %e,
                    provider = %provider,
                    tool_name = %tool_name,
                    cache_key = %key,
                    cache_ttl_secs = 600_u64,
                    "Failed to write MCP result to cache"
                );
            } else {
                info!(
                    tool_name = %tool_name,
                    provider = %provider,
                    cache_ttl_secs = 600_u64,
                    "Cached MCP tool call result"
                );
            }
        }

        Ok(result)
    }

    /// 底层发送 MCP 协议格式请求 (JSON-RPC)
    async fn send_mcp_request(&self, endpoint: &str, payload: Value) -> ApiResult<Value> {
        if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
            return Err(ApiError::bad_request("MCP endpoint must use HTTP(S) protocol".to_string()));
        }

        let host = endpoint
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");

        if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "0.0.0.0" {
            return Err(ApiError::bad_request("MCP endpoint cannot point to loopback address".to_string()));
        }

        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            match ip {
                std::net::IpAddr::V4(ip) => {
                    if ip.is_private() || ip.is_link_local() || ip.is_loopback() {
                        return Err(ApiError::bad_request(
                            "MCP endpoint cannot point to private/local address".to_string(),
                        ));
                    }
                    let octets = ip.octets();
                    if octets[0] == 169 && octets[1] == 254 {
                        return Err(ApiError::bad_request(
                            "MCP endpoint cannot point to link-local metadata address".to_string(),
                        ));
                    }
                }
                std::net::IpAddr::V6(ip) => {
                    if ip.is_loopback() {
                        return Err(ApiError::bad_request("MCP endpoint cannot point to loopback address".to_string()));
                    }
                }
            }
        }

        info!(
            has_endpoint = !endpoint.is_empty(),
            method = payload.get("method").and_then(|value| value.as_str()).unwrap_or("unknown"),
            has_params = payload.get("params").is_some(),
            "Sending MCP request"
        );

        let response =
            self.client.post(endpoint).header("Content-Type", "application/json").json(&payload).send().await.map_err(
                |e| {
                    error!(error = %e, has_endpoint = !endpoint.is_empty(), "MCP request failed");
                    ApiError::internal_with_log("Failed to connect to MCP server", &e)
                },
            )?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!(
                status = %status,
                has_endpoint = !endpoint.is_empty(),
                response_body_present = !error_text.is_empty(),
                response_body_len = error_text.len(),
                "MCP server returned error"
            );
            return Err(ApiError::internal_with_log("MCP server error", &error_text));
        }

        let result: Value = response.json().await.map_err(|e| {
            error!(error = %e, has_endpoint = !endpoint.is_empty(), "Failed to parse MCP response");
            ApiError::internal("Invalid JSON response from MCP server")
        })?;

        // 检查 JSON-RPC 错误
        if let Some(err) = result.get("error") {
            warn!(
                has_endpoint = !endpoint.is_empty(),
                error_code = err.get("code").and_then(|value| value.as_i64()),
                error_message_present = err.get("message").is_some(),
                error_data_present = err.get("data").is_some(),
                "MCP tool execution error"
            );
            return Err(ApiError::internal_with_log("MCP error", &err));
        }

        Ok(result)
    }

    /// 对代理服务进行简单的健康检查
    pub async fn check_health(&self, endpoint: &str) -> bool {
        match self.client.get(endpoint).timeout(Duration::from_secs(5)).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}

#[async_trait::async_trait]
impl McpProxyServiceApi for McpProxyService {
    async fn list_tools(&self, mcp_url: &str) -> Result<Value, ApiError> {
        // Delegate to the inherent method (kept for direct callers).
        McpProxyService::list_tools(self, mcp_url).await
    }

    async fn call_tool(
        &self,
        mcp_url: &str,
        tool_name: &str,
        arguments: Value,
        provider: &str,
        user_id: &str,
    ) -> Result<Value, ApiError> {
        McpProxyService::call_tool(self, mcp_url, tool_name, arguments, provider, user_id).await
    }
}
