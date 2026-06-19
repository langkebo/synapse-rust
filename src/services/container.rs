//! ServiceContainer — composition root.
//! Thin facade re-exporting the canonical implementation from
//! `synapse_services::container`. This eliminates the structural bottleneck
//! of maintaining two parallel implementations of the composition root.
//!
//! The canonical implementation lives in `synapse-services/src/container.rs`
//! and owns all struct definitions, assembly functions, and the `ServiceContainer`
//! impl block (including `new()`, `new_test*()`, `database_pool()`, etc.).

pub use synapse_services::container::*;
