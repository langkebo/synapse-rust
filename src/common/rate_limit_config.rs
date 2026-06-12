use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::fs;

#[derive(Debug, Error)]
pub enum RateLimitConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[source] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(#[source] serde_yaml::Error),
    #[error("Config validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitRule {
    #[serde(default = "default_per_second")]
    pub per_second: u32,
    #[serde(default = "default_burst_size")]
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
pub enum RateLimitMatchType {
    #[default]
    Exact,
    Prefix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitEndpointRule {
    pub path: String,
    #[serde(default)]
    pub match_type: RateLimitMatchType,
    pub rule: RateLimitRule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitBackend {
    /// Automatically use Redis when available, fall back to in-memory otherwise.
    #[default]
    Auto,
    /// Always use Redis; fail loudly if Redis is not available.
    Redis,
    /// Always use in-memory token bucket (single-worker mode only).
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfigFile {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Rate-limit token-bucket backend: "auto" (default), "redis", or "local".
    ///
    /// - `auto`: use Redis when available, fall back to in-memory.
    /// - `redis`: require Redis; log an error and refuse requests if Redis is down.
    /// - `local`: always use in-memory (single-worker deployments only).
    #[serde(default)]
    pub backend: RateLimitBackend,
    #[serde(default)]
    pub default: RateLimitRule,
    #[serde(default)]
    pub endpoints: Vec<RateLimitEndpointRule>,
    #[serde(default = "default_ip_header_priority")]
    pub ip_header_priority: Vec<String>,
    #[serde(default = "default_include_headers")]
    pub include_headers: bool,
    #[serde(default)]
    pub exempt_paths: Vec<String>,
    #[serde(default)]
    pub exempt_path_prefixes: Vec<String>,
    #[serde(default)]
    pub endpoint_aliases: HashMap<String, String>,
    #[serde(default)]
    pub fail_open_on_error: bool,
    #[serde(default)]
    pub sync: SyncRateLimitConfigFile,
    #[serde(default = "default_config_reload_interval")]
    pub reload_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncRateLimitConfigFile {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub initial: RateLimitRule,
    #[serde(default)]
    pub incremental: RateLimitRule,
}

fn default_enabled() -> bool {
    true
}

fn default_include_headers() -> bool {
    true
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
                    rule: RateLimitRule { per_second: 1, burst_size: 3 },
                },
                RateLimitEndpointRule {
                    path: "/_matrix/client/v3/register".to_string(),
                    match_type: RateLimitMatchType::Prefix,
                    rule: RateLimitRule { per_second: 1, burst_size: 2 },
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
        }
    }
}

impl RateLimitConfigFile {
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

    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self, RateLimitConfigError> {
        let content = fs::read_to_string(path.as_ref()).await.map_err(RateLimitConfigError::ReadError)?;
        let config: Self = serde_yaml::from_str(&content).map_err(RateLimitConfigError::ParseError)?;
        config.validate()?;
        Ok(config)
    }

    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), RateLimitConfigError> {
        let content = serde_yaml::to_string(self).map_err(RateLimitConfigError::ParseError)?;
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).await.map_err(RateLimitConfigError::ReadError)?;
        }
        fs::write(path.as_ref(), content).await.map_err(RateLimitConfigError::ReadError)?;
        Ok(())
    }
}

pub struct RateLimitConfigManager {
    config: Arc<RwLock<RateLimitConfigFile>>,
    config_path: PathBuf,
}

impl RateLimitConfigManager {
    pub fn new(config: RateLimitConfigFile, config_path: PathBuf) -> Self {
        Self { config: Arc::new(RwLock::new(config)), config_path }
    }

    pub async fn from_file<P: Into<PathBuf>>(path: P) -> Result<Self, RateLimitConfigError> {
        let path = path.into();
        let config = RateLimitConfigFile::load(&path).await?;
        Ok(Self::new(config, path))
    }

    pub fn get_config(&self) -> RateLimitConfigFile {
        self.config.read().clone()
    }

    pub fn get_config_ref(&self) -> Arc<RwLock<RateLimitConfigFile>> {
        self.config.clone()
    }

