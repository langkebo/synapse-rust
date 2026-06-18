use serde::Serialize;
use std::collections::HashSet;
use tracing::{info, warn};

use synapse_common::config::worker::WorkerConfig;

use crate::worker::types::WorkerType;

/// Result of a topology validation run.
#[derive(Debug, Clone, Serialize)]
pub struct TopologyValidation {
    /// Whether the topology is valid (no hard errors).
    pub valid: bool,
    /// Validation warnings (non-fatal issues).
    pub warnings: Vec<String>,
    /// Validation errors (fatal issues).
    pub errors: Vec<String>,
}

impl TopologyValidation {
    fn new() -> Self {
        Self { valid: true, warnings: Vec::new(), errors: Vec::new() }
    }

    fn add_error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
        self.valid = false;
    }

    fn add_warning(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    /// Report validation results to the log.
    pub fn log(&self) {
        if self.errors.is_empty() && self.warnings.is_empty() {
            info!("Topology validation: PASSED (no issues)");
            return;
        }

        for error in &self.errors {
            warn!(target: "topology_validator", error = %error, "Topology validation ERROR");
        }
        for warning in &self.warnings {
            info!(target: "topology_validator", warning = %warning, "Topology validation WARNING");
        }

        if self.valid {
            info!("Topology validation: PASSED with {} warning(s)", self.warnings.len());
        } else {
            warn!("Topology validation: FAILED — {} error(s), {} warning(s)", self.errors.len(), self.warnings.len());
        }
    }
}

/// Validate the worker topology for a given set of enabled worker types.
///
/// This is intended to be called at startup to detect configuration issues early.
/// It checks:
/// 1. `instance_map_keys` are unique across worker types
/// 2. `owned_route_prefixes` don't have conflicting overlaps (non-master)
/// 3. The master process is always present
/// 4. Stream writers are consistent
pub fn validate_topology(enabled_workers: &[WorkerType]) -> TopologyValidation {
    let mut result = TopologyValidation::new();

    // Rule 1: Master must always be present.
    if !enabled_workers.contains(&WorkerType::Master) {
        result.add_error("master is not present in the enabled worker set — at least one master process is required");
    }

    // Rule 2: instance_map_keys must be unique.
    {
        let mut seen_keys = HashSet::new();
        for w in enabled_workers {
            for key in w.instance_map_keys() {
                if !seen_keys.insert(key) {
                    result.add_error(format!(
                        "duplicate instance_map_key '{}' used by worker type '{}' and another worker",
                        key,
                        w.as_str()
                    ));
                }
            }
        }
    }

    // Rule 3: Non-master workers must not have conflicting route prefix ownership.
    // Only warn about overlaps, since sub-prefix routing (e.g. Frontend owns
    // /_matrix/client/* and Synchrotron owns /_matrix/client/*/sync) is
    // intentional in a properly configured reverse-proxy topology.
    {
        let mut owned_prefixes: Vec<(WorkerType, &str)> = Vec::new();
        for w in enabled_workers {
            if w == &WorkerType::Master {
                continue; // master owns everything by definition
            }
            for prefix in w.owned_route_prefixes() {
                owned_prefixes.push((*w, prefix));
            }
        }

        for i in 0..owned_prefixes.len() {
            for j in (i + 1)..owned_prefixes.len() {
                let (wa, pa) = owned_prefixes[i];
                let (wb, pb) = owned_prefixes[j];
                if wa == wb {
                    continue; // same worker type can have multiple instances
                }
                // Only flag if two different worker types claim the exact same
                // prefix. Sub-prefix overlaps (e.g. /_matrix/client/* vs
                // /_matrix/client/*/sync) are intentional and handled by the
                // reverse proxy.
                if pa == pb {
                    result.add_warning(format!(
                        "overlapping route prefix '{}' owned by both '{}' and '{}' — ensure the reverse proxy routes correctly",
                        pa, wa.as_str(), wb.as_str()
                    ));
                }
            }
        }
    }

    result
}

