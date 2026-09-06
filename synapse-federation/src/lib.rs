// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// B2-TODO: ratchet in progress — see scripts/quality/check_missing_docs_ratchet.sh
// (current baseline 0, ticket 04 sets up the ratchet; ticket 05 will switch to deny).
// Currently emits a large volume of missing-docs warnings that drown out real
// warnings; doc debt is tracked separately in the B2 backlog.
#![warn(missing_docs)]

pub mod client;
pub mod client_api;
pub mod dead_letter_queue;
pub mod device_sync;
pub mod edu;
pub mod event_auth;
pub mod event_broadcaster;
#[cfg(feature = "friends")]
pub mod friend;
pub mod key_rotation;
pub mod memory_tracker;
pub mod server_acl;
pub mod signing;
pub mod state_resolution;
#[cfg(any(test, feature = "test-utils"))]
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
