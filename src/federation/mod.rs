pub mod device_sync;
pub mod event_auth;
pub mod key_rotation;
pub mod memory_tracker;

// Friend federation submodule
pub mod friend;

pub use device_sync::DeviceSyncManager;
pub use event_auth::EventAuthChain;
pub use key_rotation::KeyRotationManager;
pub use memory_tracker::{FederationMemoryReport, FederationMemoryTracker, MemoryStats};

// Re-export friend federation types
pub use friend::friend_federation::*;
pub use friend::friend_queries::*;
