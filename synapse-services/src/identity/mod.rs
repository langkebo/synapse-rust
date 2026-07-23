pub mod models;
pub mod service;
pub mod storage;

pub use service::IdentityService;
pub use storage::IdentityStorage;

// Identity domain group — re-exports oidc_service under `identity::`.
pub use crate::oidc_service::OidcService;

// P7.4 — additional identity-domain service re-export (previously flat in lib.rs).
#[cfg(feature = "builtin-oidc")]
pub use crate::builtin_oidc_provider::{AuthSession, BuiltinOidcProvider, RefreshToken as BuiltinRefreshToken};
