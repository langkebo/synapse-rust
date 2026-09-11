//! Rate-limit config file types and hot-reload manager (see `RateLimitConfigManager`).

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::fs;

#[derive(Debug, Error)]
/// Errors emitted by rate-limit config loading and validation.
pub enum RateLimitConfigError {
    #[error("Failed to read config file: {0}")]
    /// `ReadError` variant.
    ReadError(#[source] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    /// `ParseError` variant.
    ParseError(#[source] serde_yaml::Error),
    #[error("Config validation error: {0}")]
    /// `ValidationError` variant.
    ValidationError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Token-bucket rule: sustained refill rate + peak burst capacity.
pub struct RateLimitRule {
    #[serde(default = "default_per_second")]
    /// Sustained refill rate (tokens per second).
    pub per_second: u32,
    #[serde(default = "default_burst_size")]
    /// Maximum tokens in the bucket (peak burst capacity).
    pub burst_size: u32,
}

fn default_per_second() -> u32 {
    10
}

fn default_burst_size() -> u32 {
    20
}

impl Default for RateLimitRule {
    fn default() -> Self {
        Self { per_second: default_per_second(), burst_size: default_burst_size() }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
/// How an endpoint path is compared to a rule's `path` field.
pub enum RateLimitMatchType {
    #[default]
    /// `Exact` variant.
    Exact,
    /// `Prefix` variant.
    Prefix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Per-path rate-limit override.
pub struct RateLimitEndpointRule {
    /// Endpoint path pattern.
    pub path: String,
    #[serde(default)]
    /// How `path` is matched: `exact` or `prefix`.
    pub match_type: RateLimitMatchType,
    /// Token-bucket rule applied to this endpoint.
    pub rule: RateLimitRule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
/// Which token-bucket implementation to use.
pub enum RateLimitBackend {
    /// Automatically use Redis when available, fall back to in-memory otherwise.
    #[default]
    /// `Auto` variant.
    Auto,
    /// Always use Redis; fail loudly if Redis is not available.
    Redis,
    /// Always use in-memory token bucket (single-worker mode only).
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Top-level YAML config for rate limiting.
pub struct RateLimitConfigFile {
    #[serde(default = "default_enabled")]
    /// Global on/off switch for rate limiting.
    pub enabled: bool,
    /// Rate-limit token-bucket backend: "auto" (default), "redis", or "local".
    ///
    /// - `auto`: use Redis when available, fall back to in-memory.
    /// - `redis`: require Redis; log an error and refuse requests if Redis is down.
    /// - `local`: always use in-memory (single-worker deployments only).
    #[serde(default)]
    pub backend: RateLimitBackend,
    #[serde(default)]
    /// Default rate-limit rule applied to any endpoint not matched by `endpoints`.
    pub default: RateLimitRule,
    #[serde(default)]
    /// Per-path overrides; first match wins.
    pub endpoints: Vec<RateLimitEndpointRule>,
    #[serde(default = "default_ip_header_priority")]
    /// Ordered list of HTTP headers to consult when resolving the client IP.
    pub ip_header_priority: Vec<String>,
    #[serde(default = "default_include_headers")]
    /// Whether to emit rate-limit headers (`X-RateLimit-*`) on responses.
    pub include_headers: bool,
    #[serde(default)]
    /// Paths that bypass rate limiting entirely (exact match).
    pub exempt_paths: Vec<String>,
    #[serde(default)]
    /// Path prefixes that bypass rate limiting.
    pub exempt_path_prefixes: Vec<String>,
    #[serde(default)]
    /// Map of alias → canonical endpoint path (so aliased paths can match `endpoints` rules).
    pub endpoint_aliases: HashMap<String, String>,
    #[serde(default)]
    /// If `true`, allow requests when the backend is unavailable. Default `false` (fail closed).
    pub fail_open_on_error: bool,
    #[serde(default)]
    /// `/sync`-specific rate-limit overrides (initial + incremental bursts).
    pub sync: SyncRateLimitConfigFile,
    #[serde(default = "default_config_reload_interval")]
    /// How often to hot-reload the YAML config file from disk.
    pub reload_interval_seconds: u64,
    /// CIDR strings for trusted reverse proxies (e.g. "10.0.0.0/8", "127.0.0.1/32").
    /// X-Forwarded-For / X-Real-IP / Forwarded headers are only trusted when the
    /// direct TCP peer address matches one of these networks.
    #[serde(default)]
    /// `trusted_proxies` field.
    pub trusted_proxies: Vec<String>,
    /// Whether to trust forwarded headers at all. When false (default), the peer
    /// address is always used regardless of the trusted_proxies list.
    #[serde(default = "default_trust_forwarded")]
    /// `trust_forwarded` field.
    pub trust_forwarded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
/// `/sync`-specific rate-limit overrides (initial + incremental bursts).
pub struct SyncRateLimitConfigFile {
    #[serde(default)]
    /// Enables `/sync` rate limiting.
    pub enabled: bool,
    #[serde(default)]
    /// Token-bucket for the very first `/sync` after login (warming up).
    pub initial: RateLimitRule,
    #[serde(default)]
    /// Token-bucket for subsequent incremental `/sync` calls.
    pub incremental: RateLimitRule,
}

fn default_enabled() -> bool {
    true
}

fn default_include_headers() -> bool {
    true
}

fn default_trust_forwarded() -> bool {
    false
}

fn default_ip_header_priority() -> Vec<String> {
    vec!["x-forwarded-for".to_string(), "x-real-ip".to_string(), "forwarded".to_string()]
}

fn default_config_reload_interval() -> u64 {
    30
}

impl Default for RateLimitConfigFile {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            backend: RateLimitBackend::default(),
            default: RateLimitRule::default(),
            endpoints: vec![
                RateLimitEndpointRule {
                    path: "/_matrix/client/v3/login".to_string(),
                    match_type: RateLimitMatchType::Prefix,
                    rule: RateLimitRule { per_second: 5, burst_size: 50 },
                },
                RateLimitEndpointRule {
                    path: "/_matrix/client/v3/register".to_string(),
                    match_type: RateLimitMatchType::Prefix,
                    rule: RateLimitRule { per_second: 1, burst_size: 10 },
                },
                RateLimitEndpointRule {
                    path: "/_matrix/client/v3/register/captcha".to_string(),
                    match_type: RateLimitMatchType::Prefix,
                    rule: RateLimitRule { per_second: 1, burst_size: 1 },
                },
            ],
            ip_header_priority: default_ip_header_priority(),
            include_headers: default_include_headers(),
            exempt_paths: vec!["/".to_string(), "/_matrix/client/versions".to_string()],
            exempt_path_prefixes: Vec::new(),
            endpoint_aliases: HashMap::new(),
            fail_open_on_error: false,
            sync: SyncRateLimitConfigFile::default(),
            reload_interval_seconds: default_config_reload_interval(),
            trusted_proxies: Vec::new(),
            trust_forwarded: default_trust_forwarded(),
        }
    }
}

impl RateLimitConfigFile {
    /// Validates this value.
    pub fn validate(&self) -> Result<(), RateLimitConfigError> {
        if self.default.per_second == 0 {
            return Err(RateLimitConfigError::ValidationError("default.per_second cannot be zero".to_string()));
        }
        if self.default.burst_size == 0 {
            return Err(RateLimitConfigError::ValidationError("default.burst_size cannot be zero".to_string()));
        }
        for (idx, endpoint) in self.endpoints.iter().enumerate() {
            if endpoint.path.is_empty() {
                return Err(RateLimitConfigError::ValidationError(format!("endpoints[{idx}].path cannot be empty")));
            }
            if endpoint.rule.per_second == 0 {
                return Err(RateLimitConfigError::ValidationError(format!(
                    "endpoints[{idx}].rule.per_second cannot be zero"
                )));
            }
            if endpoint.rule.burst_size == 0 {
                return Err(RateLimitConfigError::ValidationError(format!(
                    "endpoints[{idx}].rule.burst_size cannot be zero"
                )));
            }
        }
        Ok(())
    }

    /// Performs load.
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self, RateLimitConfigError> {
        let content = fs::read_to_string(path.as_ref()).await.map_err(RateLimitConfigError::ReadError)?;
        let config: Self = serde_yaml::from_str(&content).map_err(RateLimitConfigError::ParseError)?;
        config.validate()?;
        Ok(config)
    }

    /// Performs save.
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), RateLimitConfigError> {
        let content = serde_yaml::to_string(self).map_err(RateLimitConfigError::ParseError)?;
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).await.map_err(RateLimitConfigError::ReadError)?;
        }
        fs::write(path.as_ref(), content).await.map_err(RateLimitConfigError::ReadError)?;
        Ok(())
    }
}

/// Hot-reloading watcher that loads `RateLimitConfigFile` from disk and notifies subscribers.
pub struct RateLimitConfigManager {
    config: Arc<RwLock<RateLimitConfigFile>>,
    config_path: PathBuf,
    /// Reload health + config source, shared across `Arc` clones so the watcher
    /// and the health endpoint observe the same state.
    /// endpoint observe the same state.
    degradation: Arc<parking_lot::Mutex<RateLimitDegradation>>,
}

/// Where the currently-effective rate-limit configuration came from.
///
/// Exposed via [`RateLimitConfigManager::degradation`] so an operator can tell
/// "my config file is in effect" from "the file was missing/unparseable and the
/// built-in defaults silently took over".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Loaded from the path named by `RATE_LIMIT_CONFIG_PATH`.
    File,
    /// Fell back to [`RateLimitConfigFile::default`] because the file was
    /// missing or could not be parsed.
    Defaults,
}

impl ConfigSource {
    /// Stable label for metrics / health JSON. Renaming is a breaking change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Defaults => "defaults",
        }
    }
}

/// Snapshot of rate-limit configuration health.
///
/// A degraded manager keeps serving the **last-good** config (or the built-in
/// defaults) rather than failing closed, so the only way to notice is to
/// observe this state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitDegradation {
    /// Where the effective config came from.
    pub source: ConfigSource,
    /// Failed hot-reload attempts since the last success.
    pub consecutive_failures: u64,
    /// Failed hot-reload attempts over the process lifetime.
    pub total_failures: u64,
    /// Most recent failure message, if any.
    pub last_error: Option<String>,
}

