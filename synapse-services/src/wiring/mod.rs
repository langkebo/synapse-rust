// =============================================================================
// Wiring — decomposed service-group assemblers extracted from container.rs
// =============================================================================

/// The `accounts` module.
pub mod accounts;
/// The `admin` module.
pub mod admin;
/// The `core` module.
pub mod core;
mod e2ee;
/// The `extensions` module.
pub mod extensions;
mod federation;
mod rooms;
mod sso;

pub use accounts::{AccountServices, AccountServicesDeps};
pub use admin::{
    AdminFederationServices, AdminMediaServices, AdminModuleServices, AdminSecurityServices, AdminServices,
    AdminUserServices,
};
pub use core::CoreServices;
pub use e2ee::E2eeServices;
pub use extensions::{ExtensionServices, ExtensionServicesDeps};
pub use federation::FederationServices;
pub use rooms::RoomSyncServices;
pub use sso::SsoServices;
