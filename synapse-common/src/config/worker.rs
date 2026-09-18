use educe::Educe;
use serde::Deserialize;
use std::collections::HashMap;

// ============================================================================
// SECTION: Worker & Replication Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
/// Represents WorkerConfig.
pub struct WorkerConfig {
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,
    #[serde(default = "default_worker_instance_name")]
    /// `instance_name` field.
    pub instance_name: String,
    #[serde(default)]
    /// `worker_app` field.
    pub worker_app: Option<String>,
    #[serde(default)]
    /// `instance_map` field.
    pub instance_map: HashMap<String, InstanceLocationConfig>,
    #[serde(default)]
    /// `stream_writers` field.
    pub stream_writers: StreamWriters,
    #[serde(default)]
    /// `replication` field.
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
    /// `lock_max_retry_interval_ms` field.
    pub lock_max_retry_interval_ms: u64,
    /// Maximum number of retry attempts before giving up on lock
    /// acquisition. Default 3.
    #[serde(default = "default_lock_max_retries")]
    /// `lock_max_retries` field.
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
/// Represents InstanceLocationConfig.
pub struct InstanceLocationConfig {
    /// `host` field.
    pub host: String,
    /// `port` field.
    pub port: u16,
    /// TLS 默认启用。生产环境联邦连接应始终使用 TLS。
    /// 仅在本地开发或已通过外部代理（如 nginx）终止 TLS 时可关闭。
    #[serde(default = "default_true")]
    /// `tls` field.
    pub tls: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
/// Represents StreamWriters.
pub struct StreamWriters {
    #[serde(default = "default_stream_writers")]
    /// `events` field.
    pub events: Vec<String>,
    #[serde(default = "default_stream_writers")]
    /// `typing` field.
    pub typing: Vec<String>,
    #[serde(default = "default_stream_writers")]
    /// `to_device` field.
    pub to_device: Vec<String>,
    #[serde(default = "default_stream_writers")]
    /// `account_data` field.
    pub account_data: Vec<String>,
    #[serde(default = "default_stream_writers")]
    /// `receipts` field.
    pub receipts: Vec<String>,
    #[serde(default = "default_stream_writers")]
    /// `presence` field.
    pub presence: Vec<String>,
    #[serde(default = "default_stream_writers")]
    /// `push_rules` field.
    pub push_rules: Vec<String>,
    #[serde(default = "default_stream_writers")]
    /// `device_lists` field.
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
/// Represents ReplicationConfig.
pub struct ReplicationConfig {
    /// `enabled` field.
    pub enabled: bool,
    /// `server_name` field.
    pub server_name: String,
    /// `http` field.
    pub http: ReplicationHttpConfig,
}

#[derive(Clone, Deserialize, Default, Educe)]
#[educe(Debug)]
/// Represents ReplicationHttpConfig.
pub struct ReplicationHttpConfig {
    /// `enabled` field.
    pub enabled: bool,
    /// `host` field.
    pub host: String,
    /// `port` field.
    pub port: u16,
    #[educe(Debug(ignore))]
    /// `secret` field.
    pub secret: Option<String>,
    /// `secret_path` field.
    pub secret_path: Option<String>,
}

/// Minimum accepted length for a worker HTTP replication secret.
pub const MIN_REPLICATION_SECRET_LEN: usize = 32;

/// Values that are placeholders, project fixtures, or otherwise public.
///
/// A deployment using any of these has an effectively **unauthenticated** worker
/// surface: `replication_http_auth_middleware` gates writes to replication
/// positions, the event stream and worker/task state behind this single shared
/// secret, so a guessable value is an auth bypass, not a cosmetic issue.
///
/// `worker_replication_secret_2026` is this crate's own test fixture value
/// (`config::tests::test_resolve_env_variables_resolves_worker_replication_config`)
/// — and was measured in a real `docker/deploy/.env`, i.e. an operator copied a
/// value that is printed in the repository.
const KNOWN_WEAK_REPLICATION_SECRETS: &[&str] = &[
    "CHANGE_ME",                      // docker/deploy/.env.example placeholder
    "worker_replication_secret_2026", // this crate's test fixture value
    "test_worker_secret",             // integration-test fixture
];

/// Validate the worker HTTP replication secret.
///
/// `strict` selects the production policy. It is passed in (rather than read from
/// `cfg!(debug_assertions)` inside) so both policies are unit-testable — tests run
/// as debug builds, so a compile-time gate would leave the production branch
/// unverifiable. Callers pass `!cfg!(debug_assertions)`.
///
/// Takes the whole `WorkerConfig` because the surface is mounted only when
/// `worker.enabled && worker.replication.http.enabled` (see `WorkerBodyModule::merge_into`).
/// Checking `http.enabled` alone would fail startup for deployments that leave the
/// worker disabled, where the secret protects nothing.
///
/// * The **presence** check always applies: both switches on with neither `secret`
///   nor `secret_path` cannot authenticate anything.
/// * The **strength** checks (known-public values, minimum length) apply only when
///   `strict`: dev/test fixtures legitimately use short placeholders.
pub fn validate_replication_http_secret(config: &WorkerConfig, strict: bool) -> Result<(), String> {
    if !config.enabled || !config.replication.http.enabled {
        // The surface is not mounted at all, so there is nothing to protect.
        return Ok(());
    }
    let http = &config.replication.http;

    if http.secret.is_none() && http.secret_path.is_none() {
        return Err(
            "worker.replication.http.enabled is true but neither worker.replication.http.secret nor secret_path is configured"
                .to_string(),
        );
    }

    if !strict {
        return Ok(());
    }

    let secret = match (&http.secret, &http.secret_path) {
        (Some(secret), _) => secret.clone(),
        (None, Some(path)) => std::fs::read_to_string(path).map_err(|error| {
            format!(
                "worker.replication.http.secret_path '{path}' cannot be read ({error}); \
                 the replication surface would reject every request"
            )
        })?,
        (None, None) => unreachable!("presence checked above"),
    };
    let secret = secret.trim();

    // Checked before the length rule on purpose: every known placeholder is itself
    // shorter than the minimum, so a length-first order would make this branch
    // unreachable — a check that can never fire. Matching is by substring so that a
    // placeholder padded to satisfy the length rule ("CHANGE_ME" + digits) is still
    // caught; a real random secret containing these literals is not a realistic risk.
    if let Some(weak) = KNOWN_WEAK_REPLICATION_SECRETS.iter().find(|weak| secret.contains(**weak)) {
        return Err(format!(
            "worker.replication.http secret contains the known placeholder/test value '{weak}'. \
             It is published in this repository, so the worker replication surface \
             (replication positions, event stream, worker/task state) is effectively \
             unauthenticated. Generate a random secret, e.g. `openssl rand -hex 32`."
        ));
    }
    if secret.len() < MIN_REPLICATION_SECRET_LEN {
        return Err(format!(
            "worker.replication.http secret must be at least {MIN_REPLICATION_SECRET_LEN} bytes, got {} bytes",
            secret.len()
        ));
    }

    Ok(())
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
        let config = InstanceLocationConfig { host: "localhost".to_string(), port: 8080, tls: true };
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
        assert!(config.tls);
    }

