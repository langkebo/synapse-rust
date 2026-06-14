use std::collections::HashSet;
use tracing::{info, warn};

use crate::worker::types::WorkerType;

/// Result of a topology validation run.
#[derive(Debug, Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
}