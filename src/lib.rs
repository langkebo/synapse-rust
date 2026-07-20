// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub use synapse_services::auth;
pub mod cache;
pub mod common;
pub mod e2ee;
pub mod federation;
pub mod server;
pub mod services;
pub mod storage;
pub mod tasks;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
pub mod web;
pub mod worker;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_config;

// Explicit root re-exports (replacing the former per-module wildcard globs).
// Only the items consumed through the crate root (`synapse_rust::Foo`) are
// re-exported here; everything else is reached path-qualified
// (e.g. `synapse_rust::cache::CacheManager`).
pub use common::{config, error, metrics, ApiError, PresenceState};
pub use e2ee::backup::KeyBackupService;
pub use e2ee::device_keys::DeviceKeyService;
pub use e2ee::megolm::{EncryptedEvent, MegolmSession};
pub use e2ee::signature::{EventSignature, SignatureService};
pub use server::SynapseServer;
pub use storage::presence::PresenceStorage;
pub use synapse_common::{
    impl_api_error, map_bad_request, map_forbidden, map_internal, map_not_found, map_unauthorized,
};
pub use synapse_e2ee::cross_signing;
pub use tasks::{ScheduledTasks, TaskMetricsCollector};