    #[test]
    fn test_default_true() {
        assert!(default_true());
    }

    /// Build a worker config with both switches on unless stated otherwise, since
    /// that is the only combination where the replication surface is mounted.
    fn worker_config(enabled: bool, http_enabled: bool, secret: Option<&str>) -> WorkerConfig {
        WorkerConfig {
            enabled,
            replication: ReplicationConfig {
                enabled,
                server_name: "test".to_string(),
                http: ReplicationHttpConfig {
                    enabled: http_enabled,
                    host: "127.0.0.1".to_string(),
                    port: 9093,
                    secret: secret.map(str::to_string),
                    secret_path: None,
                },
            },
            ..WorkerConfig::default()
        }
    }

    /// Shorthand for the mounted case: worker + HTTP replication both on.
    fn http_config(secret: Option<&str>) -> WorkerConfig {
        worker_config(true, true, secret)
    }

    #[test]
    fn replication_secret_validation_skips_when_the_surface_is_not_mounted() {
        // `docker/config/homeserver.yaml` ships `worker.enabled: false` with
        // `replication.http.enabled: true`; the surface is not mounted in that
        // combination, so a weak secret there must not block startup.
        assert!(validate_replication_http_secret(&worker_config(false, true, None), true).is_ok());
        assert!(validate_replication_http_secret(&worker_config(false, true, Some("CHANGE_ME")), true).is_ok());
        assert!(validate_replication_http_secret(&worker_config(true, false, Some("CHANGE_ME")), true).is_ok());
    }