impl RateLimitDegradation {
    /// True when the effective config is not the configured file, or a reload
    /// is currently failing.
    pub fn is_degraded(&self) -> bool {
        self.source == ConfigSource::Defaults || self.consecutive_failures > 0
    }
}

impl RateLimitConfigManager {
    /// Constructs a new instance.
    ///
    /// Marks the source as [`ConfigSource::Defaults`]: this constructor is the
    /// fallback path used when the config file is absent or unparseable.
    pub fn new(config: RateLimitConfigFile, config_path: PathBuf) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
            degradation: Arc::new(parking_lot::Mutex::new(RateLimitDegradation {
                source: ConfigSource::Defaults,
                consecutive_failures: 0,
                total_failures: 0,
                last_error: None,
            })),
        }
    }

    /// Constructs from file.
    pub async fn from_file<P: Into<PathBuf>>(path: P) -> Result<Self, RateLimitConfigError> {
        let path = path.into();
        let config = RateLimitConfigFile::load(&path).await?;
        let manager = Self {
            config: Arc::new(RwLock::new(config)),
            config_path: path,
            degradation: Arc::new(parking_lot::Mutex::new(RateLimitDegradation {
                source: ConfigSource::File,
                consecutive_failures: 0,
                total_failures: 0,
                last_error: None,
            })),
        };
        Ok(manager)
    }

    /// Returns the config.
    pub fn get_config(&self) -> RateLimitConfigFile {
        self.config.read().clone()
    }

    /// Returns the config ref.
    pub fn get_config_ref(&self) -> Arc<RwLock<RateLimitConfigFile>> {
        self.config.clone()
    }

    /// Returns the path the manager watches.
    pub fn config_path(&self) -> &std::path::Path {
        &self.config_path
    }

    /// Snapshot of configuration health for metrics / health endpoints.
    pub fn degradation(&self) -> RateLimitDegradation {
        self.degradation.lock().clone()
    }

    /// Performs reload.
    ///
    /// On success the degradation counters reset; on failure the previous
    /// config stays in effect and the failure is recorded (see
    /// [`RateLimitDegradation`]) instead of being visible only as a log line.
    pub async fn reload(&self) -> Result<(), RateLimitConfigError> {
        let loaded = RateLimitConfigFile::load(&self.config_path).await;
        let new_config = match loaded {
            Ok(config) => config,
            Err(e) => {
                let mut state = self.degradation.lock();
                state.consecutive_failures += 1;
                state.total_failures += 1;
                state.last_error = Some(e.to_string());
                return Err(e);
            }
        };
        {
            let mut config = self.config.write();
            *config = new_config;
        }
        {
            let mut state = self.degradation.lock();
            // A successful reload means the file is being honoured again, so the
            // source is File even if we booted on defaults.
            state.source = ConfigSource::File;
            state.consecutive_failures = 0;
            state.last_error = None;
        }
        tracing::info!("Rate limit configuration reloaded from {:?}", self.config_path);
        Ok(())
    }

    /// Performs update.
    pub async fn update<F>(&self, f: F) -> Result<(), RateLimitConfigError>
    where
        F: FnOnce(&mut RateLimitConfigFile),
    {
        let config_to_save = {
            let mut config = self.config.write();
            f(&mut config);
            config.validate()?;
            config.clone()
        };
        config_to_save.save(&self.config_path).await?;
        tracing::info!("Rate limit configuration updated and saved to {:?}", self.config_path);
        Ok(())
    }

    /// Sets the enabled.
    pub async fn set_enabled(&self, enabled: bool) -> Result<(), RateLimitConfigError> {
        self.update(|c| c.enabled = enabled).await
    }

    /// Sets the default rule.
    pub async fn set_default_rule(&self, rule: RateLimitRule) -> Result<(), RateLimitConfigError> {
        self.update(|c| c.default = rule).await
    }

    /// Adds the endpoint.
    pub async fn add_endpoint_rule(&self, rule: RateLimitEndpointRule) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.endpoints.push(rule);
        })
        .await
    }

    /// Removes the endpoint.
    pub async fn remove_endpoint_rule(&self, path: &str) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.endpoints.retain(|r| r.path != path);
        })
        .await
    }

    /// Adds the exempt.
    pub async fn add_exempt_path(&self, path: String) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            if !c.exempt_paths.contains(&path) {
                c.exempt_paths.push(path);
            }
        })
        .await
    }

    /// Removes the exempt.
    pub async fn remove_exempt_path(&self, path: &str) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.exempt_paths.retain(|p| p != path);
        })
        .await
    }
}

