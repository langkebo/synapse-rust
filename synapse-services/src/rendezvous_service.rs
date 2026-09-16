//! Rendezvous sessions — QR sign-in session lifecycle, access control and
//! opaque message relay.
//!
//! Two transports share one store:
//!
//! * the legacy `/_matrix/client/v1/rendezvous` JSON surface (session key or
//!   bound user authorises access, opaque `type`/`content` messages), and
//! * the MSC4108 `text/plain` ETag surface (conditional `If-Match` updates).
//!
//! The service owns access control and the storage→`ApiError` mapping; the HTTP
//! layer keeps request parsing and response/header rendering (Content-Type
//! checks, `Expires` / `Last-Modified` / `ETag` formatting, 202/304/412).

use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::rendezvous::{RendezvousStoreApi, StoredRendezvousMessage};

pub use synapse_storage::rendezvous::{
    CreateRendezvousSessionParams, Msc4108UpdateOutcome, RendezvousIntent, RendezvousMessage, RendezvousSession,
    RendezvousTransport,
};

/// Header carrying the shared secret that authorises access to a session that
/// has not been bound to a user yet.
pub const RENDEZVOUS_KEY_HEADER: &str = "x-matrix-rendezvous-key";

/// MSC4108 rendezvous sessions live for 5 minutes.
const MSC4108_TTL_MS: i64 = 5 * 60 * 1000;

/// Session lifecycle, authorisation and message relay for rendezvous.
pub struct RendezvousService {
    storage: Arc<dyn RendezvousStoreApi>,
}

