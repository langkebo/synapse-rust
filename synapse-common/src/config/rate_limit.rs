use serde::Deserialize;
use std::collections::HashMap;

/// 限流配置（homeserver.yaml 的 `rate_limit` 段，运行时视图）。
///
/// B-1：限流叶子类型全仓只有一份定义，在 `crate::rate_limit_config`
/// （支持热更新的权威实现）；此处仅 re-export，避免两套同名类型漂移。
/// 本模块只保留外层 `RateLimitConfig`——它是主配置文件的反序列化视图，
/// 字段集与热更新文件（`RateLimitConfigFile`，多 `backend` /
/// `reload_interval_seconds` 等）不同，因此两个顶层 struct 各自保留。
pub use crate::rate_limit_config::{RateLimitEndpointRule, RateLimitMatchType, RateLimitRule};
pub use crate::rate_limit_config::SyncRateLimitConfigFile as SyncRateLimitConfig;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_rule_default() {
        let rule = RateLimitRule::default();
        assert_eq!(rule.per_second, 10);
        assert_eq!(rule.burst_size, 20);
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert!(config.enabled);
        assert_eq!(config.default.per_second, 10);
        assert_eq!(config.default.burst_size, 20);
        assert!(config.include_headers);
        assert!(!config.fail_open_on_error);
        assert!(config.endpoints.is_empty());
        assert!(config.exempt_path_prefixes.is_empty());
        assert!(config.endpoint_aliases.is_empty());
        assert!(config.trusted_proxies.is_empty());
        assert!(!config.trust_forwarded);
    }

    #[test]
    fn test_rate_limit_config_ip_headers() {
        let config = RateLimitConfig::default();
        assert_eq!(config.ip_header_priority.len(), 3);
        assert!(config.ip_header_priority.contains(&"x-forwarded-for".to_string()));
        assert!(config.ip_header_priority.contains(&"x-real-ip".to_string()));
        assert!(config.ip_header_priority.contains(&"forwarded".to_string()));
    }

    #[test]
    fn test_rate_limit_config_exempt_paths() {
        let config = RateLimitConfig::default();
        assert_eq!(config.exempt_paths.len(), 3);
        assert!(config.exempt_paths.contains(&"/".to_string()));
        assert!(config.exempt_paths.contains(&"/_matrix/client/versions".to_string()));
        assert!(config.exempt_paths.contains(&"/_matrix/client/v3/versions".to_string()));
    }

    #[test]
    fn test_sync_rate_limit_config_default() {
        let sync = SyncRateLimitConfig::default();
        assert!(!sync.enabled);
        assert_eq!(sync.initial.per_second, 10);
        assert_eq!(sync.initial.burst_size, 20);
        assert_eq!(sync.incremental.per_second, 10);
        assert_eq!(sync.incremental.burst_size, 20);
    }

    #[test]
    fn test_rate_limit_match_type_default() {
        let match_type = RateLimitMatchType::default();
        assert!(matches!(match_type, RateLimitMatchType::Exact));
    }

    /// B-1: 叶子类型必须是同一类型（re-export），而非两份定义。
    #[test]
    fn test_leaf_types_are_single_source() {
        fn assert_same<T>(_: &T, _: &T) {}
        let rule = RateLimitRule::default();
        let file_rule = crate::rate_limit_config::RateLimitRule::default();
        assert_same(&rule, &file_rule);
        let sync = SyncRateLimitConfig::default();
        let file_sync = crate::rate_limit_config::SyncRateLimitConfigFile::default();
        assert_same(&sync, &file_sync);
    }
}
