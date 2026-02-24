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
        Self {
            per_second: default_per_second(),
            burst_size: default_burst_size(),
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfigFile {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
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
    #[serde(default = "default_config_reload_interval")]
    pub reload_interval_seconds: u64,
}

fn default_enabled() -> bool {
    true
}

fn default_include_headers() -> bool {
    true
}

fn default_ip_header_priority() -> Vec<String> {
    vec![
        "x-forwarded-for".to_string(),
        "x-real-ip".to_string(),
        "forwarded".to_string(),
    ]
}

fn default_config_reload_interval() -> u64 {
    30
}

impl Default for RateLimitConfigFile {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            default: RateLimitRule::default(),
            endpoints: Vec::new(),
            ip_header_priority: default_ip_header_priority(),
            include_headers: default_include_headers(),
            exempt_paths: vec!["/".to_string(), "/_matrix/client/versions".to_string()],
            exempt_path_prefixes: Vec::new(),
            endpoint_aliases: HashMap::new(),
            fail_open_on_error: false,
            reload_interval_seconds: default_config_reload_interval(),
        }
    }
}

impl RateLimitConfigFile {
    pub fn validate(&self) -> Result<(), RateLimitConfigError> {
        if self.default.per_second == 0 {
            return Err(RateLimitConfigError::ValidationError(
                "default.per_second cannot be zero".to_string(),
            ));
        }
        if self.default.burst_size == 0 {
            return Err(RateLimitConfigError::ValidationError(
                "default.burst_size cannot be zero".to_string(),
            ));
        }
        for (idx, endpoint) in self.endpoints.iter().enumerate() {
            if endpoint.path.is_empty() {
                return Err(RateLimitConfigError::ValidationError(format!(
                    "endpoints[{}].path cannot be empty",
                    idx
                )));
            }
            if endpoint.rule.per_second == 0 {
                return Err(RateLimitConfigError::ValidationError(format!(
                    "endpoints[{}].rule.per_second cannot be zero",
                    idx
                )));
            }
            if endpoint.rule.burst_size == 0 {
                return Err(RateLimitConfigError::ValidationError(format!(
                    "endpoints[{}].rule.burst_size cannot be zero",
                    idx
                )));
            }
        }
        Ok(())
    }

    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self, RateLimitConfigError> {
        let content = fs::read_to_string(path.as_ref())
            .await
            .map_err(RateLimitConfigError::ReadError)?;
        let config: Self = serde_yaml::from_str(&content).map_err(RateLimitConfigError::ParseError)?;
        config.validate()?;
        Ok(config)
    }

    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), RateLimitConfigError> {
        let content = serde_yaml::to_string(self).map_err(RateLimitConfigError::ParseError)?;
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(RateLimitConfigError::ReadError)?;
        }
        fs::write(path.as_ref(), content)
            .await
            .map_err(RateLimitConfigError::ReadError)?;
        Ok(())
    }
}

pub struct RateLimitConfigManager {
    config: Arc<RwLock<RateLimitConfigFile>>,
    config_path: PathBuf,
}

impl RateLimitConfigManager {
    pub fn new(config: RateLimitConfigFile, config_path: PathBuf) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        }
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
        }).await
    }

    pub async fn remove_endpoint_rule(&self, path: &str) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.endpoints.retain(|r| r.path != path);
        }).await
    }

    pub async fn add_exempt_path(&self, path: String) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            if !c.exempt_paths.contains(&path) {
                c.exempt_paths.push(path);
            }
        }).await
    }

    pub async fn remove_exempt_path(&self, path: &str) -> Result<(), RateLimitConfigError> {
        self.update(|c| {
            c.exempt_paths.retain(|p| p != path);
        }).await
    }
}

pub fn select_endpoint_rule(
    config: &RateLimitConfigFile,
    path: &str,
) -> (String, RateLimitRule) {
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
            let endpoint_id = config
                .endpoint_aliases
                .get(&rule.path)
                .cloned()
                .unwrap_or_else(|| rule.path.clone());
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
            endpoints: config.endpoints.into_iter().map(|e| {
                crate::common::config::RateLimitEndpointRule {
                    path: e.path,
                    match_type: match e.match_type {
                        RateLimitMatchType::Exact => crate::common::config::RateLimitMatchType::Exact,
                        RateLimitMatchType::Prefix => crate::common::config::RateLimitMatchType::Prefix,
                    },
                    rule: crate::common::config::RateLimitRule {
                        per_second: e.rule.per_second,
                        burst_size: e.rule.burst_size,
                    },
                }
            }).collect(),
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
            default: RateLimitRule {
                per_second: 50,
                burst_size: 100,
            },
            endpoints: vec![RateLimitEndpointRule {
                path: "/_matrix/client/r0/login".to_string(),
                match_type: RateLimitMatchType::Exact,
                rule: RateLimitRule {
                    per_second: 5,
                    burst_size: 10,
                },
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
                rule: RateLimitRule {
                    per_second: 5,
                    burst_size: 10,
                },
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
                    rule: RateLimitRule {
                        per_second: 50,
                        burst_size: 100,
                    },
                },
                RateLimitEndpointRule {
                    path: "/_matrix/client/r0/sync".to_string(),
                    match_type: RateLimitMatchType::Prefix,
                    rule: RateLimitRule {
                        per_second: 20,
                        burst_size: 40,
                    },
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
        config.endpoint_aliases.insert(
            "/_matrix/client/r0/login".to_string(),
            "login_endpoint".to_string(),
        );
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/r0/login".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule {
                per_second: 5,
                burst_size: 10,
            },
        });

        let (id, _) = select_endpoint_rule(&config, "/_matrix/client/r0/login");
        assert_eq!(id, "login_endpoint");
    }
}
