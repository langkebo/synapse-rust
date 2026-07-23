pub mod models;
pub mod service;
pub mod storage;

pub use service::IdentityService;
pub use storage::IdentityStorage;

// Identity domain group — re-exports oidc_service under `identity::`.
pub use crate::oidc_service::OidcService;
