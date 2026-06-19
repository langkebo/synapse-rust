pub use synapse_federation::client;
pub use synapse_federation::device_sync;
pub mod edu;
pub use synapse_federation::event_auth;
pub use synapse_federation::event_broadcaster;
#[cfg(feature = "friends")]
pub mod friend;
pub use synapse_federation::key_rotation;
pub use synapse_federation::memory_tracker;
pub use synapse_federation::signing;
pub use synapse_federation::state_resolution;

pub use client::FederationClient;
pub use device_sync::DeviceSyncManager;
pub use edu::{EduDispatcher, EduProcessResult, EduType};
pub use event_auth::EventAuthChain;
pub use event_broadcaster::EventBroadcaster;
#[cfg(feature = "friends")]
pub use friend::*;
pub use key_rotation::KeyRotationManager;
pub use memory_tracker::{FederationMemoryReport, FederationMemoryTracker, MemoryStats};
