// OIDC (OpenID Connect) routes
// Matrix Spec: https://matrix.org/docs/spec/openid.html

/// The `builtin` module.
pub(crate) mod builtin;
/// The `provider` module.
pub(crate) mod provider;
/// The `sso` module.
pub(crate) mod sso;

use crate::common::error::ApiError;
use crate::web::routes::context::SsoContext;
use crate::web::routes::AppState;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use synapse_common::current_timestamp_millis;
use synapse_services::oidc_service::OidcService;
use synapse_storage::oidc_session_storage::{OidcAuthSession as DbOidcAuthSession, OidcSessionStoreApi};

// ---------------------------------------------------------------------------
// Session management — shared by sso and provider submodules
// ---------------------------------------------------------------------------

const OIDC_AUTH_SESSION_TTL_SECONDS: u64 = 600;

/// The `OidcAuthSession` struct.
#[derive(Debug, Clone)]
pub(crate) struct OidcAuthSession {
    pub(crate) nonce: String,
    pub(crate) code_verifier: String,
    pub(crate) code_challenge: String,
    pub(crate) code_challenge_method: String,
    pub(crate) redirect_uri: String,
}

/// See [`current_unix_ts`].
pub(crate) fn current_unix_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// See [`store_oidc_auth_session`].
pub(crate) async fn store_oidc_auth_session(
    storage: &Arc<dyn OidcSessionStoreApi>,
    state: &str,
    nonce: &str,
    code_verifier: &str,
    code_challenge: &str,
    code_challenge_method: &str,
    redirect_uri: &str,
) -> Result<(), ApiError> {
    let now_ms = current_timestamp_millis();
    let session = DbOidcAuthSession {
        id: 0,
        session_key: state.to_string(),
        session_type: "pkce".to_string(),
        client_id: String::new(),
        redirect_uri: redirect_uri.to_string(),
        scope: String::new(),
        state: state.to_string(),
        nonce: Some(nonce.to_string()),
        code_verifier: Some(code_verifier.to_string()),
        code_challenge: Some(code_challenge.to_string()),
        code_challenge_method: Some(code_challenge_method.to_string()),
        user_id: None,
        consent_given: false,
        created_ts: now_ms,
        expires_at: now_ms + (OIDC_AUTH_SESSION_TTL_SECONDS as i64) * 1000,
    };
    storage
        .save_auth_session(&session)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to store OIDC auth session", &e))?;
    Ok(())
}

/// See [`consume_oidc_auth_session`].
pub(crate) async fn consume_oidc_auth_session(
    storage: &Arc<dyn OidcSessionStoreApi>,
    state: &str,
) -> Result<OidcAuthSession, ApiError> {
    let now_ms = current_timestamp_millis();
    let db_session = storage
        .get_and_delete_auth_session(state)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to consume OIDC auth session", &e))?
        .ok_or_else(|| ApiError::unauthorized("OIDC state is missing, expired, or already used".to_string()))?;
    if db_session.expires_at < now_ms {
        return Err(ApiError::unauthorized("OIDC authorization session expired".to_string()));
    }
    Ok(OidcAuthSession {
        nonce: db_session.nonce.unwrap_or_default(),
        code_verifier: db_session.code_verifier.unwrap_or_default(),
        code_challenge: db_session.code_challenge.unwrap_or_default(),
        code_challenge_method: db_session.code_challenge_method.unwrap_or_default(),
        redirect_uri: db_session.redirect_uri,
    })
}

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

/// See [`oidc_fallback_manifest`].
pub fn oidc_fallback_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [(Method::GET, "/.well-known/openid-configuration"), (Method::GET, "/.well-known/jwks.json")]
        .into_iter()
        .map(|(m, p)| RouteEntry::new(m, p, "oidc_fallback"))
        .collect()
}

/// See [`oidc_route_manifest_for`].
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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use synapse_storage::oidc_session_storage::{
        OidcAuthSession as DbOidcAuthSession, OidcConsentSession, OidcRefreshToken, OidcSessionStoreApi,
    };

    /// 内存实现 OidcSessionStoreApi，用于验证 store/consume 经 storage 往返。
    struct InMemoryOidcSessionStore {
        sessions: Mutex<HashMap<String, DbOidcAuthSession>>,
    }

    #[async_trait::async_trait]
    impl OidcSessionStoreApi for InMemoryOidcSessionStore {
        async fn save_auth_session(&self, session: &DbOidcAuthSession) -> Result<(), sqlx::Error> {
            self.sessions.lock().unwrap().insert(session.session_key.clone(), session.clone());
            Ok(())
        }
        async fn get_and_delete_auth_session(
            &self,
            session_key: &str,
        ) -> Result<Option<DbOidcAuthSession>, sqlx::Error> {
            Ok(self.sessions.lock().unwrap().remove(session_key))
        }
        async fn save_refresh_token(&self, _t: &OidcRefreshToken) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn get_refresh_token(&self, _h: &str) -> Result<Option<OidcRefreshToken>, sqlx::Error> {
            Ok(None)
        }
        async fn revoke_refresh_token(&self, _h: &str, _now: i64) -> Result<bool, sqlx::Error> {
            Ok(false)
        }
        async fn revoke_user_refresh_tokens(&self, _u: &str, _now: i64) -> Result<u64, sqlx::Error> {
            Ok(0)
        }
        async fn save_consent_session(&self, _s: &OidcConsentSession) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn get_and_delete_consent_session(&self, _id: &str) -> Result<Option<OidcConsentSession>, sqlx::Error> {
            Ok(None)
        }
        async fn get_consent_session(&self, _id: &str) -> Result<Option<OidcConsentSession>, sqlx::Error> {
            Ok(None)
        }
        async fn delete_consent_session(&self, _id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn cleanup_expired_sessions(&self, _now: i64) -> Result<u64, sqlx::Error> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_oidc_auth_session_roundtrip_via_storage() {
        let store: Arc<dyn OidcSessionStoreApi> =
            Arc::new(InMemoryOidcSessionStore { sessions: Mutex::new(HashMap::new()) });
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        store_oidc_auth_session(
            &store,
            &state,
            "nonce",
            &code_verifier,
            &code_challenge,
            "S256",
            "https://example.com/callback",
        )
        .await
        .unwrap();

        let session = consume_oidc_auth_session(&store, &state).await.unwrap();
        assert_eq!(session.nonce, "nonce");
        assert_eq!(session.code_verifier, code_verifier);
        assert_eq!(session.code_challenge, code_challenge);
        assert_eq!(session.redirect_uri, "https://example.com/callback");
    }

    #[tokio::test]
    async fn test_oidc_auth_session_is_one_time_use_via_storage() {
        let store: Arc<dyn OidcSessionStoreApi> =
            Arc::new(InMemoryOidcSessionStore { sessions: Mutex::new(HashMap::new()) });
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        store_oidc_auth_session(
            &store,
            &state,
            "nonce",
            &code_verifier,
            &code_challenge,
            "S256",
            "https://example.com/callback",
        )
        .await
        .unwrap();

        let _ = consume_oidc_auth_session(&store, &state).await.unwrap();
        let error = consume_oidc_auth_session(&store, &state).await.unwrap_err();
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
        };

        let error = validate_state_pkce_binding(&session).unwrap_err();
        assert!(error.to_string().contains("binding validation failed"));
    }
}