/// Shared longest-prefix rule selection over an endpoint list (B-1: single
/// implementation for both the hot-reload file config and the runtime
/// homeserver.yaml view — the leaf types are the same types).
fn select_rule(
    endpoints: &[RateLimitEndpointRule],
    endpoint_aliases: &HashMap<String, String>,
    default: &RateLimitRule,
    path: &str,
) -> (String, RateLimitRule) {
    let mut best_match: Option<&RateLimitEndpointRule> = None;
    let mut best_match_len = 0;

    for rule in endpoints {
        let is_match = match rule.match_type {
            RateLimitMatchType::Exact => rule.path == path,
            RateLimitMatchType::Prefix => path.starts_with(&rule.path),
        };

        if is_match && rule.path.len() > best_match_len {
            best_match = Some(rule);
            best_match_len = rule.path.len();
        }
    }

    match best_match {
        Some(rule) => {
            let endpoint_id = endpoint_aliases.get(&rule.path).cloned().unwrap_or_else(|| rule.path.clone());
            (endpoint_id, rule.rule.clone())
        }
        None => (path.to_string(), default.clone()),
    }
}

/// Selects the endpoint.
pub fn select_endpoint_rule(config: &RateLimitConfigFile, path: &str) -> (String, RateLimitRule) {
    select_rule(&config.endpoints, &config.endpoint_aliases, &config.default, path)
}

