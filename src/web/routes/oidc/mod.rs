// OIDC (OpenID Connect) routes
// Matrix Spec: https://matrix.org/docs/spec/openid.html

pub(crate) mod builtin;
pub(crate) mod provider;
pub(crate) mod sso;

pub(crate) use sso::format_token_response;

use crate::common::error::ApiError;
use crate::web::routes::context::SsoContext;
use crate::web::routes::AppState;
use axum::routing::{get, post};
use axum::Router;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use synapse_services::oidc_service::OidcService;

// ---------------------------------------------------------------------------
// Session management — shared by sso and provider submodules
// ---------------------------------------------------------------------------

const OIDC_AUTH_SESSION_TTL_SECONDS: u64 = 600;

#[derive(Debug, Clone)]
pub(crate) struct OidcAuthSession {
    pub(crate) nonce: String,
    pub(crate) code_verifier: String,
    pub(crate) code_challenge: String,
    pub(crate) code_challenge_method: String,
    pub(crate) redirect_uri: String,
    pub(crate) expires_at: u64,
}

static OIDC_AUTH_SESSIONS: OnceLock<Mutex<HashMap<String, OidcAuthSession>>> = OnceLock::new();

fn oidc_auth_sessions() -> &'static Mutex<HashMap<String, OidcAuthSession>> {
    OIDC_AUTH_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn current_unix_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn cleanup_expired_oidc_sessions(sessions: &mut HashMap<String, OidcAuthSession>, now: u64) {
    sessions.retain(|_, session| session.expires_at >= now);
}

pub(crate) fn store_oidc_auth_session(
    state: &str,
    nonce: &str,
    code_verifier: &str,
    code_challenge: &str,
    code_challenge_method: &str,
    redirect_uri: &str,
) -> Result<(), ApiError> {
    let now = current_unix_ts();
    let mut sessions = oidc_auth_sessions()
        .lock()
        .map_err(|e| ApiError::internal_with_log("Failed to acquire OIDC auth session lock", &e))?;
    cleanup_expired_oidc_sessions(&mut sessions, now);
    sessions.insert(
        state.to_string(),
        OidcAuthSession {
            nonce: nonce.to_string(),
            code_verifier: code_verifier.to_string(),
            code_challenge: code_challenge.to_string(),
            code_challenge_method: code_challenge_method.to_string(),
            redirect_uri: redirect_uri.to_string(),
            expires_at: now + OIDC_AUTH_SESSION_TTL_SECONDS,
        },
    );
    Ok(())
}

pub(crate) fn consume_oidc_auth_session(state: &str) -> Result<OidcAuthSession, ApiError> {
    let now = current_unix_ts();
    let mut sessions = oidc_auth_sessions()
        .lock()
        .map_err(|e| ApiError::internal_with_log("Failed to acquire OIDC auth session lock", &e))?;
    cleanup_expired_oidc_sessions(&mut sessions, now);
    let session = sessions
        .remove(state)
        .ok_or_else(|| ApiError::unauthorized("OIDC state is missing, expired, or already used".to_string()))?;
    if session.expires_at < now {
        return Err(ApiError::unauthorized("OIDC authorization session expired".to_string()));
    }
    Ok(session)
}

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
        .route("/_matrix/client/r0/login/sso/redirect", get(sso::sso_redirect))
        .route("/_matrix/client/v3/login/sso/userinfo", get(provider::oidc_userinfo))
        .route("/_matrix/client/r0/login/sso/userinfo", get(provider::oidc_userinfo))
        // v3 paths
        .route("/_matrix/client/v3/oidc/userinfo", get(provider::oidc_userinfo))
        .route("/_matrix/client/v3/oidc/token", post(provider::oidc_token))
        .route("/_matrix/client/v3/oidc/logout", post(provider::oidc_logout))
        .route("/_matrix/client/v3/oidc/authorize", get(provider::oidc_authorize))
        .route("/_matrix/client/v3/oidc/callback", get(sso::oidc_callback))
        // r0 compatibility paths
        .route("/_matrix/client/r0/oidc/userinfo", get(provider::oidc_userinfo))
        .route("/_matrix/client/r0/oidc/token", post(provider::oidc_token))
        .route("/_matrix/client/r0/oidc/logout", post(provider::oidc_logout))
        .route("/_matrix/client/r0/oidc/authorize", get(provider::oidc_authorize))
        .route("/_matrix/client/r0/oidc/callback", get(sso::oidc_callback));

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

pub fn oidc_enabled(ctx: &SsoContext) -> bool {
    #[cfg(feature = "saml-sso")]
    let saml_enabled = ctx.saml_service.is_enabled();
    #[cfg(not(feature = "saml-sso"))]
    let saml_enabled = false;

    ctx.oidc_service.is_some() || ctx.builtin_oidc_provider.is_some() || saml_enabled
}

