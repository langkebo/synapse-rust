use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use crate::cache::CacheManager;
use crate::common::error::{ApiError, ApiResult};

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
                error!("Failed to build HTTP client for McpProxyService: {}", e);
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
        let is_cacheable = provider == "trendradar"
            && (tool_name == "get_latest_news" || tool_name == "get_trending_topics");

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
                    info!("MCP tool call {} hit cache.", tool_name);
                    return Ok(cached_val);
                }
                Ok(None) => {} // Cache miss
                Err(e) => warn!("Failed to read cache for MCP tool call: {}", e),
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
                warn!("Failed to write MCP result to cache: {}", e);
            } else {
                info!("MCP tool call {} cached successfully for 600s.", tool_name);
            }
        }

        Ok(result)
    }

    /// 底层发送 MCP 协议格式请求 (JSON-RPC)
    async fn send_mcp_request(&self, endpoint: &str, payload: Value) -> ApiResult<Value> {
        info!("Sending MCP request to {}: {}", endpoint, payload);

        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                error!("MCP request failed: {}", e);
                ApiError::internal(format!("Failed to connect to MCP server: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("MCP server returned error {}: {}", status, error_text);
            return Err(ApiError::internal(format!(
                "MCP server error: {}",
                error_text
            )));
        }

        let result: Value = response.json().await.map_err(|e| {
            error!("Failed to parse MCP response: {}", e);
            ApiError::internal("Invalid JSON response from MCP server")
        })?;

        // 检查 JSON-RPC 错误
        if let Some(err) = result.get("error") {
            warn!("MCP tool execution error: {}", err);
            return Err(ApiError::internal(format!("MCP error: {}", err)));
        }

        Ok(result)
    }

    /// 对代理服务进行简单的健康检查
    pub async fn check_health(&self, endpoint: &str) -> bool {
        match self
            .client
            .get(endpoint)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}
