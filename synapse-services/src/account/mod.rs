//! Account services domain group.
//!
//! Re-exports account-related service modules (account_device_list_service,
//! account_identity_service, registration_service, etc.) under a single
//! namespace so that new account services can be added here without touching
//! `lib.rs`.
//!
//! Consumers may use either:
//! - `synapse_services::account::RegistrationService` (preferred, grouped path)
//! - `synapse_services::RegistrationService` (legacy flat path, via `pub use account::*` in lib.rs)

pub use crate::account_device_list_service::{
    AccountDeviceListService, DeviceListDeletion, DeviceListDelta, DeviceListEntry, DeviceListSnapshot,
};
pub use crate::account_identity_service::AccountIdentityService;
pub use crate::registration_service::RegistrationService;

// P7.4 — additional account-domain service re-exports (previously flat in lib.rs).
pub use crate::account_data_service::*;
pub use crate::captcha_service::*;
pub use crate::dehydrated_device_service::DehydratedDeviceService;
pub use crate::refresh_token_service::*;
pub use crate::registration_token_service::*;
pub use crate::sms_provider::*;
pub use crate::uia_service::*;
pub use crate::user_lock_service::*;
pub use crate::user_service::UserService;