pub fn create_oidc_fallback_router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/openid-configuration", get(builtin::get_openid_configuration))
        .route("/.well-known/jwks.json", get(builtin::jwks_fallback))
}

// ---------------------------------------------------------------------------
// Route ledger manifests
// ---------------------------------------------------------------------------

/// Manifest for `create_oidc_router`. Note that this router is only merged
/// into the assembly when OIDC / built-in OIDC / SAML is enabled — when none
/// is, `assembly` falls back to a smaller pair of `/.well-known/*` routes
/// declared inline. The ledger entries below match the *enabled* path; if
/// you exercise the manifest in the always-fallback path, expect the
/// `/.well-known/*` entries to overlap with the inline assembly fallback.
pub fn oidc_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_matrix/client/v3/login/sso/redirect"),
        (Method::GET, "/_matrix/client/r0/login/sso/redirect"),
        (Method::GET, "/_matrix/client/v3/login/sso/userinfo"),
        (Method::GET, "/_matrix/client/r0/login/sso/userinfo"),
        (Method::GET, "/_matrix/client/v3/oidc/userinfo"),
        (Method::POST, "/_matrix/client/v3/oidc/token"),
        (Method::POST, "/_matrix/client/v3/oidc/logout"),
        (Method::GET, "/_matrix/client/v3/oidc/authorize"),
        (Method::GET, "/_matrix/client/v3/oidc/callback"),
        (Method::GET, "/_matrix/client/r0/oidc/userinfo"),
        (Method::POST, "/_matrix/client/r0/oidc/token"),
        (Method::POST, "/_matrix/client/r0/oidc/logout"),
        (Method::GET, "/_matrix/client/r0/oidc/authorize"),
        (Method::GET, "/_matrix/client/r0/oidc/callback"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "oidc"))
    .chain({
        #[cfg(feature = "builtin-oidc")]
        {
            vec![
                RouteEntry::new(Method::POST, "/_matrix/client/v3/oidc/login", "oidc"),
                RouteEntry::new(Method::GET, "/.well-known/openid-configuration", "oidc"),
                RouteEntry::new(Method::GET, "/.well-known/jwks.json", "oidc"),
            ]
        }
        #[cfg(not(feature = "builtin-oidc"))]
        {
            vec![]
        }
    })
    .collect()
}

pub fn oidc_fallback_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [(Method::GET, "/.well-known/openid-configuration"), (Method::GET, "/.well-known/jwks.json")]
        .into_iter()
        .map(|(m, p)| RouteEntry::new(m, p, "oidc_fallback"))
        .collect()
}

pub fn oidc_route_manifest_for(ctx: &SsoContext) -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    if oidc_enabled(ctx) {
        oidc_route_manifest()
    } else {
        oidc_fallback_manifest()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oidc_auth_session_roundtrip() {
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        store_oidc_auth_session(
            &state,
            "nonce",
            &code_verifier,
            &code_challenge,
            "S256",
            "https://example.com/callback",
        )
        .unwrap();

        let session = consume_oidc_auth_session(&state).unwrap();
        assert_eq!(session.nonce, "nonce");
        assert_eq!(session.code_verifier, code_verifier);
        assert_eq!(session.code_challenge, code_challenge);
        assert_eq!(session.redirect_uri, "https://example.com/callback");
    }

    #[test]
    fn test_oidc_auth_session_is_one_time_use() {
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        store_oidc_auth_session(
            &state,
            "nonce",
            &code_verifier,
            &code_challenge,
            "S256",
            "https://example.com/callback",
        )
        .unwrap();

        let _ = consume_oidc_auth_session(&state).unwrap();
        let error = consume_oidc_auth_session(&state).unwrap_err();
        assert!(error.to_string().contains("state is missing"));
    }

    #[test]
    fn test_validate_state_pkce_binding_accepts_valid_binding() {
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        let session = OidcAuthSession {
            nonce: "nonce".to_string(),
            code_verifier,
            code_challenge,
            code_challenge_method: "S256".to_string(),
            redirect_uri: "https://example.com/callback".to_string(),
            expires_at: current_unix_ts() + 60,
        };

        assert!(validate_state_pkce_binding(&session).is_ok());
    }

    #[test]
    fn test_validate_state_pkce_binding_rejects_mismatched_challenge() {
        let (code_verifier, _) = OidcService::generate_pkce();
        let session = OidcAuthSession {
            nonce: "nonce".to_string(),
            code_verifier,
            code_challenge: "invalid_challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            redirect_uri: "https://example.com/callback".to_string(),
            expires_at: current_unix_ts() + 60,
        };

        let error = validate_state_pkce_binding(&session).unwrap_err();
        assert!(error.to_string().contains("binding validation failed"));
    }
}
