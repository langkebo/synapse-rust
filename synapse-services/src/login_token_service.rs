//! Short-lived login token store for MSC4108 QR sign-in.
//!
//! The existing device (already authenticated) generates a login token; the
//! token is sent to the new device over the MSC4108 secure channel, which
//! exchanges it for a real access token via `POST /_matrix/client/v3/login`
//! with `type: "m.login.token"`. Tokens are single-use and expire after 60
//! seconds, persisted to the `login_tokens` table so QR sign-in works across
//! workers and restarts.

use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_storage::login_token::LoginTokenStoreApi;

/// MSC4108 QR sign-in login tokens live for 60 seconds.
const LOGIN_TOKEN_TTL_MS: i64 = 60_000;

/// Generates and consumes the single-use login tokens used by QR sign-in.
pub struct LoginTokenService {
    storage: Arc<dyn LoginTokenStoreApi>,
}

impl LoginTokenService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn LoginTokenStoreApi>) -> Self {
        Self { storage }
    }

    /// Lifetime of a freshly generated token, in milliseconds.
    ///
    /// Exposed so the HTTP layer can report `expires_in_ms` without keeping a
    /// second copy of the constant.
    pub fn ttl_ms(&self) -> i64 {
        LOGIN_TOKEN_TTL_MS
    }

    /// Generate a new login token for the given user, returning the token
    /// string (a random UUID).
    pub async fn generate(&self, user_id: &str, device_id: Option<&str>) -> Result<String, ApiError> {
        let token = uuid::Uuid::new_v4().to_string();
        let expires_at = current_timestamp_millis() + LOGIN_TOKEN_TTL_MS;
        self.storage
            .create_login_token(&token, user_id, device_id, expires_at)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to store QR login token", e))?;
        Ok(token)
    }

    /// Validate and consume a login token (single-use).
    ///
    /// Returns `Some((user_id, device_id))` if valid, `None` if the token is
    /// unknown, expired or already consumed.
    pub async fn consume(&self, token: &str) -> Result<Option<(String, Option<String>)>, ApiError> {
        let entry = self
            .storage
            .consume_login_token(token)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to consume QR login token", e))?;
        Ok(entry.map(|e| (e.user_id, e.device_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::login_token::InMemoryLoginTokenStore;

    fn service() -> LoginTokenService {
        LoginTokenService::new(Arc::new(InMemoryLoginTokenStore::new()))
    }

    #[tokio::test]
    async fn generate_returns_a_uuid_v4_string() {
        let token = service().generate("@alice:localhost", Some("DEV-001")).await.unwrap();
        assert_eq!(token.len(), 36, "UUID v4 string must be 36 chars, got {token}");
        let segments: Vec<&str> = token.split('-').collect();
        assert_eq!(segments.len(), 5, "UUID must have 5 hyphen-separated segments");
        assert!(segments[2].starts_with('4'), "UUID v4 variant must start with '4' in 3rd group");
    }

    #[tokio::test]
    async fn consecutive_tokens_are_unique() {
        let service = service();
        let t1 = service.generate("@alice:localhost", None).await.unwrap();
        let t2 = service.generate("@alice:localhost", None).await.unwrap();
        let t3 = service.generate("@alice:localhost", None).await.unwrap();
        assert_ne!(t1, t2);
        assert_ne!(t2, t3);
        assert_ne!(t1, t3);
    }

    #[tokio::test]
    async fn consume_round_trips_user_and_device() {
        let service = service();
        let token = service.generate("@bob:localhost", Some("DEVICE-X")).await.unwrap();
        let (user_id, device_id) = service.consume(&token).await.unwrap().expect("fresh token must consume");
        assert_eq!(user_id, "@bob:localhost");
        assert_eq!(device_id.as_deref(), Some("DEVICE-X"));
    }

    #[tokio::test]
    async fn consume_preserves_none_device() {
        let service = service();
        let token = service.generate("@carol:localhost", None).await.unwrap();
        let (user_id, device_id) = service.consume(&token).await.unwrap().expect("fresh token must consume");
        assert_eq!(user_id, "@carol:localhost");
        assert!(device_id.is_none(), "device_id must stay None");
    }

    #[tokio::test]
    async fn tokens_are_single_use() {
        let service = service();
        let token = service.generate("@dave:localhost", None).await.unwrap();
        assert!(service.consume(&token).await.unwrap().is_some(), "first consume must succeed");
        assert!(service.consume(&token).await.unwrap().is_none(), "second consume must fail (single-use)");
    }

    #[tokio::test]
    async fn unknown_and_empty_tokens_are_rejected() {
        let service = service();
        assert!(service.consume("never-generated-uuid-0000-0000-000000000000").await.unwrap().is_none());
        assert!(service.consume("").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn consuming_one_token_does_not_affect_another() {
        let service = service();
        let t1 = service.generate("@eve:localhost", Some("D1")).await.unwrap();
        let t2 = service.generate("@frank:localhost", Some("D2")).await.unwrap();
        assert!(service.consume(&t1).await.unwrap().is_some());
        let (user2, dev2) = service.consume(&t2).await.unwrap().expect("t2 must be unaffected by t1");
        assert_eq!(user2, "@frank:localhost");
        assert_eq!(dev2.as_deref(), Some("D2"));
    }

    #[tokio::test]
    async fn user_and_device_ids_round_trip_exactly() {
        let service = service();
        let weird_user = "@weird_user+test:sub.domain.example.org";
        let device = "DEVICE-with-special_chars.123";
        let token = service.generate(weird_user, Some(device)).await.unwrap();
        let (user_id, dev) = service.consume(&token).await.unwrap().expect("must consume");
        assert_eq!(user_id, weird_user);
        assert_eq!(dev.as_deref(), Some(device));
    }

    #[test]
    fn ttl_is_sixty_seconds() {
        let service = LoginTokenService::new(Arc::new(InMemoryLoginTokenStore::new()));
        assert_eq!(service.ttl_ms(), 60_000, "MSC4108 login token TTL must be 60 seconds");
    }
}
