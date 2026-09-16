//! Email-verification token policy (threepid `m.email.identity` flows).
//!
//! Owns token creation/consumption, the single-use claim used by the
//! account-compat flow, and the admin cleanup sweep, so the HTTP layer never
//! touches `EmailVerificationStorage` directly (B4-5c).

use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_storage::email_verification::EmailVerificationStorage;

pub use synapse_storage::email_verification::EmailVerificationToken;

/// Service over the `email_verification_tokens` table.
pub struct EmailVerificationService {
    storage: Arc<EmailVerificationStorage>,
}

impl EmailVerificationService {
    /// See [`new`].
    pub fn new(storage: Arc<EmailVerificationStorage>) -> Self {
        Self { storage }
    }

    /// Create a verification token, returning its row id (the client-facing
    /// `sid`).
    pub async fn create_verification_token(
        &self,
        email: &str,
        token: &str,
        expires_in_seconds: i64,
        user_id: Option<&str>,
        session_data: Option<serde_json::Value>,
    ) -> Result<i64, ApiError> {
        self.storage
            .create_verification_token(email, token, expires_in_seconds, user_id, session_data)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create email verification token", e))
    }

    /// Validate and consume a submitted token (single-use).
    pub async fn validate_and_consume_token(
        &self,
        token_id: i64,
        submitted_token: &str,
        client_secret: &str,
    ) -> Result<EmailVerificationToken, ApiError> {
        self.storage.validate_and_consume_token(token_id, submitted_token, client_secret).await
    }

    /// Atomically claim an already-used token row, for the "the token was
    /// already consumed, return the previous result" flow.
    pub async fn claim_used_token(&self, token_id: i64) -> Result<Option<EmailVerificationToken>, ApiError> {
        self.storage
            .claim_used_token(token_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to claim used email verification token", e))
    }

    /// Fetch a token row by id (used to assert on token state in flows and
    /// tests; returns `None` once the row has been consumed).
    pub async fn get_verification_token_by_id(
        &self,
        token_id: i64,
    ) -> Result<Option<EmailVerificationToken>, ApiError> {
        self.storage
            .get_verification_token_by_id(token_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get verification token", e))
    }

    /// Delete expired tokens, returning how many rows were removed.
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, ApiError> {
        self.storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal_with_cause("Email token cleanup failed", e))
    }
}
