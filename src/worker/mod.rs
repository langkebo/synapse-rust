// Re-export worker types directly from source crates.
pub use synapse_services::worker::*;

/// S4: worker-side heartbeat sender + load-stats collector (worker binary support).
pub mod heartbeat;