/// Same selection over the runtime `config::RateLimitConfig` view. The leaf
/// types are re-exports of this module's types, so this is a thin wrapper.
pub fn select_endpoint_rule_runtime(config: &crate::config::RateLimitConfig, path: &str) -> (String, RateLimitRule) {
    select_rule(&config.endpoints, &config.endpoint_aliases, &config.default, path)
}

/// Starts the config.
///
/// Reload failures are recorded on the manager (see
/// [`RateLimitConfigManager::degradation`]) and escalate from WARN to ERROR
/// after [`RELOAD_FAILURE_ESCALATION_THRESHOLD`] consecutive attempts, so a
/// persistently broken config file cannot hide behind a repeating WARN.
pub async fn start_config_watcher(
    manager: Arc<RateLimitConfigManager>,
    interval_seconds: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
        loop {
            interval.tick().await;
            if let Err(e) = manager.reload().await {
                let state = manager.degradation();
                if state.consecutive_failures >= RELOAD_FAILURE_ESCALATION_THRESHOLD {
                    tracing::error!(
                        target: "security_audit",
                        event = "rate_limit_config_degraded",
                        path = %manager.config_path().display(),
                        consecutive_failures = state.consecutive_failures,
                        total_failures = state.total_failures,
                        error = %e,
                        "限流配置热加载持续失败，仍在沿用最后一次成功的配置；\
                         rate_limit_config_source_is_file 与 /health 会报告 degraded"
                    );
                } else {
                    tracing::warn!(
                        path = %manager.config_path().display(),
                        consecutive_failures = state.consecutive_failures,
                        "Failed to reload rate limit config: {}",
                        e
                    );
                }
            }
        }
    })
}

