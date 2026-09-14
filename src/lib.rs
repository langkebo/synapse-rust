//! The `synapse-rust` root crate: main binary entry point, route handlers,
//! service wiring, and the re-export surface for the synapse-rust homeserver.
//! Aggregates all sibling crates (cache, common, e2ee, federation, services,
//! storage) into a single deployable homeserver binary.

// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// B-3.1-b-7: synapse-rust root fully documented + deny(missing_docs).
// ratchet baseline tracked in scripts/quality/check_missing_docs_ratchet.sh;
// this crate is now at zero missing-doc warnings under `cargo doc`.
#![deny(missing_docs)]

pub use synapse_services::auth;
/// The `cache` module.
pub mod cache;
/// The `common` module.
pub mod common;
/// The `e2ee` module.
pub mod e2ee;
/// The `federation` module.
pub mod federation;
/// The `server` module.
pub mod server;
/// The `storage` module.
pub mod storage;
/// The `tasks` module.
pub mod tasks;
/// The `test_utils` module.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
/// The `web` module.
pub mod web;
/// The `worker` module.
pub mod worker;

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
pub use synapse_common::{map_database, map_internal};
pub use synapse_e2ee::cross_signing;
pub use tasks::ScheduledTasks;
