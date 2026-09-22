use serde::Deserialize;
use std::collections::HashMap;

/// Re-exported item.
pub use crate::rate_limit_config::SyncRateLimitConfigFile as SyncRateLimitConfig;
/// 限流配置（homeserver.yaml 的 `rate_limit` 段，运行时视图）。
///
/// B-1：限流叶子类型全仓只有一份定义，在 `crate::rate_limit_config`
/// （支持热更新的权威实现）；此处仅 re-export，避免两套同名类型漂移。
/// 本模块只保留外层 `RateLimitConfig`——它是主配置文件的反序列化视图，
/// 字段集与热更新文件（`RateLimitConfigFile`，多 `backend` /
/// `reload_interval_seconds` 等）不同，因此两个顶层 struct 各自保留。
pub use crate::rate_limit_config::{RateLimitEndpointRule, RateLimitMatchType, RateLimitRule};

// ============================================================================
// SECTION: Rate Limiting
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
/// Represents RateLimitConfig.
///
/// `deny_unknown_fields`: this is the `rate_limit:` section of
/// `homeserver.yaml`. The leaf types (re-exported below) already reject
/// unknown keys; without this attribute a misspelled outer key (e.g.
/// `ip_header_priorty` for `ip_header_priority`) would be silently dropped,
/// silently weakening the rate limits the operator believes are in force.
/// Note: `backend` / `reload_interval_seconds` are file-only fields of
/// `RateLimitConfigFile` and deliberately do NOT appear here — writing them
/// in `homeserver.yaml` should now fail loudly rather than be ignored.
pub struct RateLimitConfig {
    /// 是否启用限流
    #[serde(default = "default_rate_limit_enabled")]
    /// `enabled` field.
    pub enabled: bool,
    /// 默认限流规则
    #[serde(default)]
    /// `default` field.
    pub default: RateLimitRule,
    /// 端点级限流规则
    #[serde(default)]
    /// `endpoints` field.
    pub endpoints: Vec<RateLimitEndpointRule>,
    /// IP 头优先级列表
    #[serde(default)]
    /// `ip_header_priority` field.
    pub ip_header_priority: Vec<String>,
    /// 是否包含请求头进行限流判断
    #[serde(default)]
    /// `include_headers` field.
    pub include_headers: bool,
    /// 豁免路径列表
    #[serde(default)]
    /// `exempt_paths` field.
    pub exempt_paths: Vec<String>,
    /// 豁免路径前缀列表
    #[serde(default)]
    /// `exempt_path_prefixes` field.
    pub exempt_path_prefixes: Vec<String>,
    /// 端点别名映射
    #[serde(default)]
    /// `endpoint_aliases` field.
    pub endpoint_aliases: HashMap<String, String>,
    /// 错误时是否开放访问
    #[serde(default = "default_rate_limit_fail_open")]
    /// `fail_open_on_error` field.
    pub fail_open_on_error: bool,
    /// 同步接口的资源隔离限流（initial vs incremental）
    #[serde(default)]
    /// `sync` field.
    pub sync: SyncRateLimitConfig,
    /// CIDR strings for trusted reverse proxies (e.g. "10.0.0.0/8", "127.0.0.1/32").
    #[serde(default)]
    /// `trusted_proxies` field.
    pub trusted_proxies: Vec<String>,
    /// Whether to trust forwarded headers at all.
    #[serde(default)]
    /// `trust_forwarded` field.
    pub trust_forwarded: bool,
    /// Per-user limit for the report endpoints (rooms + users).
    ///
    /// Enforced in the handlers rather than by the path-based middleware, which
    /// only supports exact/prefix path rules and therefore cannot express
    /// `/_matrix/client/v3/rooms/{room_id}/report`.  Upstream #20036 applies the
    /// `rc_reports` limit to the room report endpoint.
    #[serde(default = "default_rc_reports")]
    /// `rc_reports` field.
    pub rc_reports: RateLimitRule,
}

/// Conservative default for the report endpoints: a user may file 10 reports in
/// a burst and then one per second.  Deliberately tight because reports are an
/// abuse vector; operators can raise it in `homeserver.yaml`.
fn default_rc_reports() -> RateLimitRule {
    RateLimitRule { per_second: 1, burst_size: 10 }
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
            rc_reports: default_rc_reports(),
        }
    }
}

/// Converts the `homeserver.yaml` runtime view into the hot-reloadable
/// file-backed config.
///
/// ## Why this exists
///
/// When `RATE_LIMIT_CONFIG_PATH` is missing or unparseable the server needs a
/// fallback. It used to build one from `RateLimitConfigFile::default()` —
/// **hard-coded** values — which silently discarded any `rate_limit:` section
/// the operator had written in `homeserver.yaml`. Neither file was honoured.
///
/// With this conversion the documented configuration is a real fallback, and
/// `RateLimitConfigFile` (the type the middleware actually consults) stays the
/// single in-memory representation.
///
/// `backend` and `reload_interval_seconds` have no counterpart in the runtime
/// view and therefore keep their own defaults.
impl From<&RateLimitConfig> for crate::rate_limit_config::RateLimitConfigFile {
    fn from(cfg: &RateLimitConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            backend: crate::rate_limit_config::RateLimitBackend::default(),
            default: cfg.default.clone(),
            endpoints: cfg.endpoints.clone(),
            ip_header_priority: cfg.ip_header_priority.clone(),
            include_headers: cfg.include_headers,
            exempt_paths: cfg.exempt_paths.clone(),
            exempt_path_prefixes: cfg.exempt_path_prefixes.clone(),
            endpoint_aliases: cfg.endpoint_aliases.clone(),
            fail_open_on_error: cfg.fail_open_on_error,
            sync: cfg.sync.clone(),
            reload_interval_seconds: crate::rate_limit_config::default_config_reload_interval(),
            trusted_proxies: cfg.trusted_proxies.clone(),
            trust_forwarded: cfg.trust_forwarded,
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
