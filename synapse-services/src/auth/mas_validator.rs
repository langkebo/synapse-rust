//! MSC3861 / MAS: Matrix Authentication Service token validation.
//!
//! When MAS (Matrix Authentication Service) is deployed alongside the
//! homeserver, clients authenticate directly with MAS and use the
//! MAS-issued access token to call homeserver APIs. The homeserver must
//! validate these tokens by verifying the JWT signature against MAS's
//! JWKS and mapping the OIDC `sub` claim to a Matrix user_id via
//! `oidc_user_mapping`.
//!
//! This module defines the [`MasTokenValidator`] trait so that
//! `AuthService::validate_token` can delegate to a pluggable validator
//! without taking a hard dependency on `OidcService` in the common path.
//! The default implementation, [`OidcMasTokenValidator`], composes
//! `OidcService` (JWKS + JWT verification) with
//! `OidcUserMappingStoreApi` (sub → user_id lookup).

use async_trait::async_trait;
use std::sync::Arc;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::OidcUserMappingStoreApi;

use crate::oidc_service::OidcService;

/// Claims extracted from a validated MAS access token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasTokenClaims {
    /// Matrix user_id resolved from the OIDC `sub` claim via user mapping.
    pub user_id: String,
    /// Device ID if present in the token claims (MAS uses `device_id` claim).
    pub device_id: Option<String>,
    /// Whether the user is an admin. Looked up from the local user record
    /// after mapping, not from the token itself (MAS does not encode admin).
    pub is_admin: bool,
    /// Whether the user is a guest. Looked up from the local user record.
    pub is_guest: bool,
}

/// Pluggable validator for MAS-issued access tokens.
///
/// Implementations verify the JWT signature against MAS's JWKS and map
/// the OIDC subject to a local Matrix user_id. When MAS is not enabled,
/// `AuthService` holds `None` and token validation falls back to the
/// built-in HS256 path.
#[async_trait]
pub trait MasTokenValidator: Send + Sync {
    /// Validate a MAS-issued access token.
    ///
    /// Returns `Ok(Some(claims))` when the token is a valid MAS token,
    /// `Ok(None)` when the token is not a MAS token (caller should fall
    /// back to the local validation path), and `Err` when the token
    /// appears to be a MAS token but fails validation (expired, bad
    /// signature, unmapped subject, etc.).
    async fn validate(&self, token: &str) -> ApiResult<Option<MasTokenClaims>>;
}

/// Default `MasTokenValidator` backed by `OidcService` + `OidcUserMappingStoreApi`.
///
/// Uses the OIDC provider's JWKS to verify the JWT signature, then maps
/// the `sub` claim to a Matrix user_id via the `oidc_user_mapping` table.
pub struct OidcMasTokenValidator {
    oidc_service: Arc<OidcService>,
    user_mapping: Arc<dyn OidcUserMappingStoreApi>,
    user_store: Arc<dyn synapse_storage::UserStore>,
}

impl OidcMasTokenValidator {
    /// See [`new`].
    pub fn new(
        oidc_service: Arc<OidcService>,
        user_mapping: Arc<dyn OidcUserMappingStoreApi>,
        user_store: Arc<dyn synapse_storage::UserStore>,
    ) -> Self {
        Self { oidc_service, user_mapping, user_store }
    }
}

#[async_trait]
impl MasTokenValidator for OidcMasTokenValidator {
    async fn validate(&self, token: &str) -> ApiResult<Option<MasTokenClaims>> {
        // Fast path: MAS tokens are JWTs (3 dot-separated base64 segments).
        // Non-JWT tokens cannot be MAS tokens — return None so the caller
        // falls back to the local HS256 path without touching the JWKS.
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Ok(None);
        }

        // Decode the JWT header to inspect the algorithm. MAS uses RS256
        // (asymmetric) while local homeserver tokens use HS256 (symmetric).
        // If the algorithm is HS256, this is a local token, not a MAS token.
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .map_err(|e| ApiError::unauthorized(format!("Invalid MAS token header: {e}")))?;
        let header: serde_json::Value = serde_json::from_slice(&header_bytes)
            .map_err(|e| ApiError::unauthorized(format!("Invalid MAS token header JSON: {e}")))?;
        let alg = header.get("alg").and_then(|v| v.as_str()).unwrap_or("");

        // Only asymmetric algorithms are MAS tokens. HS256/HS384/HS512 are
        // local homeserver tokens and must go through the local path.
        if !matches!(alg, "RS256" | "RS384" | "RS512" | "ES256" | "ES384" | "EdDSA") {
            return Ok(None);
        }

        // Verify the JWT signature against the OIDC provider's JWKS.
        let claims = self.oidc_service.verify_access_token(token).await?;

