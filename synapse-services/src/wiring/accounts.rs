//! Account — user identity, devices, tokens, presence.
//!
//! ARCH-07/08 (2026-08-10): The following storage fields are not accessed via
//! the container after construction but are retained as "backing storage":
//! - `qr_login_storage` — `QrLoginStorage` is accessed directly in tests;
//!   the container copy is not re-read.
//! - `sticky_event_storage` — consumed by `RoomSyncServices` during
//!   construction; the container copy is not re-read.

use std::sync::Arc;

use synapse_storage::*;

use crate::account::UserService;

/// The `AccountServices` struct.
#[derive(Clone)]
pub struct AccountServices {
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<crate::account_device_list_service::AccountDeviceListService>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<crate::account_identity_service::AccountIdentityService>,
    /// The `user_storage` field.
    pub user_storage: Arc<dyn UserStore>,
    /// The `threepid_storage` field.
    pub threepid_storage: Arc<dyn ThreepidStoreApi>,
    /// The `device_storage` field.
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    /// The `token_storage` field.
    pub token_storage: Arc<dyn AccessTokenStoreApi>,
    /// The `presence_storage` field.
    pub presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    /// The `presence_service` field.
    pub presence_service: Arc<crate::presence_service::PresenceService>,
    /// The `qr_login_storage` field.
    pub qr_login_storage: Arc<synapse_storage::qr_login::QrLoginStorage>,
    /// The `invite_blocklist_service` field.
    pub invite_blocklist_service: Arc<crate::invite_blocklist_service::InviteBlocklistService>,
    /// The `sticky_event_storage` field.
    pub sticky_event_storage: Arc<synapse_storage::sticky_event::StickyEventStorage>,
    /// The `user_service` field.
    pub user_service: Arc<UserService>,
}

/// Dependency bundle for [`AccountServices::new`].
pub struct AccountServicesDeps {
    /// The `pool` field.
    pub pool: Arc<sqlx::PgPool>,
    /// The `user_storage` field.
    pub user_storage: Arc<dyn UserStore>,
    /// The `device_storage` field.
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    /// The `threepid_storage` field.
    pub threepid_storage: Arc<dyn ThreepidStoreApi>,
    /// The `presence_storage` field.
    pub presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    /// The `presence_service` field.
    pub presence_service: Arc<crate::presence_service::PresenceService>,
    /// The `qr_login_storage` field.
    pub qr_login_storage: Arc<synapse_storage::qr_login::QrLoginStorage>,
    /// The `invite_blocklist_storage` field.
    pub invite_blocklist_storage: Arc<synapse_storage::invite_blocklist::InviteBlocklistStorage>,
    /// The `sticky_event_storage` field.
    pub sticky_event_storage: Arc<synapse_storage::sticky_event::StickyEventStorage>,
    /// The `account_device_list_service` field.
    pub account_device_list_service: Arc<crate::account_device_list_service::AccountDeviceListService>,
    /// The `account_identity_service` field.
    pub account_identity_service: Arc<crate::account_identity_service::AccountIdentityService>,
    /// The `user_service` field.
    pub user_service: Arc<UserService>,
}

impl AccountServices {
    /// See [`new`].
    pub fn new(deps: AccountServicesDeps) -> Self {
        Self {
            account_device_list_service: deps.account_device_list_service,
            account_identity_service: deps.account_identity_service,
            user_storage: deps.user_storage,
            threepid_storage: deps.threepid_storage,
            device_storage: deps.device_storage,
            token_storage: Arc::new(AccessTokenStorage::new(&deps.pool)),
            presence_storage: deps.presence_storage,
            presence_service: deps.presence_service,
            qr_login_storage: deps.qr_login_storage,
            invite_blocklist_service: Arc::new(crate::invite_blocklist_service::InviteBlocklistService::new(
                deps.invite_blocklist_storage,
            )),
            sticky_event_storage: deps.sticky_event_storage,
            user_service: deps.user_service,
        }
    }
}
