use std::collections::HashMap;
use std::sync::Arc;

use crate::oidc_session_storage::{OidcAuthSession, OidcConsentSession, OidcRefreshToken, OidcSessionStoreApi};
use tokio::sync::RwLock;

/// In-memory [`OidcSessionStoreApi`] covering the PKCE auth-session path.
///
/// Refresh-token and consent-session methods are inert stubs: the service
/// under test only exercises `save_auth_session` /
/// `get_and_delete_auth_session`, and a stub that records nothing keeps the
/// double honest about what it actually models.
pub struct InMemoryOidcSessionStore {
    sessions: Arc<RwLock<HashMap<String, OidcAuthSession>>>,
}

impl Default for InMemoryOidcSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryOidcSessionStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self { sessions: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Number of pending (not yet consumed) auth sessions.
    pub async fn pending_sessions(&self) -> usize {
        self.sessions.read().await.len()
    }
}

#[async_trait::async_trait]
impl OidcSessionStoreApi for InMemoryOidcSessionStore {
    async fn save_auth_session(&self, session: &OidcAuthSession) -> Result<(), sqlx::Error> {
        self.sessions.write().await.insert(session.session_key.clone(), session.clone());
        Ok(())
    }

    async fn get_and_delete_auth_session(&self, session_key: &str) -> Result<Option<OidcAuthSession>, sqlx::Error> {
        Ok(self.sessions.write().await.remove(session_key))
    }

    async fn save_refresh_token(&self, _token: &OidcRefreshToken) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_refresh_token(&self, _token_hash: &str) -> Result<Option<OidcRefreshToken>, sqlx::Error> {
        Ok(None)
    }

    async fn revoke_refresh_token(&self, _token_hash: &str, _now_ts: i64) -> Result<bool, sqlx::Error> {
        Ok(false)
    }

    async fn revoke_user_refresh_tokens(&self, _user_id: &str, _now_ts: i64) -> Result<u64, sqlx::Error> {
        Ok(0)
    }

    async fn save_consent_session(&self, _session: &OidcConsentSession) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_and_delete_consent_session(
        &self,
        _session_id: &str,
    ) -> Result<Option<OidcConsentSession>, sqlx::Error> {
        Ok(None)
    }

    async fn get_consent_session(&self, _session_id: &str) -> Result<Option<OidcConsentSession>, sqlx::Error> {
        Ok(None)
    }

    async fn delete_consent_session(&self, _session_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn cleanup_expired_sessions(&self, _now_ts: i64) -> Result<u64, sqlx::Error> {
        Ok(0)
    }
}
