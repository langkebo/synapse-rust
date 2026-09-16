//! Application services domain group.
//!
//! Re-exports application-related service modules (application_service,
//! module_service) under a single namespace so that new application services
//! can be added here without touching `lib.rs`.
//!
//! Consumers use the grouped path:
//! - `synapse_services::application::ApplicationServiceManager`

pub use crate::application_service::{ApplicationServiceManager, ApplicationServiceScheduler, NamespacesInfo};
pub use crate::module_service::*;