impl RendezvousService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn RendezvousStoreApi>) -> Self {
        Self { storage }
    }

    // ── Legacy JSON surface ───────────────────────────────────────────────

    /// Create a session and return it (including the authorisation key).
    pub async fn create_session(&self, params: CreateRendezvousSessionParams) -> Result<RendezvousSession, ApiError> {
        self.storage
            .create_session(params)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create session", e))
    }

    /// Load a live session, mapping "missing or expired" to `M_NOT_FOUND`.
    pub async fn load_session(&self, session_id: &str) -> Result<RendezvousSession, ApiError> {
        self.storage
            .get_session(session_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get session", e))?
            .ok_or_else(|| ApiError::not_found("Session not found or expired".to_string()))
    }

    /// Authorise `action` on `session_id` and return the session.
    ///
    /// Two credentials are accepted: the session key (from
    /// [`RENDEZVOUS_KEY_HEADER`]) for a session that has not been bound yet, or
    /// the bound user. A presented-but-wrong key is an error rather than a
    /// fall-through, so a bad key can never be rescued by a valid login.
    pub async fn authorize(
        &self,
        session_id: &str,
        presented_key: Option<&str>,
        bound_caller: Option<&str>,
        request_id: &str,
        action: &str,
    ) -> Result<RendezvousSession, ApiError> {
        let session = self.load_session(session_id).await?;

        if let Some(session_key) = presented_key {
            if session.key.as_deref() == Some(session_key) {
                return Ok(session);
            }

            tracing::warn!(request_id = %request_id, session_id = %session_id, action, "Invalid rendezvous key");
            return Err(ApiError::unauthorized(format!("Invalid rendezvous key for {action}")));
        }

        if let (Some(auth_user_id), Some(bound_user_id)) = (bound_caller, session.user_id.as_deref()) {
            if auth_user_id == bound_user_id {
                return Ok(session);
            }

            tracing::warn!(
                request_id = %request_id,
                session_id = %session_id,
                action,
                auth_user_id = %auth_user_id,
                bound_user_id = %bound_user_id,
                "Forbidden rendezvous session access"
            );
            return Err(ApiError::forbidden(format!("You are not allowed to {action} this rendezvous session")));
        }

        tracing::warn!(request_id = %request_id, session_id = %session_id, action, "Missing rendezvous access credentials");
        Err(ApiError::unauthorized(format!(
            "Rendezvous access to {action} requires the {RENDEZVOUS_KEY_HEADER} header or the bound user"
        )))
    }

    /// Set the session status.
    pub async fn update_status(&self, session_id: &str, status: &str) -> Result<(), ApiError> {
        self.storage
            .update_session_status(session_id, status)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update session", e))
    }

    /// Bind the authenticated user (and device) to the session.
    pub async fn bind_user(&self, session_id: &str, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        self.storage
            .bind_user_to_session(session_id, user_id, device_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to bind user", e))
    }

    /// Delete a session (idempotent).
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.storage
            .delete_session(session_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete session", e))
    }

    /// Store an opaque relay message on the session.
    pub async fn store_message(
        &self,
        session_id: &str,
        direction: &str,
        message: &RendezvousMessage,
    ) -> Result<(), ApiError> {
        self.storage
            .store_message(session_id, direction, message)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to send message", e))
    }

    /// Read the relay messages stored on the session.
    pub async fn messages(&self, session_id: &str) -> Result<Vec<StoredRendezvousMessage>, ApiError> {
        self.storage
            .get_messages(session_id, None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get messages", e))
    }

    // ── MSC4108 text/plain surface ────────────────────────────────────────

    /// Create an MSC4108 session holding `initial_data`.
    ///
    /// Returns `(session_id, etag, created_ts, expires_at)`; the timestamps feed
    /// the `Last-Modified` / `Expires` response headers.
    pub async fn create_msc4108(&self, initial_data: &str) -> Result<(String, String, i64, i64), ApiError> {
        self.storage
            .create_msc4108_session(initial_data, MSC4108_TTL_MS)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create MSC4108 session", e))
    }

    /// Read an MSC4108 payload, or `None` when the session is missing/expired.
    pub async fn msc4108_data(&self, session_id: &str) -> Result<Option<(String, String, i64, i64)>, ApiError> {
        self.storage
            .get_msc4108_data(session_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get MSC4108 data", e))
    }

    /// Conditionally overwrite an MSC4108 payload.
    pub async fn update_msc4108(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Msc4108UpdateOutcome, ApiError> {
        self.storage
            .update_msc4108_data(session_id, data, if_match)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update MSC4108 data", e))
    }

    /// Delete an MSC4108 session, reporting whether a live row existed.
    pub async fn delete_msc4108(&self, session_id: &str) -> Result<bool, ApiError> {
        self.storage
            .delete_msc4108_session(session_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete MSC4108 session", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::ApiErrorKind;
    use synapse_storage::test_mocks::rendezvous::InMemoryRendezvousStore;

    fn service() -> RendezvousService {
        RendezvousService::new(Arc::new(InMemoryRendezvousStore::new()))
    }

    fn params() -> CreateRendezvousSessionParams {
        CreateRendezvousSessionParams {
            intent: RendezvousIntent::LoginStart,
            transport: RendezvousTransport::HttpV1,
            transport_data: None,
            expires_in_ms: None,
        }
    }

    #[tokio::test]
    async fn session_key_authorises_a_read() {
        let service = service();
        let session = service.create_session(params()).await.expect("create");
        let key = session.key.clone().expect("session carries a key");

        let authorised = service
            .authorize(&session.session_id, Some(&key), None, "req-1", "read")
            .await
            .expect("key must authorise");
        assert_eq!(authorised.session_id, session.session_id);
    }

    #[tokio::test]
    async fn wrong_key_is_rejected_even_with_a_valid_login() {
        let service = service();
        let session = service.create_session(params()).await.expect("create");

        let err = service
            .authorize(&session.session_id, Some("not-the-key"), Some("@alice:example.com"), "req-2", "read")
            .await
            .expect_err("wrong key must not fall through to the login check");
        assert_eq!(err.kind, ApiErrorKind::Unauthorized);
    }

    #[tokio::test]
    async fn missing_credentials_are_rejected() {
        let service = service();
        let session = service.create_session(params()).await.expect("create");

        let err = service.authorize(&session.session_id, None, None, "req-3", "update").await.unwrap_err();
        assert_eq!(err.kind, ApiErrorKind::Unauthorized);
        assert!(err.message.contains(RENDEZVOUS_KEY_HEADER), "got: {}", err.message);
    }

    #[tokio::test]
    async fn another_user_is_forbidden_not_unauthorized() {
        let service = service();
        let session = service.create_session(params()).await.expect("create");
        service.bind_user(&session.session_id, "@alice:example.com", "DEVICE").await.expect("bind");

        let err = service
            .authorize(&session.session_id, None, Some("@mallory:example.com"), "req-4", "delete")
            .await
            .unwrap_err();
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
    }

    #[tokio::test]
    async fn bound_user_is_authorised_without_the_key() {
        let service = service();
        let session = service.create_session(params()).await.expect("create");
        service.bind_user(&session.session_id, "@alice:example.com", "DEVICE").await.expect("bind");

        assert!(service
            .authorize(&session.session_id, None, Some("@alice:example.com"), "req-5", "read")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn unknown_session_is_not_found() {
        let err = service().authorize("no-such-session", None, None, "req-6", "read").await.unwrap_err();
        assert_eq!(err.kind, ApiErrorKind::NotFound);
    }

    #[tokio::test]
    async fn msc4108_lifecycle_is_not_found_after_delete() {
        let service = service();
        let (session_id, etag, _created, _expires) = service.create_msc4108("payload").await.expect("create");
        assert_eq!(service.msc4108_data(&session_id).await.unwrap().map(|(d, ..)| d).as_deref(), Some("payload"));

        // A stale If-Match is a precondition failure, not a 404.
        let stale = service.update_msc4108(&session_id, "next", Some("\"1\"")).await.unwrap();
        assert!(matches!(stale, Msc4108UpdateOutcome::PreconditionFailed { .. }));

        // The correct ETag succeeds.
        let updated = service.update_msc4108(&session_id, "next", Some(&etag)).await.unwrap();
        assert!(matches!(updated, Msc4108UpdateOutcome::Updated { .. }));

        assert!(service.delete_msc4108(&session_id).await.unwrap());
        assert!(!service.delete_msc4108(&session_id).await.unwrap(), "second delete reports no row");
        assert!(service.msc4108_data(&session_id).await.unwrap().is_none());
    }
}
