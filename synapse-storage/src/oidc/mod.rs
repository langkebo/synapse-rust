//! OIDC storage domain group.
//!
//! Re-exports OIDC/OAuth-related storage modules (`oauth_client_storage`,
//! `oidc_session_storage`, `oidc_user_mapping`) under a single namespace so
//! that new OIDC storage modules can be added here without touching `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::oidc::OidcUserMappingStorage` over
//! the flat `synapse_storage::OidcUserMappingStorage`.

pub use crate::oauth_client_storage::OAuthClientStoreApi;
pub use crate::oidc_session_storage::OidcSessionStoreApi;
pub use crate::oidc_user_mapping::{OidcUserMappingStorage, OidcUserMappingStoreApi};