    pub async fn reload(&self) -> Result<(), RateLimitConfigError> {
        let new_config = RateLimitConfigFile::load(&self.config_path).await?;
        {
            let mut config = self.config.write();
            *config = new_config;
        }
        tracing::info!("Rate limit configuration reloaded from {:?}", self.config_path);
        Ok(())
    }

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

    pub async fn set_enabled(&self, enabled: bool) -> Result<(), RateLimitConfigError> {
        self.update(|c| c.enabled = enabled).await
    }

    pub async fn set_default_rule(&self, rule: RateLimitRule) -> Result<(), RateLimitConfigError> {
        self.update(|c| c.default = rule).await
    }

    pub async fn add_endpoint_rule(&self, rule: RateLimitEndpointRule) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.endpoints.push(rule);
        })
        .await
    }

    pub async fn remove_endpoint_rule(&self, path: &str) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.endpoints.retain(|r| r.path != path);
        })
        .await
    }

    pub async fn add_exempt_path(&self, path: String) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            if !c.exempt_paths.contains(&path) {
                c.exempt_paths.push(path);
            }
        })
        .await
    }

    pub async fn remove_exempt_path(&self, path: &str) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.exempt_paths.retain(|p| p != path);
        })
        .await
    }
}

pub fn select_endpoint_rule(config: &RateLimitConfigFile, path: &str) -> (String, RateLimitRule) {
    let mut best_match: Option<&RateLimitEndpointRule> = None;
    let mut best_match_len = 0;

    for rule in &config.endpoints {
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
            let endpoint_id = config.endpoint_aliases.get(&rule.path).cloned().unwrap_or_else(|| rule.path.clone());
            (endpoint_id, rule.rule.clone())
        }
        None => (path.to_string(), config.default.clone()),
    }
}

pub fn select_endpoint_rule_runtime(
    config: &crate::common::config::RateLimitConfig,
    path: &str,
) -> (String, crate::common::config::RateLimitRule) {
    let mut best_match: Option<&crate::common::config::RateLimitEndpointRule> = None;
    let mut best_match_len = 0;

    for rule in &config.endpoints {
        let is_match = match rule.match_type {
            crate::common::config::RateLimitMatchType::Exact => rule.path == path,
            crate::common::config::RateLimitMatchType::Prefix => path.starts_with(&rule.path),
        };

        if is_match && rule.path.len() > best_match_len {
            best_match = Some(rule);
            best_match_len = rule.path.len();
        }
    }

    match best_match {
        Some(rule) => {
            let endpoint_id = config.endpoint_aliases.get(&rule.path).cloned().unwrap_or_else(|| rule.path.clone());
            (endpoint_id, rule.rule.clone())
        }
        None => (path.to_string(), config.default.clone()),
    }
}

pub async fn start_config_watcher(
    manager: Arc<RateLimitConfigManager>,
    interval_seconds: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
        loop {
            interval.tick().await;
            if let Err(e) = manager.reload().await {
                tracing::warn!("Failed to reload rate limit config: {}", e);
            }
        }
    })
}

#[derive(Debug, Clone)]
pub struct RateLimitConfigAdapter {
    pub enabled: bool,
    pub default: crate::common::config::RateLimitRule,
    pub endpoints: Vec<crate::common::config::RateLimitEndpointRule>,
    pub ip_header_priority: Vec<String>,
    pub include_headers: bool,
    pub exempt_paths: Vec<String>,
    pub exempt_path_prefixes: Vec<String>,
    pub endpoint_aliases: HashMap<String, String>,
    pub fail_open_on_error: bool,
}

