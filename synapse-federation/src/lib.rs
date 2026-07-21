// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used))]

pub mod client;
pub mod client_api;
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
pub use device_sync::DeviceSyncManager;
pub use event_auth::EventAuthChain;
pub use event_broadcaster::EventBroadcaster;
#[cfg(feature = "friends")]
pub use friend::*;
pub use key_rotation::{KeyRotationManager, KeyRotationManagerApi};
pub use memory_tracker::{FederationMemoryReport, FederationMemoryTracker, MemoryStats};
pub use server_acl::ServerAclContent;
