//! Application services domain group.
//!
//! Re-exports application-related service modules (application_service,
//! module_service) under a single namespace so that new application services
//! can be added here without touching `lib.rs`.
//!
//! Consumers may use either:
//! - `synapse_services::application::ApplicationServiceManager` (preferred, grouped path)
//! - `synapse_services::ApplicationServiceManager` (legacy flat path, via `pub use application::*` in lib.rs)

pub use crate::application_service::{ApplicationServiceManager, ApplicationServiceScheduler, NamespacesInfo};
pub use crate::module_service::*;
