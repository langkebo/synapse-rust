// OIDC (OpenID Connect) routes
// Matrix Spec: <https://matrix.org/docs/spec/openid.html>

/// The `builtin` module.
pub(crate) mod builtin;
/// The `provider` module.
pub(crate) mod provider;
/// The `sso` module.
pub(crate) mod sso;

use crate::routes::context::SsoContext;
use crate::routes::AppState;
use axum::routing::{get, post};
use axum::Router;
use std::time::{SystemTime, UNIX_EPOCH};
use synapse_common::error::ApiError;
use synapse_services::oidc_service::OidcService;
use synapse_services::oidc_session_service::OidcAuthSession;

/// See [`current_unix_ts`].
pub(crate) fn current_unix_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

// ---------------------------------------------------------------------------
// Session management — shared by sso and provider submodules
// ---------------------------------------------------------------------------

/// See [`validate_state_pkce_binding`].
pub(crate) fn validate_state_pkce_binding(auth_session: &OidcAuthSession) -> Result<(), ApiError> {
    if auth_session.code_challenge_method != "S256" {
        return Err(ApiError::unauthorized("Unsupported OIDC PKCE challenge method".to_string()));
    }
    if auth_session.code_verifier.len() < 43 || auth_session.code_verifier.len() > 128 {
        return Err(ApiError::unauthorized("Invalid OIDC PKCE verifier length".to_string()));
    }
    if !OidcService::verify_pkce(&auth_session.code_verifier, &auth_session.code_challenge) {
        return Err(ApiError::unauthorized("OIDC state/PKCE binding validation failed".to_string()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Router factories
// ---------------------------------------------------------------------------

/// Build the OIDC router with all SSO, provider, and (when feature-gated)
/// built-in OIDC endpoints.
pub fn create_oidc_router(state: AppState) -> Router<AppState> {
    // `mut` needed when `builtin-oidc` or `saml-sso` feature is enabled; unused otherwise.
    #[allow(unused_mut)]
    let mut router = Router::new()
        .route("/_matrix/client/v3/login/sso/redirect", get(sso::sso_redirect))
        .route("/_matrix/client/v3/login/sso/userinfo", get(provider::oidc_userinfo))
        // v3 paths
        .route("/_matrix/client/v3/oidc/userinfo", get(provider::oidc_userinfo))
        .route("/_matrix/client/v3/oidc/token", post(provider::oidc_token))
        .route("/_matrix/client/v3/oidc/logout", post(provider::oidc_logout))
        .route("/_matrix/client/v3/oidc/authorize", get(provider::oidc_authorize))
        .route("/_matrix/client/v3/oidc/callback", get(sso::oidc_callback))
        // r0 compatibility paths
;

    // Built-in OIDC Provider endpoints
    #[cfg(feature = "builtin-oidc")]
    {
        router = router
            .route("/_matrix/client/v3/oidc/login", post(builtin::builtin_oidc_login))
            .route("/.well-known/openid-configuration", get(builtin::openid_discovery))
            .route("/.well-known/jwks.json", get(builtin::jwks));
    }
    router.with_state(state)
}

/// See [`oidc_enabled`].
pub fn oidc_enabled(ctx: &SsoContext) -> bool {
    #[cfg(feature = "saml-sso")]
    let saml_enabled = ctx.saml_service.is_enabled();
    #[cfg(not(feature = "saml-sso"))]
    let saml_enabled = false;

    ctx.oidc_service.is_some() || ctx.builtin_oidc_provider.is_some() || saml_enabled
}

/// See [`create_oidc_fallback_router`].
pub fn create_oidc_fallback_router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/openid-configuration", get(builtin::get_openid_configuration))
        .route("/.well-known/jwks.json", get(builtin::jwks_fallback))
}

// ---------------------------------------------------------------------------
// Route ledger manifests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_services::oidc_service::OidcService;

    #[test]
    fn validate_state_pkce_binding_accepts_valid_binding() {
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        let session = OidcAuthSession {
            nonce: "nonce".to_string(),
            code_verifier,
            code_challenge,
            code_challenge_method: "S256".to_string(),
            redirect_uri: "https://example.com/callback".to_string(),
        };

        assert!(validate_state_pkce_binding(&session).is_ok());
    }

    #[test]
    fn validate_state_pkce_binding_rejects_mismatched_challenge() {
        let (code_verifier, _) = OidcService::generate_pkce();
        let session = OidcAuthSession {
            nonce: "nonce".to_string(),
            code_verifier,
            code_challenge: "invalid_challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            redirect_uri: "https://example.com/callback".to_string(),
        };

        let error = validate_state_pkce_binding(&session).unwrap_err();
        assert!(error.to_string().contains("binding validation failed"));
    }
}