    #[test]
    fn replication_secret_validation_requires_a_secret_when_enabled() {
        let error = validate_replication_http_secret(&http_config(None), false)
            .expect_err("enabled without any secret must fail even in non-strict mode");
        assert!(error.contains("neither worker.replication.http.secret nor secret_path"), "{error}");
    }

    #[test]
    fn replication_secret_validation_rejects_short_and_public_values_in_strict_mode() {
        let short = "s".repeat(MIN_REPLICATION_SECRET_LEN - 1);
        let error = validate_replication_http_secret(&http_config(Some(&short)), true).expect_err("short secret");
        assert!(error.contains("at least"), "{error}");

        for weak in KNOWN_WEAK_REPLICATION_SECRETS {
            // Both the bare value and a length-padded one: the bare value is itself
            // under the minimum, so a length-first implementation would report a length
            // error and make the placeholder rule unreachable. The padded form proves
            // the placeholder rule fires on its own terms.
            for candidate in [(*weak).to_string(), format!("{weak}{}", "0".repeat(MIN_REPLICATION_SECRET_LEN))] {
                let error = validate_replication_http_secret(&http_config(Some(&candidate)), true)
                    .expect_err("placeholder content");
                assert!(error.contains("known placeholder"), "{candidate}: {error}");
            }
        }
    }

    #[test]
    fn replication_secret_validation_accepts_a_long_random_secret() {
        let secret = "a".repeat(MIN_REPLICATION_SECRET_LEN);
        assert!(validate_replication_http_secret(&http_config(Some(&secret)), true).is_ok());
    }

    #[test]
    fn replication_secret_validation_is_lenient_outside_strict_mode() {
        // Dev/test fixtures legitimately use short placeholders; strict mode is what
        // a release build selects.
        assert!(validate_replication_http_secret(&http_config(Some("test_worker_secret")), false).is_ok());
    }

    #[test]
    fn replication_secret_validation_reads_and_checks_secret_path() {
        let dir = std::env::temp_dir().join(format!("synapse-worker-secret-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let weak = dir.join("weak");
        std::fs::write(&weak, "CHANGE_ME\n").expect("write weak");
        let strong = dir.join("strong");
        std::fs::write(&strong, format!("{}\n", "b".repeat(MIN_REPLICATION_SECRET_LEN))).expect("write strong");

        let mut config = http_config(None);
        config.replication.http.secret_path = Some(weak.to_string_lossy().to_string());
        let error = validate_replication_http_secret(&config, true).expect_err("weak file");
        assert!(error.contains("known placeholder"), "{error}");

        config.replication.http.secret_path = Some(strong.to_string_lossy().to_string());
        assert!(validate_replication_http_secret(&config, true).is_ok());

        config.replication.http.secret_path = Some(dir.join("missing").to_string_lossy().to_string());
        let error = validate_replication_http_secret(&config, true).expect_err("unreadable file");
        assert!(error.contains("cannot be read"), "{error}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
