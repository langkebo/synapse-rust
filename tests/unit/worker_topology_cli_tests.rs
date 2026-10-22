//! Offline topology check (`--validate-topology`, W-B): argument recognition and
//! exit codes.
//!
//! The exit code is the interface a deployment pipeline consumes, so it is pinned
//! here rather than left to the reader of `src/cli.rs`: 0 = deployable,
//! 1 = the configured topology has errors, 2 = the configuration could not be
//! loaded. The startup *enforcement* decision lives in
//! `synapse-services/src/worker/topology_gate.rs`, which has its own tests —
//! this file covers only the process-level contract.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use synapse_common::config::worker::WorkerConfig;
use synapse_rust::cli::{
    topology_exit_code, wants_topology_validation, TOPOLOGY_CONFIG_ERROR, TOPOLOGY_INVALID, TOPOLOGY_OK,
    VALIDATE_TOPOLOGY_FLAG,
};

/// A worker-enabled configuration the static validator rejects: `instance_map` is
/// populated while replication is off.
fn invalid_config() -> WorkerConfig {
    let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
    config.instance_map.insert(
        "event_persister".to_string(),
        synapse_common::config::worker::InstanceLocationConfig {
            host: "127.0.0.1".to_string(),
            port: 9102,
            tls: false,
        },
    );
    config
}

#[test]
fn the_flag_is_recognised_next_to_other_arguments() {
    assert!(wants_topology_validation(["/usr/bin/synapse-rust", VALIDATE_TOPOLOGY_FLAG]));
    assert!(wants_topology_validation([VALIDATE_TOPOLOGY_FLAG, "--verbose"]));
    // An empty argument list is the normal daemon start: no offline check.
    assert!(!wants_topology_validation(Vec::<String>::new()));
    assert!(!wants_topology_validation(["/usr/bin/synapse-rust"]));
}

#[test]
fn the_flag_is_matched_exactly_not_as_a_prefix() {
    // `--validate-topology-strict` (or any other flag merely starting with the
    // same characters) must not silently trigger the check: a typo would then
    // *skip* startup and exit 0, which looks like a healthy deploy.
    assert!(!wants_topology_validation(["--validate-topology-strict"]));
    assert!(!wants_topology_validation(["--validate"]));
    assert!(!wants_topology_validation(["--validate-topology=true"]));
}

#[test]
fn a_deployable_configuration_exits_zero() {
    // A monolith deployment is valid by construction, and is also the case where
    // the check matters least — pinned because exit 0 is what lets a pipeline
    // proceed.
    assert_eq!(topology_exit_code(&WorkerConfig::default(), true), TOPOLOGY_OK);
}

#[test]
fn a_broken_configuration_exits_non_zero() {
    assert_eq!(topology_exit_code(&invalid_config(), true), TOPOLOGY_INVALID);
}

#[test]
fn the_exit_codes_are_distinct_and_stable() {
    // The numeric values are the contract with CI; changing them silently would
    // turn "not deployable" into "looks fine" for anything that only checks == 0.
    assert_eq!(TOPOLOGY_OK, 0);
    assert_eq!(TOPOLOGY_INVALID, 1);
    assert_eq!(TOPOLOGY_CONFIG_ERROR, 2);
    assert_eq!(
        [TOPOLOGY_OK, TOPOLOGY_INVALID, TOPOLOGY_CONFIG_ERROR].iter().collect::<std::collections::HashSet<_>>().len(),
        3
    );
}