/// Consecutive reload failures after which the watcher logs at ERROR level.
///
/// At the default 30s interval this is ~90s of a broken config file — long
/// enough to ride out an atomic-rename race, short enough to alert promptly.
pub const RELOAD_FAILURE_ESCALATION_THRESHOLD: u64 = 3;

#[derive(Debug, Clone)]
/// In-memory adapter around [`RateLimitConfigFile`] with identical fields.
pub struct RateLimitConfigAdapter {
    /// Global on/off switch.
    pub enabled: bool,
    /// Default rate-limit rule.
    pub default: RateLimitRule,
    /// Per-path overrides.
    pub endpoints: Vec<RateLimitEndpointRule>,
    /// Ordered list of headers consulted for client IP.
    pub ip_header_priority: Vec<String>,
    /// Emit rate-limit headers on responses.
    pub include_headers: bool,
    /// Paths that bypass rate limiting (exact match).
    pub exempt_paths: Vec<String>,
    /// Path prefixes that bypass rate limiting.
    pub exempt_path_prefixes: Vec<String>,
    /// Map of alias → canonical endpoint path.
    pub endpoint_aliases: HashMap<String, String>,
    /// Allow requests when the backend is unavailable.
    pub fail_open_on_error: bool,
    /// CIDR strings for trusted reverse proxies.
    pub trusted_proxies: Vec<String>,
    /// Whether to honor `X-Forwarded-For` etc.
    pub trust_forwarded: bool,
}

