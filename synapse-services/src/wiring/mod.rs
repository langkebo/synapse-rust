// =============================================================================
// Wiring — decomposed service-group assemblers extracted from container.rs
// =============================================================================

pub mod accounts;
pub mod admin;
pub mod core;
mod e2ee;
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