fn worker_type_for_instance_name(instance_name: &str) -> Option<WorkerType> {
    WorkerType::all().into_iter().find(|worker_type| {
        worker_type.instance_map_keys().iter().any(|key| {
            instance_name == *key
                || instance_name
                    .strip_prefix(key)
                    .is_some_and(|suffix| suffix.starts_with('-') || suffix.starts_with('_'))
        })
    })
}

fn is_worker_instance(instance_name: &str, baseline_name: &str) -> bool {
    instance_name == baseline_name
        || instance_name
            .strip_prefix(baseline_name)
            .is_some_and(|suffix| suffix.starts_with('-') || suffix.starts_with('_'))
}

pub fn resolved_current_instance_name(config: &WorkerConfig) -> String {
    if !config.enabled {
        return "master".to_string();
    }

    let configured = config.instance_name.trim();
    if configured.contains("${") {
        std::env::var("WORKER_INSTANCE_NAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| configured.to_string())
    } else {
        configured.to_string()
    }
}

fn stream_writer_sets(config: &WorkerConfig) -> [(&'static str, &Vec<String>); 8] {
    [
        ("events", &config.stream_writers.events),
        ("typing", &config.stream_writers.typing),
        ("to_device", &config.stream_writers.to_device),
        ("account_data", &config.stream_writers.account_data),
        ("receipts", &config.stream_writers.receipts),
        ("presence", &config.stream_writers.presence),
        ("push_rules", &config.stream_writers.push_rules),
        ("device_lists", &config.stream_writers.device_lists),
    ]
}

fn allowed_stream_writer_types(stream_name: &str) -> &'static [WorkerType] {
    match stream_name {
        "events" => &[WorkerType::Master, WorkerType::EventPersister],
        "typing" | "to_device" | "account_data" | "receipts" | "presence" | "push_rules" | "device_lists" => {
            &[WorkerType::Master]
        }
        _ => &[WorkerType::Master],
    }
}

fn configured_worker_types(config: &WorkerConfig) -> Vec<WorkerType> {
    let mut inferred_workers = Vec::new();
    let mut known_instances: HashSet<String> =
        HashSet::from(["master".to_string(), resolved_current_instance_name(config)]);
    known_instances.extend(config.instance_map.keys().cloned());

    for instance_name in &known_instances {
        if let Some(worker_type) = worker_type_for_instance_name(instance_name) {
            if !inferred_workers.contains(&worker_type) {
                inferred_workers.push(worker_type);
            }
        }
    }

    inferred_workers
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteOwnerProbe {
    Sync,
    Media,
    Federation,
}

impl RouteOwnerProbe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Media => "media",
            Self::Federation => "federation",
        }
    }

    pub fn path(&self) -> &'static str {
        match self {
            Self::Sync => "/_matrix/client/v3/sync",
            Self::Media => "/_matrix/media/v3/config",
            Self::Federation => "/_matrix/federation/v1/version",
        }
    }

    fn preferred_worker_type(&self) -> WorkerType {
        match self {
            Self::Sync => WorkerType::Synchrotron,
            Self::Media => WorkerType::MediaRepository,
            Self::Federation => WorkerType::FederationReader,
        }
    }
}

pub fn expected_route_owner_for_probe(config: &WorkerConfig, probe: RouteOwnerProbe) -> WorkerType {
    if !config.enabled {
        return WorkerType::Master;
    }

    let preferred_worker_type = probe.preferred_worker_type();
    if configured_worker_types(config).contains(&preferred_worker_type) {
        preferred_worker_type
    } else {
        WorkerType::Master
    }
}

pub fn current_instance_worker_type(config: &WorkerConfig) -> WorkerType {
    if !config.enabled {
        return WorkerType::Master;
    }

    worker_type_for_instance_name(&resolved_current_instance_name(config)).unwrap_or(WorkerType::Master)
}

pub fn global_maintenance_owner(config: &WorkerConfig) -> WorkerType {
    if !config.enabled {
        return WorkerType::Master;
    }

    let has_dedicated_background_worker = std::iter::once(resolved_current_instance_name(config))
        .chain(config.instance_map.keys().cloned())
        .any(|instance_name| is_worker_instance(&instance_name, "background_worker"));

    if has_dedicated_background_worker {
        WorkerType::Background
    } else {
        WorkerType::Master
    }
}

