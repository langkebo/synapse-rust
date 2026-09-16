//! OIDC authorization-session persistence — binds the `state` value returned
//! by the IdP to the PKCE material created when the flow started.
//!
//! The session is single-use: `consume` deletes it. Both the SSO redirect
//! handler and the built-in provider handler share this service so the TTL and
//! the "missing / expired / already used" failure modes have one definition.

use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use synapse_storage::oidc_session_storage::{OidcAuthSession as DbOidcAuthSession, OidcSessionStoreApi};

/// How long a pending OIDC authorization session stays valid.
const OIDC_AUTH_SESSION_TTL_SECONDS: u64 = 600;

/// The PKCE material a pending authorization request is bound to.
#[derive(Debug, Clone)]
pub struct OidcAuthSession {
    /// The `nonce` field.
    pub nonce: String,
    /// The `code_verifier` field.
    pub code_verifier: String,
    /// The `code_challenge` field.
    pub code_challenge: String,
    /// The `code_challenge_method` field.
    pub code_challenge_method: String,
    /// The `redirect_uri` field.
    pub redirect_uri: String,
}

/// Stores and consumes pending OIDC authorization sessions.
pub struct OidcSessionService {
    storage: Arc<dyn OidcSessionStoreApi>,
}

impl OidcSessionService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn OidcSessionStoreApi>) -> Self {
        Self { storage }
    }

    /// Persist the PKCE material for a freshly started authorization request,
    /// keyed by the `state` value sent to the IdP.
    pub async fn store(
        &self,
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
        self.storage
            .save_auth_session(&session)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to store OIDC auth session", e))?;
        Ok(())
    }

    /// Consume (single-use) the authorization session for `state`.
    ///
    /// A missing, expired or already-consumed session is indistinguishable to
    /// the caller — all map to `M_UNAUTHORIZED`.
    pub async fn consume(&self, state: &str) -> Result<OidcAuthSession, ApiError> {
        let now_ms = current_timestamp_millis();
        let db_session = self
            .storage
            .get_and_delete_auth_session(state)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to consume OIDC auth session", e))?
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oidc_service::OidcService;
    use synapse_storage::test_mocks::oidc_session::InMemoryOidcSessionStore;

    fn service() -> OidcSessionService {
        OidcSessionService::new(Arc::new(InMemoryOidcSessionStore::new()))
    }

    #[tokio::test]
    async fn store_then_consume_round_trips_pkce_material() {
        let service = service();
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        service
            .store(&state, "nonce", &code_verifier, &code_challenge, "S256", "https://example.com/callback")
            .await
            .unwrap();

        let session = service.consume(&state).await.unwrap();
        assert_eq!(session.nonce, "nonce");
        assert_eq!(session.code_verifier, code_verifier);
        assert_eq!(session.code_challenge, code_challenge);
        assert_eq!(session.code_challenge_method, "S256");
        assert_eq!(session.redirect_uri, "https://example.com/callback");
    }

    #[tokio::test]
    async fn sessions_are_single_use() {
        let service = service();
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        service
            .store(&state, "nonce", &code_verifier, &code_challenge, "S256", "https://example.com/callback")
            .await
            .unwrap();

        assert!(service.consume(&state).await.is_ok());
        let error = service.consume(&state).await.unwrap_err();
        assert!(error.to_string().contains("state is missing"), "got: {error}");
    }

    #[tokio::test]
    async fn unknown_state_is_rejected() {
        let error = service().consume("never-issued-state").await.unwrap_err();
        assert!(error.to_string().contains("state is missing"), "got: {error}");
    }
}