impl From<RateLimitConfigFile> for RateLimitConfigAdapter {
    fn from(config: RateLimitConfigFile) -> Self {
        // B-1: leaf types are unified, so this is a straight field move.
        Self {
            enabled: config.enabled,
            default: config.default,
            endpoints: config.endpoints,
            ip_header_priority: config.ip_header_priority,
            include_headers: config.include_headers,
            exempt_paths: config.exempt_paths,
            exempt_path_prefixes: config.exempt_path_prefixes,
            endpoint_aliases: config.endpoint_aliases,
            fail_open_on_error: config.fail_open_on_error,
            trusted_proxies: config.trusted_proxies,
            trust_forwarded: config.trust_forwarded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = RateLimitConfigFile::default();
        assert!(config.enabled);
        assert_eq!(config.default.per_second, 10);
        assert_eq!(config.default.burst_size, 20);
        assert!(config.include_headers);
    }

    #[test]
    fn test_config_validation() {
        let mut config = RateLimitConfigFile::default();
        config.default.per_second = 0;
        assert!(config.validate().is_err());

        config.default.per_second = 10;
        config.default.burst_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_endpoint_validation() {
        let mut config = RateLimitConfigFile::default();
        config.endpoints.push(RateLimitEndpointRule {
            path: "".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule::default(),
        });
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn test_config_save_and_load() {
        let config = RateLimitConfigFile {
            enabled: true,
            default: RateLimitRule { per_second: 50, burst_size: 100 },
            endpoints: vec![RateLimitEndpointRule {
                path: "/_matrix/client/r0/login".to_string(),
                match_type: RateLimitMatchType::Exact,
                rule: RateLimitRule { per_second: 5, burst_size: 10 },
            }],
            ..Default::default()
        };

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        config.save(&path).await.unwrap();

        let loaded = RateLimitConfigFile::load(&path).await.unwrap();
        assert_eq!(loaded.enabled, config.enabled);
        assert_eq!(loaded.default.per_second, 50);
        assert_eq!(loaded.endpoints.len(), 1);
    }

    #[test]
    fn test_select_endpoint_rule_exact() {
        let config = RateLimitConfigFile {
            endpoints: vec![RateLimitEndpointRule {
                path: "/_matrix/client/r0/login".to_string(),
                match_type: RateLimitMatchType::Exact,
                rule: RateLimitRule { per_second: 5, burst_size: 10 },
            }],
            ..Default::default()
        };

        let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/r0/login");
        assert_eq!(id, "/_matrix/client/r0/login");
        assert_eq!(rule.per_second, 5);
    }

    #[test]
    fn test_select_endpoint_rule_prefix() {
        let config = RateLimitConfigFile {
            endpoints: vec![
                RateLimitEndpointRule {
                    path: "/_matrix/client".to_string(),
                    match_type: RateLimitMatchType::Prefix,
                    rule: RateLimitRule { per_second: 50, burst_size: 100 },
                },
                RateLimitEndpointRule {
                    path: "/_matrix/client/r0/sync".to_string(),
                    match_type: RateLimitMatchType::Prefix,
                    rule: RateLimitRule { per_second: 20, burst_size: 40 },
                },
            ],
            ..Default::default()
        };

        let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/r0/sync?since=123");
        assert_eq!(id, "/_matrix/client/r0/sync");
        assert_eq!(rule.per_second, 20);

        let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/versions");
        assert_eq!(id, "/_matrix/client");
        assert_eq!(rule.per_second, 50);
    }

    #[test]
    fn test_select_endpoint_rule_default() {
        let config = RateLimitConfigFile::default();
        let (id, rule) = select_endpoint_rule(&config, "/unknown/path");
        assert_eq!(id, "/unknown/path");
        assert_eq!(rule.per_second, config.default.per_second);
    }

    #[test]
    fn test_endpoint_aliases() {
        let mut config = RateLimitConfigFile::default();
        config.endpoint_aliases.insert("/_matrix/client/r0/login".to_string(), "login_endpoint".to_string());
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/r0/login".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule { per_second: 5, burst_size: 10 },
        });

        let (id, _) = select_endpoint_rule(&config, "/_matrix/client/r0/login");
        assert_eq!(id, "login_endpoint");
    }
}

#[cfg(test)]
mod degradation_tests {
    //! Observability for rate-limit configuration degradation.
    //!
    //! ## Why
    //!
    //! Before this, a missing/unparseable `RATE_LIMIT_CONFIG_PATH` caused the
    //! server to fall back to `RateLimitConfigFile::default()` **silently**:
    //! the only trace was a single `tracing::warn!`, with no metric and no
    //! health signal. An operator who wrote limits into the config file had
    //! them dropped without any machine-readable indication.
    //!
    //! A failing hot-reload was worse: the watcher retries every
    //! `reload_interval_seconds` (default 30s) and logs one WARN per attempt
    //! forever, while the process keeps serving the **last-good** config. A
    //! single-file bind mount whose inode is invalidated by an atomic host-side
    //! replace reproduces exactly this (see docs/audit/P4_performance_baseline_2026-09-11.md §5.6).

    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn manager_loaded_from_file_reports_file_source() {
        let file = NamedTempFile::new().expect("temp file");
        let config = RateLimitConfigFile::default();
        std::fs::write(file.path(), serde_yaml::to_string(&config).expect("serialize")).expect("write");

        let manager = RateLimitConfigManager::from_file(file.path()).await.expect("load");
        let degradation = manager.degradation();

        assert_eq!(degradation.source, ConfigSource::File);
        assert_eq!(degradation.consecutive_failures, 0);
        assert_eq!(degradation.total_failures, 0);
        assert!(degradation.last_error.is_none(), "刚加载成功的配置不应带错误");
    }

