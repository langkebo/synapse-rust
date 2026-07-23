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
