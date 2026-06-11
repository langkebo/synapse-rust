//! Email verification service.
//!
//! Encapsulates the three email-verification flows that previously
//! lived in the route layer:
//!   1. `create_verification_token`     — POST /register/email/requestToken
//!   2. `get_verification_token_by_id`  — POST /register/email/submitToken
//!   3. `mark_token_used`               — POST /register/email/submitToken
//!
//! The route layer is now free of `crate::storage::*` references for
//! this surface. See `web::routes::auth_compat` for the call sites.

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use crate::common::error::ApiError;
use crate::storage::email_verification::EmailVerificationStorage;

#[derive(Clone)]
pub struct EmailVerificationService {
    storage: Arc<EmailVerificationStorage>,
}

impl EmailVerificationService {
    pub fn new(storage: Arc<EmailVerificationStorage>) -> Self {
        Self { storage }
    }

    pub async fn create_verification_token(
        &self,
        email: &str,
        token: &str,
        ttl_seconds: i64,
        user_id: Option<&str>,
        session_data: Option<Value>,
    ) -> Result<i64, ApiError> {
        self.storage
            .create_verification_token(email, token, ttl_seconds, user_id, session_data)
            .await
            .map_err(|e| {
                ::tracing::error!(
                    email = %email,
                    user_id = ?user_id,
                    ttl_seconds,
                    error = %e,
                    "Failed to store email verification token"
                );
                ApiError::internal("Failed to store verification token. Please try again later.".to_string())
            })
    }

    pub async fn submit_token(
        &self,
        sid: i64,
        client_secret: &str,
        token: &str,
    ) -> Result<(), ApiError> {
        let verification_token = self
            .storage
            .get_verification_token_by_id(sid)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get verification token", &e))?;

        let verification_token = verification_token
            .ok_or_else(|| ApiError::bad_request("Invalid session ID or session not found".to_string()))?;

        if verification_token.is_used {
            return Err(ApiError::bad_request("Verification token has already been used".to_string()));
        }

        if verification_token.expires_at < Utc::now().timestamp_millis() {
            return Err(ApiError::bad_request("Verification token has expired".to_string()));
        }

        if verification_token.token != token {
            return Err(ApiError::bad_request("Invalid verification token".to_string()));
        }

        // Mirror the legacy session_data->client_secret check.
        let stored_secret = match verification_token.session_data.as_ref() {
            Some(Value::String(s)) => Some(s.as_str()),
            Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
            _ => None,
        };
        if stored_secret != Some(client_secret) {
            return Err(ApiError::bad_request("Client secret mismatch".to_string()));
        }

        self.storage
            .mark_token_used(sid)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to mark token as used", &e))?;
        Ok(())
    }
}
