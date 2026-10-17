//! Multi-instance naming (audit V-W-2): do `client_reader-1` / `client_reader_2`
//! style names still classify as the worker type they are derived from?
//!
//! Why this needed checking rather than assuming: the classification is a
//! **prefix** match (`topology_validator.rs`, `worker_type_for_instance_name`), and
//! everything downstream hangs off it — `current_instance_worker_type` decides
//! which routes an instance claims, `global_maintenance_owner` decides which
//! instance runs pruning and the app-service scheduler, and the new reference
//! multi-process stack (W-D) names its services from the same table. A suffix rule
//! that only accepted `-1` and not `_1`, or that matched `client_readerx`, would
//! quietly turn a scaled-out deployment into "everything falls back to master".
//!
//! The suffix must start with `-` or `_`: `client_readerx` is a *different* name,
//! not the second instance of `client_reader`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use synapse_common::config::worker::{InstanceLocationConfig, WorkerConfig};
use synapse_services::worker::topology_validator::{
    current_instance_worker_type, global_maintenance_owner, should_run_global_maintenance,
};
use synapse_storage::worker::WorkerType;

/// A worker-enabled configuration whose own instance name is `instance_name`.
fn config_named(instance_name: &str) -> WorkerConfig {
    WorkerConfig { enabled: true, instance_name: instance_name.to_string(), ..WorkerConfig::default() }
}

fn location() -> InstanceLocationConfig {
    InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8008, tls: false }
}

#[test]
fn every_worker_type_is_recognised_by_its_plain_instance_name() {
    // The baseline the suffixes are compared against: each type's canonical name
    // must classify as itself, otherwise the suffix cases below prove nothing.
    for (name, expected) in [
        ("master", WorkerType::Master),
        ("client_reader", WorkerType::Frontend),
        ("event_persister", WorkerType::EventPersister),
        ("sync_worker", WorkerType::Synchrotron),
        ("federation_sender", WorkerType::FederationSender),
        ("federation_reader", WorkerType::FederationReader),
        ("media_repository", WorkerType::MediaRepository),
        ("pusher", WorkerType::Pusher),
        ("appservice_worker", WorkerType::AppService),
        ("background_worker", WorkerType::Background),
    ] {
        assert_eq!(
            current_instance_worker_type(&config_named(name)),
            expected,
            "'{name}' must classify as {expected:?}"
        );
    }
}

#[test]
fn numbered_and_suffixed_instances_classify_as_their_base_type() {
    // V-W-2's actual question, both separator spellings, for every type.
    for (name, expected) in [
        ("client_reader-1", WorkerType::Frontend),
        ("client_reader-2", WorkerType::Frontend),
        ("client_reader_17", WorkerType::Frontend),
        ("event_persister-3", WorkerType::EventPersister),
        ("event_persister_4", WorkerType::EventPersister),
        ("sync_worker-2", WorkerType::Synchrotron),
        ("federation_reader-9", WorkerType::FederationReader),
        ("federation_sender-1", WorkerType::FederationSender),
        ("media_repository_5", WorkerType::MediaRepository),
        ("pusher-1", WorkerType::Pusher),
        ("appservice_worker-1", WorkerType::AppService),
        ("background_worker-2", WorkerType::Background),
    ] {
        assert_eq!(
            current_instance_worker_type(&config_named(name)),
            expected,
            "'{name}' must classify as {expected:?}"
        );
    }
}

#[test]
fn a_name_that_merely_starts_with_a_base_name_is_not_that_type() {
    // The suffix rule is what makes the naming scheme safe: without it,
    // `client_readerx` (a typo, or an unrelated service) would silently be treated
    // as a frontend worker and inherit its route ownership.
    for name in ["client_readerx", "client_readerX-1", "event_persister2", "sync_workerfoo"] {
        assert_eq!(
            current_instance_worker_type(&config_named(name)),
            WorkerType::Master,
            "'{name}' is not a numbered instance of any known type, so it must fall back to Master"
        );
    }
}

#[test]
fn an_unknown_instance_name_falls_back_to_master() {
    // The documented fallback: an unrecognised name is not an error, it is master.
    // `validate_worker_config` reports it as a *warning* instead.
    assert_eq!(current_instance_worker_type(&config_named("definitely_not_a_worker")), WorkerType::Master);
}

#[test]
fn a_disabled_worker_block_is_always_master_regardless_of_the_name() {
    // Monolith safety: nothing about the naming scheme may change behaviour when
    // workers are off.
    let mut config = config_named("sync_worker-3");
    config.enabled = false;

    assert_eq!(current_instance_worker_type(&config), WorkerType::Master);
}

#[test]
fn global_maintenance_follows_the_suffixed_background_worker_name() {
    // V-W-2's consequence for duties: `global_maintenance_owner` uses the same
    // prefix rule (`is_worker_instance`), so a *numbered* background worker owns
    // maintenance. This is exactly the trap W-D documents — declaring
    // `background_worker-*` hands the work to that instance, so whoever declares it
    // must run the **root** binary under that name.
    let mut instance_map = HashMap::new();
    instance_map.insert("background_worker-2".to_string(), location());
    let mut config = config_named("master");
    config.instance_map = instance_map;

    assert_eq!(global_maintenance_owner(&config), WorkerType::Background);
    assert!(!should_run_global_maintenance(&config), "the master must stand down once a background worker is declared");

    // …and the declared background worker itself is the one that must run it.
    assert!(should_run_global_maintenance(&config_named("background_worker-2")));
}
