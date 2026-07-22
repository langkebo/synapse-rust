use serde::Deserialize;
use std::collections::HashMap;

// ============================================================================
// SECTION: Worker & Replication Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_worker_instance_name")]
    pub instance_name: String,
    #[serde(default)]
    pub worker_app: Option<String>,
    #[serde(default)]
    pub instance_map: HashMap<String, InstanceLocationConfig>,
    #[serde(default)]
    pub stream_writers: StreamWriters,
    #[serde(default)]
    pub replication: ReplicationConfig,
    /// Maximum interval (in milliseconds) between lock acquisition retries.
    ///
    /// When a worker fails to acquire a background update lock, it retries
    /// with exponential backoff capped at this interval. This prevents CPU
    /// starvation / DoS under lock contention.
    ///
    /// Aligned with Synapse v1.153.0 which lowered
    /// `WORKER_LOCK_MAX_RETRY_INTERVAL` to 5 seconds.
    #[serde(default = "default_lock_max_retry_interval_ms")]
    pub lock_max_retry_interval_ms: u64,
    /// Maximum number of retry attempts before giving up on lock
    /// acquisition. Default 3.
    #[serde(default = "default_lock_max_retries")]
    pub lock_max_retries: u32,
}

fn default_worker_instance_name() -> String {
    "master".to_string()
}

fn default_lock_max_retry_interval_ms() -> u64 {
    5000
}

fn default_lock_max_retries() -> u32 {
    3
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            instance_name: default_worker_instance_name(),
            worker_app: None,
            instance_map: HashMap::new(),
            stream_writers: StreamWriters::default(),
            replication: ReplicationConfig::default(),
            lock_max_retry_interval_ms: default_lock_max_retry_interval_ms(),
            lock_max_retries: default_lock_max_retries(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstanceLocationConfig {
    pub host: String,
    pub port: u16,
    /// TLS 默认启用。生产环境联邦连接应始终使用 TLS。
    /// 仅在本地开发或已通过外部代理（如 nginx）终止 TLS 时可关闭。
    #[serde(default = "default_true")]
    pub tls: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamWriters {
    #[serde(default = "default_stream_writers")]
    pub events: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub typing: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub to_device: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub account_data: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub receipts: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub presence: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub push_rules: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub device_lists: Vec<String>,
}

fn default_stream_writers() -> Vec<String> {
    vec![default_worker_instance_name()]
}

impl Default for StreamWriters {
    fn default() -> Self {
        let default = default_stream_writers();
        Self {
            events: default.clone(),
            typing: default.clone(),
            to_device: default.clone(),
            account_data: default.clone(),
            receipts: default.clone(),
            presence: default.clone(),
            push_rules: default.clone(),
            device_lists: default,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReplicationConfig {
    pub enabled: bool,
    pub server_name: String,
    pub http: ReplicationHttpConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReplicationHttpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub secret: Option<String>,
    pub secret_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_config_default() {
        let config = WorkerConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.instance_name, "master");
        assert!(config.worker_app.is_none());
        assert!(config.instance_map.is_empty());
        assert_eq!(config.lock_max_retry_interval_ms, 5000);
        assert_eq!(config.lock_max_retries, 3);
    }

    #[test]
    fn test_stream_writers_default() {
        let writers = StreamWriters::default();
        assert_eq!(writers.events, vec!["master"]);
        assert_eq!(writers.typing, vec!["master"]);
        assert_eq!(writers.to_device, vec!["master"]);
        assert_eq!(writers.account_data, vec!["master"]);
        assert_eq!(writers.receipts, vec!["master"]);
        assert_eq!(writers.presence, vec!["master"]);
        assert_eq!(writers.push_rules, vec!["master"]);
        assert_eq!(writers.device_lists, vec!["master"]);
    }

    #[test]
    fn test_replication_config_default() {
        let config = ReplicationConfig::default();
        assert!(!config.enabled);
        assert!(config.server_name.is_empty());
        assert!(!config.http.enabled);
        assert!(config.http.host.is_empty());
        assert_eq!(config.http.port, 0);
        assert!(config.http.secret.is_none());
        assert!(config.http.secret_path.is_none());
    }

    #[test]
    fn test_instance_location_config_creation() {
        let config = InstanceLocationConfig {
            host: "localhost".to_string(),
            port: 8080,
            tls: true,
        };
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
        assert!(config.tls);
    }

    #[test]
    fn test_default_true() {
        assert!(default_true());
    }
}
