use serde::Deserialize;
use std::collections::HashMap;

/// 限流配置。
///
/// 配置 API 请求限流规则，包括全局限流和端点级限流。

// ============================================================================
// SECTION: Rate Limiting
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    /// 是否启用限流
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
    /// 默认限流规则
    #[serde(default)]
    pub default: RateLimitRule,
    /// 端点级限流规则
    #[serde(default)]
    pub endpoints: Vec<RateLimitEndpointRule>,
    /// IP 头优先级列表
    #[serde(default)]
    pub ip_header_priority: Vec<String>,
    /// 是否包含请求头进行限流判断
    #[serde(default)]
    pub include_headers: bool,
    /// 豁免路径列表
    #[serde(default)]
    pub exempt_paths: Vec<String>,
    /// 豁免路径前缀列表
    #[serde(default)]
    pub exempt_path_prefixes: Vec<String>,
    /// 端点别名映射
    #[serde(default)]
    pub endpoint_aliases: HashMap<String, String>,
    /// 错误时是否开放访问
    #[serde(default = "default_rate_limit_fail_open")]
    pub fail_open_on_error: bool,
    /// 同步接口的资源隔离限流（initial vs incremental）
    #[serde(default)]
    pub sync: SyncRateLimitConfig,
    /// CIDR strings for trusted reverse proxies (e.g. "10.0.0.0/8", "127.0.0.1/32").
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    /// Whether to trust forwarded headers at all.
    #[serde(default)]
    pub trust_forwarded: bool,
}

fn default_rate_limit_enabled() -> bool {
    true
}

fn default_rate_limit_fail_open() -> bool {
    false
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SyncRateLimitConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub initial: RateLimitRule,
    #[serde(default)]
    pub incremental: RateLimitRule,
}

/// 单个限流规则。
///
/// 定义令牌桶算法的参数：每秒补充令牌数和桶容量。
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitRule {
    /// 每秒允许的请求数
    #[serde(default = "default_rate_limit_per_second")]
    pub per_second: u32,
    /// 令牌桶容量（突发请求数）
    #[serde(default = "default_rate_limit_burst_size")]
    pub burst_size: u32,
}

fn default_rate_limit_per_second() -> u32 {
    10
}

fn default_rate_limit_burst_size() -> u32 {
    20
}

impl Default for RateLimitRule {
    fn default() -> Self {
        Self { per_second: default_rate_limit_per_second(), burst_size: default_rate_limit_burst_size() }
    }
}

/// 端点级限流规则。
///
/// 为特定 API 路径配置独立的限流参数。
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitEndpointRule {
    /// 匹配的路径
    pub path: String,
    /// 路径匹配类型
    #[serde(default)]
    pub match_type: RateLimitMatchType,
    /// 该路径的限流规则
    pub rule: RateLimitRule,
}

/// 路径匹配类型。
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitMatchType {
    /// 精确匹配
    #[default]
    Exact,
    /// 前缀匹配
    Prefix,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_rate_limit_enabled(),
            default: RateLimitRule::default(),
            endpoints: Vec::new(),
            ip_header_priority: vec!["x-forwarded-for".to_string(), "x-real-ip".to_string(), "forwarded".to_string()],
            include_headers: true,
            exempt_paths: vec![
                "/".to_string(),
                "/_matrix/client/versions".to_string(),
                "/_matrix/client/v3/versions".to_string(),
            ],
            exempt_path_prefixes: Vec::new(),
            endpoint_aliases: HashMap::new(),
            fail_open_on_error: default_rate_limit_fail_open(),
            sync: SyncRateLimitConfig::default(),
            trusted_proxies: Vec::new(),
            trust_forwarded: false,
        }
    }
}
