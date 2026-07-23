//! Auth storage domain group.
//!
//! Re-exports authentication-related storage modules (user, device, token,
//! threepid, captcha, openid_token) under a single namespace so that new
//! auth storage modules can be added here without touching `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::auth::UserStorage` over the
//! flat `synapse_storage::UserStorage`.

pub use crate::captcha::{
    CaptchaConfig, CaptchaRateLimit, CaptchaSendLog, CaptchaStorage, CaptchaStoreApi, CaptchaTemplate,
    CreateCaptchaRequest, CreateSendLogRequest, RegistrationCaptcha,
};
pub use crate::device::{Device, DeviceListStoreApi, DeviceStorage};
pub use crate::openid_token::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStorage, OpenIdTokenStoreApi};
pub use crate::threepid::{
    CreateThreepidRequest, ThreepidStorage, ThreepidStoreApi, ThreepidValidationSession, UserThreepid,
};
pub use crate::token::{AccessToken, AccessTokenStorage, AccessTokenStoreApi};
pub use crate::user::{
    LockedUser, User, UserDirectorySearchResult, UserProfile, UserSearchResult, UserSearchResultWithPresence,
    UserStatsSummary, UserStorage, UserStore,
};

// P7.3: email_verification, refresh_token, registration_token, saml, cas, and
// privacy are auth-related storage modules — group them under `auth::` so
// they are flat-re-exported via `pub use auth::*;` rather than via explicit
// flat re-exports in lib.rs.
#[cfg(feature = "cas-sso")]
pub use crate::cas::{
    CasProxyGrantingTicket, CasProxyTicket, CasRegisteredService, CasSloSession, CasStorage, CasStoreApi, CasTicket,
    CasUserAttribute, CreatePgtRequest, CreateProxyTicketRequest, CreateTicketRequest, RegisterServiceRequest,
    ValidateTicketRequest,
};
pub use crate::email_verification::*;
#[cfg(feature = "privacy-ext")]
pub use crate::privacy::{
    CreatePrivacySettingsParams, PrivacySettingsUpdate, PrivacyStorage, PrivacyStoreApi, UserPrivacySettings,
};
pub use crate::refresh_token::*;
pub use crate::registration_token::*;
#[cfg(feature = "saml-sso")]
pub use crate::saml::{
    CreateSamlAuthEventRequest, CreateSamlIdentityProviderRequest, CreateSamlLogoutRequestRequest,
    CreateSamlSessionRequest, CreateSamlUserMappingRequest, SamlAuthEvent, SamlIdentityProvider, SamlLogoutRequest,
    SamlSession, SamlStorage, SamlStoreApi, SamlUserMapping,
};