pub fn should_run_global_maintenance(config: &WorkerConfig) -> bool {
    current_instance_worker_type(config) == global_maintenance_owner(config)
}

/// Validate the configured worker topology using the actual `worker` config.
///
/// This supplements `validate_topology(...)` by checking:
/// 1. stream writer owners reference known instances
/// 2. multi-worker setups enable replication
/// 3. replication HTTP listener has a configured secret
/// 4. worker topology is inferred from actual instance names when possible
pub fn validate_worker_config(config: &WorkerConfig) -> TopologyValidation {
    let inferred_workers = configured_worker_types(config);
    let mut validation = TopologyValidation::new();

    if !config.enabled {
        return validate_topology(&[WorkerType::Master]);
    }

    let mut known_instances: HashSet<String> =
        HashSet::from(["master".to_string(), resolved_current_instance_name(config)]);
    known_instances.extend(config.instance_map.keys().cloned());

    for instance_name in &known_instances {
        if worker_type_for_instance_name(instance_name).is_none() {
            validation.add_warning(format!(
                "instance '{}' does not map to a known worker type baseline; route ownership checks may be incomplete",
                instance_name
            ));
        }
    }

    if !config.instance_map.is_empty() && !config.replication.enabled {
        validation.add_error(
            "worker.instance_map is configured but worker.replication.enabled is false — multi-worker topology requires replication",
        );
    }

    if config.replication.http.enabled
        && config.replication.http.secret.is_none()
        && config.replication.http.secret_path.is_none()
    {
        validation.add_error(
            "worker.replication.http.enabled is true but neither worker.replication.http.secret nor secret_path is configured",
        );
    }

    for (stream_name, owners) in stream_writer_sets(config) {
        if owners.is_empty() {
            validation.add_error(format!("stream writer '{}' has no configured owners", stream_name));
            continue;
        }

        for owner in owners {
            if !known_instances.contains(owner) {
                validation.add_error(format!(
                    "stream writer '{}' references unknown instance '{}' (known: {})",
                    stream_name,
                    owner,
                    {
                        let mut names: Vec<_> = known_instances.iter().cloned().collect();
                        names.sort();
                        names.join(", ")
                    }
                ));
                continue;
            }

            if let Some(owner_type) = worker_type_for_instance_name(owner) {
                let allowed_types = allowed_stream_writer_types(stream_name);
                if !allowed_types.contains(&owner_type) {
                    let allowed = allowed_types.iter().map(WorkerType::as_str).collect::<Vec<_>>().join(", ");
                    validation.add_error(format!(
                        "stream writer '{}' is assigned to instance '{}' (type '{}') but current topology baseline only allows: {}",
                        stream_name,
                        owner,
                        owner_type.as_str(),
                        allowed
                    ));
                }
            } else {
                validation.add_warning(format!(
                    "stream writer '{}' uses instance '{}' whose worker type cannot be inferred; capability validation skipped",
                    stream_name,
                    owner
                ));
            }
        }
    }

    let topology_validation = validate_topology(&inferred_workers);
    validation.valid &= topology_validation.valid;
    validation.errors.extend(topology_validation.errors);
    validation.warnings.extend(topology_validation.warnings);
    if !validation.errors.is_empty() {
        validation.valid = false;
    }

    validation
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use synapse_common::config::worker::{
        InstanceLocationConfig, ReplicationConfig, ReplicationHttpConfig, StreamWriters, WorkerConfig,
    };

    #[test]
    fn test_validate_monolith_is_valid() {
        let workers = vec![WorkerType::Master];
        let result = validate_topology(&workers);
        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_missing_master_is_error() {
        let workers = vec![WorkerType::Frontend];
        let result = validate_topology(&workers);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("master")));
    }

    #[test]
    fn test_validate_split_minimal_is_valid() {
        let workers = vec![
            WorkerType::Master,
            WorkerType::Frontend,
            WorkerType::Synchrotron,
            WorkerType::EventPersister,
            WorkerType::FederationReader,
            WorkerType::FederationSender,
            WorkerType::MediaRepository,
            WorkerType::Background,
        ];
        let result = validate_topology(&workers);
        assert!(result.valid);
        assert!(result.errors.is_empty());
        // Sub-prefix overlaps (e.g. Frontend /_matrix/client/* vs Synchrotron
        // /_matrix/client/*/sync) are expected and should not produce errors,
        // only informational warnings.
    }

    #[test]
    fn test_validate_duplicate_instance_map_keys() {
        let workers = vec![
            WorkerType::Master,
            WorkerType::Frontend,
            // Two frontends share the same instance_map_key — this is a duplicate
            WorkerType::Frontend,
        ];
        let result = validate_topology(&workers);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("instance_map_key")));
    }

    #[test]
    fn test_validate_master_with_pusher_is_valid() {
        // Master handles background_jobs, so a separate background worker is
        // not required for pusher to function.
        let workers = vec![WorkerType::Master, WorkerType::Pusher];
        let result = validate_topology(&workers);
        assert!(result.valid);
    }

    #[test]
    fn test_validate_master_with_synchrotron_is_valid() {
        // Master handles event_persistence, so a separate event_persister is
        // not required for synchrotron to function.
        let workers = vec![WorkerType::Master, WorkerType::Synchrotron];
        let result = validate_topology(&workers);
        assert!(result.valid);
    }

    #[test]
    fn test_validate_log_does_not_panic() {
        let workers = vec![WorkerType::Master];
        let result = validate_topology(&workers);
        result.log();
    }

    #[test]
    fn test_validate_worker_config_default_monolith_is_valid() {
        let result = validate_worker_config(&WorkerConfig::default());
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_worker_config_detects_unknown_stream_writer_owner() {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.stream_writers.events = vec!["event_persister".to_string()];

        let result = validate_worker_config(&config);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("unknown instance 'event_persister'")));
    }

    #[test]
    fn test_validate_worker_config_requires_replication_for_multi_worker_topology() {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "client_reader-1".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8101, tls: false },
        );

        let result = validate_worker_config(&config);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("requires replication")));
    }

    #[test]
    fn test_validate_worker_config_requires_replication_secret_when_http_is_enabled() {
        let config = WorkerConfig {
            enabled: true,
            instance_name: "master".to_string(),
            replication: ReplicationConfig {
                enabled: true,
                server_name: "localhost".to_string(),
                http: ReplicationHttpConfig {
                    enabled: true,
                    host: "127.0.0.1".to_string(),
                    port: 9093,
                    secret: None,
                    secret_path: None,
                },
            },
            ..WorkerConfig::default()
        };

        let result = validate_worker_config(&config);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("secret")));
    }

    #[test]
    fn test_validate_worker_config_rejects_event_stream_owned_by_frontend() {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "client_reader-1".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8101, tls: false },
        );
        config.stream_writers.events = vec!["client_reader-1".to_string()];
        config.replication.enabled = true;

        let result = validate_worker_config(&config);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("only allows: master, event_persister")));
    }

    #[test]
    fn test_validate_worker_config_rejects_non_master_typing_stream_owner() {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "sync_worker".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8103, tls: false },
        );
        config.stream_writers.typing = vec!["sync_worker".to_string()];
        config.replication.enabled = true;

        let result = validate_worker_config(&config);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("stream writer 'typing'")));
        assert!(result.errors.iter().any(|e| e.contains("only allows: master")));
    }

    #[test]
    fn test_validate_worker_config_accepts_split_minimal_like_setup() {
        let mut instance_map = HashMap::new();
        instance_map.insert(
            "client_reader-1".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8101, tls: false },
        );
        instance_map.insert(
            "sync_worker".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8103, tls: false },
        );
        instance_map.insert(
            "event_persister".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 9102, tls: false },
        );

        let config = WorkerConfig {
            enabled: true,
            instance_name: "master".to_string(),
            instance_map,
            stream_writers: StreamWriters {
                events: vec!["event_persister".to_string()],
                typing: vec!["master".to_string()],
                to_device: vec!["master".to_string()],
                account_data: vec!["master".to_string()],
                receipts: vec!["master".to_string()],
                presence: vec!["master".to_string()],
                push_rules: vec!["master".to_string()],
                device_lists: vec!["master".to_string()],
            },
            replication: ReplicationConfig {
                enabled: true,
                server_name: "localhost".to_string(),
                http: ReplicationHttpConfig {
                    enabled: true,
                    host: "127.0.0.1".to_string(),
                    port: 9093,
                    secret: Some("test-secret".to_string()),
                    secret_path: None,
                },
            },
            ..WorkerConfig::default()
        };

        let result = validate_worker_config(&config);
        assert!(result.valid, "expected config to be valid, got errors: {:?}", result.errors);
    }

    #[test]
    fn test_expected_route_owner_for_probe_defaults_to_master_in_monolith() {
        let config = WorkerConfig::default();

        assert_eq!(expected_route_owner_for_probe(&config, RouteOwnerProbe::Sync), WorkerType::Master);
        assert_eq!(expected_route_owner_for_probe(&config, RouteOwnerProbe::Media), WorkerType::Master);
        assert_eq!(expected_route_owner_for_probe(&config, RouteOwnerProbe::Federation), WorkerType::Master);
    }

    #[test]
    fn test_expected_route_owner_for_probe_prefers_specialized_workers_when_configured() {
        let mut instance_map = HashMap::new();
        instance_map.insert(
            "sync_worker".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8103, tls: false },
        );
        instance_map.insert(
            "media_repository".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8104, tls: false },
        );
        instance_map.insert(
            "federation_reader".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8449, tls: false },
        );

        let config = WorkerConfig {
            enabled: true,
            instance_name: "master".to_string(),
            instance_map,
            replication: ReplicationConfig {
                enabled: true,
                server_name: "localhost".to_string(),
                http: ReplicationHttpConfig {
                    enabled: true,
                    host: "127.0.0.1".to_string(),
                    port: 9093,
                    secret: Some("test-secret".to_string()),
                    secret_path: None,
                },
            },
            ..WorkerConfig::default()
        };

        assert_eq!(expected_route_owner_for_probe(&config, RouteOwnerProbe::Sync), WorkerType::Synchrotron);
        assert_eq!(expected_route_owner_for_probe(&config, RouteOwnerProbe::Media), WorkerType::MediaRepository);
        assert_eq!(expected_route_owner_for_probe(&config, RouteOwnerProbe::Federation), WorkerType::FederationReader);
    }

    #[test]
    fn test_current_instance_worker_type_defaults_to_master_for_monolith() {
        let config = WorkerConfig::default();

        assert_eq!(current_instance_worker_type(&config), WorkerType::Master);
    }

    #[test]
    fn test_current_instance_worker_type_uses_instance_name_when_workers_enabled() {
        let config =
            WorkerConfig { enabled: true, instance_name: "sync_worker".to_string(), ..WorkerConfig::default() };

        assert_eq!(current_instance_worker_type(&config), WorkerType::Synchrotron);
    }

    #[test]
    fn test_current_instance_worker_type_prefers_worker_instance_env_for_placeholders() {
        unsafe {
            std::env::set_var("WORKER_INSTANCE_NAME", "background_worker");
        }
        let config = WorkerConfig {
            enabled: true,
            instance_name: "${WORKER_INSTANCE_NAME:-master}".to_string(),
            ..WorkerConfig::default()
        };

        assert_eq!(resolved_current_instance_name(&config), "background_worker");
        assert_eq!(current_instance_worker_type(&config), WorkerType::Background);

        unsafe {
            std::env::remove_var("WORKER_INSTANCE_NAME");
        }
    }

    #[test]
    fn test_global_maintenance_owner_prefers_background_worker_when_present() {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "background_worker".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8105, tls: false },
        );

        assert_eq!(global_maintenance_owner(&config), WorkerType::Background);
        assert!(!should_run_global_maintenance(&config));
    }
}