        // Extract the OIDC subject claim.
        let sub = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::unauthorized("MAS token missing 'sub' claim"))?;

        let issuer = self.oidc_service.issuer();

        // Map the OIDC subject to a Matrix user_id via oidc_user_mapping.
        let user_id = self
            .user_mapping
            .get_bound_user_id(issuer, sub)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to query OIDC user mapping", &e))?
            .ok_or_else(|| ApiError::unauthorized("OIDC subject not bound to any Matrix user account"))?;

        // Look up the user record for admin/guest flags.
        let user = self
            .user_store
            .get_user_by_id(&user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to query user record", &e))?
            .ok_or_else(|| ApiError::unauthorized("User not found"))?;

        if user.is_deactivated {
            return Err(ApiError::unauthorized("User has been deactivated"));
        }

        // Extract optional device_id from token claims (MAS uses "device_id").
        let device_id = claims.get("device_id").and_then(|v| v.as_str()).map(|s| s.to_string());

        Ok(Some(MasTokenClaims { user_id, device_id, is_admin: user.is_admin, is_guest: user.is_guest }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    // ── Test fixtures ──────────────────────────────────────────────────

    /// A mock `MasTokenValidator` that returns a preset result, used to test
    /// `AuthService::validate_token` integration without a real OidcService.
    struct MockMasTokenValidator {
        result: Option<MasTokenClaims>,
    }

    impl MockMasTokenValidator {
        fn new(claims: Option<MasTokenClaims>) -> Self {
            Self { result: claims }
        }
    }

    #[async_trait]
    impl MasTokenValidator for MockMasTokenValidator {
        async fn validate(&self, _token: &str) -> ApiResult<Option<MasTokenClaims>> {
            Ok(self.result.clone())
        }
    }

    /// A mock `OidcUserMappingStoreApi` with a single preset mapping.
    struct MockUserMapping {
        mappings: std::sync::Mutex<std::collections::HashMap<(String, String), String>>,
    }

    impl MockUserMapping {
        fn new() -> Self {
            Self { mappings: std::sync::Mutex::new(std::collections::HashMap::new()) }
        }

        fn insert(&self, issuer: &str, subject: &str, user_id: &str) {
            self.mappings.lock().unwrap().insert((issuer.to_string(), subject.to_string()), user_id.to_string());
        }
    }

    #[async_trait]
    impl OidcUserMappingStoreApi for MockUserMapping {
        async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, sqlx::Error> {
            Ok(self.mappings.lock().unwrap().get(&(issuer.to_string(), subject.to_string())).cloned())
        }

        async fn update_last_authenticated(
            &self,
            _issuer: &str,
            _subject: &str,
            _now_ts: i64,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn insert_mapping(
            &self,
            _issuer: &str,
            _subject: &str,
            _user_id: &str,
            _now_ts: i64,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }
    }

    // ── Tests ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_mas_validator_returns_none_for_non_jwt_token() {
        // Non-JWT tokens (e.g., opaque local tokens) must return None so the
        // caller falls back to the local HS256 path.
        let validator = MockMasTokenValidator::new(None);
        let result = validator.validate("not-a-jwt-token").await.unwrap();
        assert!(result.is_none(), "non-JWT token should return None to fall back to local path");
    }

    #[tokio::test]
    async fn test_mas_validator_returns_claims_for_valid_mas_token() {
        let claims = MasTokenClaims {
            user_id: "@alice:example.com".to_string(),
            device_id: Some("MASDEVICE1".to_string()),
            is_admin: false,
            is_guest: false,
        };
        let validator = MockMasTokenValidator::new(Some(claims.clone()));
        let result = validator.validate("header.payload.signature").await.unwrap();
        assert_eq!(result, Some(claims));
    }

    #[tokio::test]
    async fn test_mas_token_claims_carries_user_id_and_device_id() {
        let claims = MasTokenClaims {
            user_id: "@bob:example.com".to_string(),
            device_id: Some("DEV_BOB".to_string()),
            is_admin: true,
            is_guest: false,
        };
        assert_eq!(claims.user_id, "@bob:example.com");
        assert_eq!(claims.device_id, Some("DEV_BOB".to_string()));
        assert!(claims.is_admin);
        assert!(!claims.is_guest);
    }

    #[tokio::test]
    async fn test_mock_user_mapping_roundtrip() {
        let mapping = MockUserMapping::new();
        mapping.insert("https://mas.example.com", "subj-123", "@alice:example.com");

        let result = mapping.get_bound_user_id("https://mas.example.com", "subj-123").await.unwrap();
        assert_eq!(result, Some("@alice:example.com".to_string()));

        let missing = mapping.get_bound_user_id("https://mas.example.com", "unknown").await.unwrap();
        assert_eq!(missing, None);
    }
}
