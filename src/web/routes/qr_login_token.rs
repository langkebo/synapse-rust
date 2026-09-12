//! Short-lived login token store for MSC4108 QR sign-in.
//!
//! The existing device (already authenticated) generates a login token via
//! `POST /_matrix/client/v1/login/qr_token`. The token is sent to the new
//! device over the MSC4108 secure channel. The new device exchanges it for a
//! real access token via `POST /_matrix/client/v3/login` with
//! `type: "m.login.token"`.
//!
//! Tokens are single-use and expire after 60 seconds. Persisted to the
//! `login_tokens` table so QR sign-in works across workers and restarts.

use std::sync::Arc;

use synapse_common::current_timestamp_millis;
use synapse_storage::login_token::LoginTokenStoreApi;

use crate::common::ApiError;

const LOGIN_TOKEN_TTL_MS: i64 = 60_000;

/// Generate a new login token for the given user.
/// Returns the token string (a random UUID).
pub async fn generate_login_token(
    storage: &Arc<dyn LoginTokenStoreApi>,
    user_id: &str,
    device_id: Option<&str>,
) -> Result<String, ApiError> {
    let token = uuid::Uuid::new_v4().to_string();
    let expires_at = current_timestamp_millis() + LOGIN_TOKEN_TTL_MS;
    storage
        .create_login_token(&token, user_id, device_id, expires_at)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to store QR login token", e))?;
    Ok(token)
}

/// Validate and consume a login token (single-use).
/// Returns `Some((user_id, device_id))` if valid, `None` if invalid/expired.
pub async fn consume_login_token(
    storage: &Arc<dyn LoginTokenStoreApi>,
    token: &str,
) -> Result<Option<(String, Option<String>)>, ApiError> {
    let entry = storage
        .consume_login_token(token)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to consume QR login token", e))?;
    Ok(entry.map(|e| (e.user_id, e.device_id)))
}