impl From<RateLimitConfigFile> for RateLimitConfigAdapter {
    fn from(config: RateLimitConfigFile) -> Self {
        Self {
            enabled: config.enabled,
            default: crate::common::config::RateLimitRule {
                per_second: config.default.per_second,
                burst_size: config.default.burst_size,
            },
            endpoints: config
                .endpoints
                .into_iter()
                .map(|e| crate::common::config::RateLimitEndpointRule {
                    path: e.path,
                    match_type: match e.match_type {
                        RateLimitMatchType::Exact => crate::common::config::RateLimitMatchType::Exact,
                        RateLimitMatchType::Prefix => crate::common::config::RateLimitMatchType::Prefix,
                    },
                    rule: crate::common::config::RateLimitRule {
                        per_second: e.rule.per_second,
                        burst_size: e.rule.burst_size,
                    },
                })
                .collect(),
            ip_header_priority: config.ip_header_priority,
            include_headers: config.include_headers,
            exempt_paths: config.exempt_paths,
            exempt_path_prefixes: config.exempt_path_prefixes,
            endpoint_aliases: config.endpoint_aliases,
            fail_open_on_error: config.fail_open_on_error,
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

    // — RateLimitConfigManager tests —

    fn temp_config_path() -> (PathBuf, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        (path, temp_file)
    }

    #[tokio::test]
    async fn test_manager_new_and_get_config() {
        let config = RateLimitConfigFile::default();
        let (path, _temp) = temp_config_path();
        let manager = RateLimitConfigManager::new(config.clone(), path);

        let retrieved = manager.get_config();
        assert_eq!(retrieved.enabled, config.enabled);
        assert_eq!(retrieved.default.per_second, config.default.per_second);
    }

    #[tokio::test]
    async fn test_manager_get_config_ref() {
        let config = RateLimitConfigFile::default();
        let (path, _temp) = temp_config_path();
        let manager = RateLimitConfigManager::new(config, path);

        let config_ref = manager.get_config_ref();
        let locked = config_ref.read();
        assert_eq!(locked.default.per_second, 10);
        assert_eq!(locked.default.burst_size, 20);
    }

    #[tokio::test]
    async fn test_manager_from_file() {
        let config = RateLimitConfigFile {
            enabled: true,
            default: RateLimitRule { per_second: 30, burst_size: 60 },
            ..Default::default()
        };

        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        let retrieved = manager.get_config();
        assert_eq!(retrieved.default.per_second, 30);
        assert_eq!(retrieved.default.burst_size, 60);
    }

    #[tokio::test]
    async fn test_manager_reload() {
        let config =
            RateLimitConfigFile { default: RateLimitRule { per_second: 10, burst_size: 20 }, ..Default::default() };

        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();

        // Modify the file on disk
        let updated =
            RateLimitConfigFile { default: RateLimitRule { per_second: 99, burst_size: 199 }, ..Default::default() };
        updated.save(&path).await.unwrap();

        manager.reload().await.unwrap();
        let retrieved = manager.get_config();
        assert_eq!(retrieved.default.per_second, 99);
        assert_eq!(retrieved.default.burst_size, 199);
    }

    #[tokio::test]
    async fn test_manager_set_enabled() {
        let config = RateLimitConfigFile { enabled: true, ..Default::default() };
        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        assert!(manager.get_config().enabled);

        manager.set_enabled(false).await.unwrap();
        assert!(!manager.get_config().enabled);

        // Verify persisted on disk
        let loaded = RateLimitConfigFile::load(&path).await.unwrap();
        assert!(!loaded.enabled);
    }

    #[tokio::test]
    async fn test_manager_set_default_rule() {
        let config = RateLimitConfigFile::default();
        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        manager.set_default_rule(RateLimitRule { per_second: 50, burst_size: 100 }).await.unwrap();

        let retrieved = manager.get_config();
        assert_eq!(retrieved.default.per_second, 50);
        assert_eq!(retrieved.default.burst_size, 100);
    }

    #[tokio::test]
    async fn test_manager_add_endpoint_rule() {
        let config = RateLimitConfigFile::default();
        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        manager
            .add_endpoint_rule(RateLimitEndpointRule {
                path: "/custom/endpoint".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule { per_second: 5, burst_size: 10 },
            })
            .await
            .unwrap();

        let retrieved = manager.get_config();
        let found = retrieved.endpoints.iter().any(|e| e.path == "/custom/endpoint");
        assert!(found);
    }

    #[tokio::test]
    async fn test_manager_remove_endpoint_rule() {
        let mut config = RateLimitConfigFile::default();
        config.endpoints.push(RateLimitEndpointRule {
            path: "/custom/endpoint".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule::default(),
        });

        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        assert_eq!(manager.get_config().endpoints.len(), 4); // 3 defaults + 1 custom

        manager.remove_endpoint_rule("/custom/endpoint").await.unwrap();

        let retrieved = manager.get_config();
        assert_eq!(retrieved.endpoints.len(), 3);
        assert!(!retrieved.endpoints.iter().any(|e| e.path == "/custom/endpoint"));
    }

    #[tokio::test]
    async fn test_manager_add_exempt_path() {
        let config = RateLimitConfigFile::default();
        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        manager.add_exempt_path("/custom/exempt".to_string()).await.unwrap();

        let retrieved = manager.get_config();
        assert!(retrieved.exempt_paths.contains(&"/custom/exempt".to_string()));
    }

    #[tokio::test]
    async fn test_manager_add_exempt_path_dedup() {
        let config = RateLimitConfigFile::default();
        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        let original_count = manager.get_config().exempt_paths.len();

        // Add same path twice — should not add duplicate
        manager.add_exempt_path("/dedup/path".to_string()).await.unwrap();
        manager.add_exempt_path("/dedup/path".to_string()).await.unwrap();

        let retrieved = manager.get_config();
        assert_eq!(retrieved.exempt_paths.len(), original_count + 1);
    }

    #[tokio::test]
    async fn test_manager_remove_exempt_path() {
        let mut config = RateLimitConfigFile::default();
        config.exempt_paths.push("/custom/exempt".to_string());

        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        assert!(manager.get_config().exempt_paths.contains(&"/custom/exempt".to_string()));

        manager.remove_exempt_path("/custom/exempt").await.unwrap();

        let retrieved = manager.get_config();
        assert!(!retrieved.exempt_paths.contains(&"/custom/exempt".to_string()));
    }

    #[tokio::test]
    async fn test_manager_update_validation_rejects_zero_per_second() {
        let config = RateLimitConfigFile::default();
        let (path, _temp) = temp_config_path();
        config.save(&path).await.unwrap();

        let manager = RateLimitConfigManager::from_file(&path).await.unwrap();
        let result = manager.set_default_rule(RateLimitRule { per_second: 0, burst_size: 10 }).await;
        assert!(result.is_err());
    }

    // — RateLimitConfigAdapter tests —

    #[test]
    fn test_adapter_from_config_file() {
        let config = RateLimitConfigFile {
            enabled: true,
            default: RateLimitRule { per_second: 25, burst_size: 50 },
            endpoints: vec![RateLimitEndpointRule {
                path: "/test".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule { per_second: 5, burst_size: 10 },
            }],
            ip_header_priority: vec!["x-custom".to_string()],
            include_headers: false,
            exempt_paths: vec!["/exempt".to_string()],
            exempt_path_prefixes: vec!["/prefix".to_string()],
            endpoint_aliases: {
                let mut m = HashMap::new();
                m.insert("/test".to_string(), "test_alias".to_string());
                m
            },
            fail_open_on_error: true,
            ..Default::default()
        };

        let adapter = RateLimitConfigAdapter::from(config);
        assert!(adapter.enabled);
        assert_eq!(adapter.default.per_second, 25);
        assert_eq!(adapter.default.burst_size, 50);
        assert_eq!(adapter.endpoints.len(), 1);
        assert_eq!(adapter.endpoints[0].path, "/test");
        assert_eq!(adapter.ip_header_priority, vec!["x-custom"]);
        assert!(!adapter.include_headers);
        assert_eq!(adapter.exempt_paths, vec!["/exempt"]);
        assert_eq!(adapter.exempt_path_prefixes, vec!["/prefix"]);
        assert_eq!(adapter.endpoint_aliases.get("/test"), Some(&"test_alias".to_string()));
        assert!(adapter.fail_open_on_error);
    }

    // — select_endpoint_rule additional tests —

    #[test]
    fn test_select_endpoint_rule_exact_no_match() {
        let config = RateLimitConfigFile {
            endpoints: vec![RateLimitEndpointRule {
                path: "/exact/match".to_string(),
                match_type: RateLimitMatchType::Exact,
                rule: RateLimitRule { per_second: 5, burst_size: 10 },
            }],
            ..Default::default()
        };

        // Exact match on a different path — should fall back to default
        let (id, rule) = select_endpoint_rule(&config, "/exact/match/extra");
        assert_eq!(id, "/exact/match/extra");
        assert_eq!(rule.per_second, config.default.per_second);
    }

    // — select_endpoint_rule_runtime tests —

    #[test]
    fn test_select_endpoint_rule_runtime_exact() {
        let config = crate::common::config::RateLimitConfig {
            enabled: true,
            default: crate::common::config::RateLimitRule { per_second: 10, burst_size: 20 },
            endpoints: vec![crate::common::config::RateLimitEndpointRule {
                path: "/_matrix/client/r0/login".to_string(),
                match_type: crate::common::config::RateLimitMatchType::Exact,
                rule: crate::common::config::RateLimitRule { per_second: 5, burst_size: 10 },
            }],
            ..Default::default()
        };

        let (id, rule) = select_endpoint_rule_runtime(&config, "/_matrix/client/r0/login");
        assert_eq!(id, "/_matrix/client/r0/login");
        assert_eq!(rule.per_second, 5);
    }

    #[test]
    fn test_select_endpoint_rule_runtime_prefix() {
        let config = crate::common::config::RateLimitConfig {
            enabled: true,
            default: crate::common::config::RateLimitRule { per_second: 10, burst_size: 20 },
            endpoints: vec![
                crate::common::config::RateLimitEndpointRule {
                    path: "/_matrix/client".to_string(),
                    match_type: crate::common::config::RateLimitMatchType::Prefix,
                    rule: crate::common::config::RateLimitRule { per_second: 50, burst_size: 100 },
                },
                crate::common::config::RateLimitEndpointRule {
                    path: "/_matrix/client/r0/sync".to_string(),
                    match_type: crate::common::config::RateLimitMatchType::Prefix,
                    rule: crate::common::config::RateLimitRule { per_second: 20, burst_size: 40 },
                },
            ],
            ..Default::default()
        };

        let (id, rule) = select_endpoint_rule_runtime(&config, "/_matrix/client/r0/sync?since=123");
        assert_eq!(id, "/_matrix/client/r0/sync");
        assert_eq!(rule.per_second, 20);
    }

    #[test]
    fn test_select_endpoint_rule_runtime_default() {
        let config = crate::common::config::RateLimitConfig::default();
        let (id, rule) = select_endpoint_rule_runtime(&config, "/unknown/path");
        assert_eq!(id, "/unknown/path");
        assert_eq!(rule.per_second, config.default.per_second);
    }

    #[test]
    fn test_select_endpoint_rule_runtime_alias() {
        let mut config = crate::common::config::RateLimitConfig {
            enabled: true,
            default: crate::common::config::RateLimitRule { per_second: 10, burst_size: 20 },
            endpoints: vec![crate::common::config::RateLimitEndpointRule {
                path: "/_matrix/client/r0/login".to_string(),
                match_type: crate::common::config::RateLimitMatchType::Exact,
                rule: crate::common::config::RateLimitRule { per_second: 5, burst_size: 10 },
            }],
            ..Default::default()
        };
        config.endpoint_aliases.insert("/_matrix/client/r0/login".to_string(), "login".to_string());

        let (id, _) = select_endpoint_rule_runtime(&config, "/_matrix/client/r0/login");
        assert_eq!(id, "login");
    }

    // — validate() additional edge cases —

    #[test]
    fn test_validate_endpoint_per_second_zero() {
        let mut config = RateLimitConfigFile::default();
        config.endpoints.push(RateLimitEndpointRule {
            path: "/test".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule { per_second: 0, burst_size: 10 },
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_endpoint_burst_size_zero() {
        let mut config = RateLimitConfigFile::default();
        config.endpoints.push(RateLimitEndpointRule {
            path: "/test".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule { per_second: 10, burst_size: 0 },
        });
        assert!(config.validate().is_err());
    }
}
