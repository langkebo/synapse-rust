//! Account — user identity, devices, tokens, presence.

use std::sync::Arc;

use synapse_storage::*;

use crate::UserService;

#[derive(Clone)]
pub struct AccountServices {
    pub account_device_list_service: Arc<crate::account_device_list_service::AccountDeviceListService>,
    pub account_identity_service: Arc<crate::account_identity_service::AccountIdentityService>,
    pub user_storage: Arc<dyn UserStore>,
    pub threepid_storage: ThreepidStorage,
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    pub token_storage: AccessTokenStorage,
    pub presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    pub presence_service: Arc<crate::presence_service::PresenceService>,
    pub qr_login_storage: Arc<dyn QrLoginStoreApi>,
    pub invite_blocklist_storage: Arc<dyn InviteBlocklistStoreApi>,
    pub sticky_event_storage: Arc<dyn StickyEventStoreApi>,
    pub user_service: Arc<UserService>,
}

/// Dependency bundle for [`AccountServices::new`].
pub struct AccountServicesDeps {
    pub pool: Arc<sqlx::PgPool>,
    pub user_storage: Arc<dyn UserStore>,
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    pub threepid_storage: ThreepidStorage,
    pub presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    pub presence_service: Arc<crate::presence_service::PresenceService>,
    pub qr_login_storage: Arc<dyn QrLoginStoreApi>,
    pub invite_blocklist_storage: Arc<dyn InviteBlocklistStoreApi>,
    pub sticky_event_storage: Arc<dyn StickyEventStoreApi>,
    pub account_device_list_service: Arc<crate::account_device_list_service::AccountDeviceListService>,
    pub account_identity_service: Arc<crate::account_identity_service::AccountIdentityService>,
    pub user_service: Arc<UserService>,
}

impl AccountServices {
    pub fn new(deps: AccountServicesDeps) -> Self {
        Self {
            account_device_list_service: deps.account_device_list_service,
            account_identity_service: deps.account_identity_service,
            user_storage: deps.user_storage,
            threepid_storage: deps.threepid_storage,
            device_storage: deps.device_storage,
            token_storage: AccessTokenStorage::new(&deps.pool),
            presence_storage: deps.presence_storage,
            presence_service: deps.presence_service,
            qr_login_storage: deps.qr_login_storage,
            invite_blocklist_storage: deps.invite_blocklist_storage,
            sticky_event_storage: deps.sticky_event_storage,
            user_service: deps.user_service,
        }
    }
}