    #[tokio::test]
    async fn manager_built_from_defaults_reports_default_source() {
        let manager =
            RateLimitConfigManager::new(RateLimitConfigFile::default(), PathBuf::from("/nonexistent/rate_limit.yaml"));
        let degradation = manager.degradation();

        assert_eq!(
            degradation.source,
            ConfigSource::Defaults,
            "用内置默认值构造时必须标记为 Defaults —— 这正是「配置文件缺失」的降级路径"
        );
    }

    #[tokio::test]
    async fn reload_failure_records_error_and_counters() {
        let file = NamedTempFile::new().expect("temp file");
        std::fs::write(file.path(), serde_yaml::to_string(&RateLimitConfigFile::default()).expect("ser"))
            .expect("write");
        let manager = RateLimitConfigManager::from_file(file.path()).await.expect("load");
        assert_eq!(manager.degradation().consecutive_failures, 0);

        // Break the file, then delete it — the two realistic failure modes
        // (parse error vs. vanished path, the latter being the bind-mount case).
        std::fs::write(file.path(), "this: [is not: valid yaml").expect("write");
        assert!(manager.reload().await.is_err());
        let after_parse = manager.degradation();
        assert_eq!(after_parse.consecutive_failures, 1);
        assert_eq!(after_parse.total_failures, 1);
        assert!(after_parse.last_error.is_some(), "解析失败必须记录错误信息");

        std::fs::remove_file(file.path()).expect("remove");
        assert!(manager.reload().await.is_err());
        let after_missing = manager.degradation();
        assert_eq!(after_missing.consecutive_failures, 2, "连续失败必须累加");
        assert_eq!(after_missing.total_failures, 2);
        let err = after_missing.last_error.expect("must record error");
        assert!(
            err.contains("No such file") || err.contains("Failed to read"),
            "错误信息应说明是读取失败（挂载 inode 失效场景），实际: {err}"
        );
    }

    #[tokio::test]
    async fn successful_reload_clears_degradation() {
        let file = NamedTempFile::new().expect("temp file");
        std::fs::write(file.path(), serde_yaml::to_string(&RateLimitConfigFile::default()).expect("ser"))
            .expect("write");
        let manager = RateLimitConfigManager::from_file(file.path()).await.expect("load");

        std::fs::remove_file(file.path()).expect("remove");
        assert!(manager.reload().await.is_err());
        assert_eq!(manager.degradation().consecutive_failures, 1);

        std::fs::write(file.path(), serde_yaml::to_string(&RateLimitConfigFile::default()).expect("ser"))
            .expect("restore");
        manager.reload().await.expect("reload should recover");

        let recovered = manager.degradation();
        assert_eq!(recovered.consecutive_failures, 0, "恢复后连续失败计数必须清零");
        assert!(recovered.last_error.is_none(), "恢复后不得残留错误");
        assert_eq!(recovered.total_failures, 1, "累计失败数保留用于事后分析");
    }

    #[tokio::test]
    async fn degradation_is_readable_from_the_shared_handle() {
        // The watcher holds an Arc; the health endpoint holds a clone. They must
        // observe the same state.
        let file = NamedTempFile::new().expect("temp file");
        std::fs::write(file.path(), serde_yaml::to_string(&RateLimitConfigFile::default()).expect("ser"))
            .expect("write");
        let manager = Arc::new(RateLimitConfigManager::from_file(file.path()).await.expect("load"));
        let clone = manager.clone();

        std::fs::remove_file(file.path()).expect("remove");
        assert!(clone.reload().await.is_err());

        assert_eq!(manager.degradation().consecutive_failures, 1);
    }

    #[test]
    fn config_source_labels_are_stable() {
        // Operators grep these labels; renaming is a breaking change.
        assert_eq!(ConfigSource::File.as_str(), "file");
        assert_eq!(ConfigSource::Defaults.as_str(), "defaults");
    }
}
