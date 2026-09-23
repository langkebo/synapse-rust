// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// B-3.1-b-4: synapse-federation fully documented + deny(missing_docs).
// ratchet baseline tracked in scripts/quality/check_missing_docs_ratchet.sh;
// this crate is now at zero missing-doc warnings under `cargo doc`.
#![deny(missing_docs)]
//! Matrix federation (server-to-server) protocol implementation.
//!
//! Implements the Matrix spec's S2S federation protocol over the synapse-rust
//! homeserver: PDU/EDU production, origin-server signature verification
//! (canonical JSON + ed25519), key-rotation and federation key queries,
//! state resolution, server-ACL enforcement, server discovery (`well-known`),
//! federation dead-letter queue for retries, and a friends opt-in feature
//! behind the `friends` Cargo feature flag. Public surface is split into
// per-feature modules; consumers normally reach it through the
// high-level `*Client` / `*Manager` types re-exported below.

/// The `client` module.
pub mod client;
/// The `client_api` module.
pub mod client_api;
/// The `dead_letter_queue` module.
pub mod dead_letter_queue;
/// The `device_sync` module.
pub mod device_sync;
/// The `edu` module.
pub mod edu;
/// The `event_auth` module.
pub mod event_auth;
/// The `event_broadcaster` module.
pub mod event_broadcaster;
#[cfg(feature = "friends")]
/// The `friend` module.
pub mod friend;
/// The `key_rotation` module.
pub mod key_rotation;
/// The `memory_tracker` module.
pub mod memory_tracker;

pub mod make_response_validation;
/// The `server_acl` module.
pub mod server_acl;
/// The `signing` module.
pub mod signing;
/// The `state_resolution` module.
pub mod state_resolution;
#[cfg(any(test, feature = "test-utils"))]
/// The `test_mocks` module.
pub mod test_mocks;

pub use client::FederationClient;
pub use dead_letter_queue::{
    DeadLetterQueueApi, DeadLetterQueueError, DlqEntry, InMemoryDeadLetterQueue, PgDeadLetterQueue,
};
pub use device_sync::DeviceSyncManager;
pub use event_auth::EventAuthChain;
pub use event_broadcaster::EventBroadcaster;
#[cfg(feature = "friends")]
pub use friend::*;
pub use key_rotation::{KeyRotationManager, KeyRotationManagerApi};
pub use memory_tracker::{FederationMemoryReport, FederationMemoryTracker, MemoryStats};
pub use server_acl::ServerAclContent;
