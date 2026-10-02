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
use synapse_common::config::{MasConfig, OidcConfig};
use synapse_common::{ApiError, ApiResult};
use synapse_storage::{OidcUserMappingStoreApi, UserStore};

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

/// MSC3861 (H-1)：配置 → 校验器的**唯一**接线点。
///
/// 返回 `None`（默认行为，本地 HS256 路径完全不变）当 `mas.enabled = false`；
/// 返回 `Some(..)` 当 [`MasConfig::is_configured`]（`enabled` + `issuer_url` 非空）
/// **且** `client_id` 非空。
///
/// 为什么把 `client_id` 当作硬条件：本仓走的是本地 JWKS 校验（没有 introspection
/// 那一步用 client 凭据完成的绑定），`aud` 是唯一能阻止"同一 MAS 实例上为别的客户端
/// 签发的令牌"通过的东西。`Config::validate()` 已在启动期拒绝半配置，这里是第二道
/// 防线 —— 宁可**不接线**，也不能"接了却不校验 audience"。
pub fn build_mas_validator(
    mas: &MasConfig,
    user_mapping: Arc<dyn OidcUserMappingStoreApi>,
    user_store: Arc<dyn UserStore>,
) -> Option<Arc<dyn MasTokenValidator>> {
    if !mas.is_configured() || mas.client_id.trim().is_empty() {
        return None;
    }

    let oidc_config = Arc::new(OidcConfig {
        enabled: true,
        issuer: mas.issuer_url.clone(),
        client_id: mas.client_id.clone(),
        client_secret: (!mas.client_secret.trim().is_empty()).then(|| mas.client_secret.clone()),
        ..Default::default()
    });

    Some(Arc::new(OidcMasTokenValidator::new(Arc::new(OidcService::new(oidc_config)), user_mapping, user_store)))
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
            .map_err(|e| ApiError::internal_with_cause("Failed to query OIDC user mapping", e))?
            .ok_or_else(|| ApiError::unauthorized("OIDC subject not bound to any Matrix user account"))?;

        // Look up the user record for admin/guest flags.
        let user = self
            .user_store
            .get_user_by_id(&user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to query user record", e))?
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

    // ── H-1：配置 → 接线（行为断言，不是 grep）──────────────────────────────

    fn test_mas(issuer: &str, client_id: &str) -> MasConfig {
        MasConfig {
            enabled: true,
            issuer_url: issuer.to_string(),
            client_id: client_id.to_string(),
            client_secret: "secret".to_string(),
            admin_token: None,
        }
    }

    /// 只有"enabled + issuer_url + client_id"齐备才接线；任何一半都不接。
    #[test]
    fn build_mas_validator_requires_full_configuration() {
        let mapping: Arc<dyn OidcUserMappingStoreApi> =
            Arc::new(synapse_storage::test_mocks::InMemoryOidcUserMappingStore::new());
        let users: Arc<dyn UserStore> = Arc::new(synapse_storage::test_mocks::FakeUserStore::new());

        let disabled = MasConfig::default();
        assert!(build_mas_validator(&disabled, mapping.clone(), users.clone()).is_none(), "默认关闭必须不接线");

        let no_issuer = MasConfig { enabled: true, ..test_mas("", "hs-client") };
        assert!(build_mas_validator(&no_issuer, mapping.clone(), users.clone()).is_none(), "缺 issuer 必须不接线");

        let no_client_id = test_mas("https://mas.example.com", "");
        assert!(
            build_mas_validator(&no_client_id, mapping.clone(), users.clone()).is_none(),
            "缺 client_id（audience 锚点）必须不接线"
        );

        let full = test_mas("https://mas.example.com", "hs-client");
        assert!(build_mas_validator(&full, mapping, users).is_some(), "配置齐备必须接线");
    }

    /// H-1 fail-closed：校验器对"像 MAS 票"的令牌返回 `Err` 时，`validate_token`
    /// **绝不能**回落到本地 HS256 路径 —— 否则被 MAS 拒绝的令牌会在本地路径上"复活"。
    /// 反过来 `Ok(None)`（不是 MAS 票）必须回落，否则本地令牌会被 MAS 部署整体打死。
    #[tokio::test]
    async fn mas_rejection_never_falls_back_and_non_mas_falls_through() {
        use crate::auth::test_harness::build_test_auth_service;

        struct Rejecting;
        #[async_trait]
        impl MasTokenValidator for Rejecting {
            async fn validate(&self, _token: &str) -> ApiResult<Option<MasTokenClaims>> {
                Err(ApiError::unauthorized("MAS: token expired"))
            }
        }

        struct NotAMasToken;
        #[async_trait]
        impl MasTokenValidator for NotAMasToken {
            async fn validate(&self, _token: &str) -> ApiResult<Option<MasTokenClaims>> {
                Ok(None)
            }
        }

        let harness = build_test_auth_service();
        let rejecting = harness.service.with_mas_validator(Arc::new(Rejecting));
        let err = rejecting
            .validate_token("eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.c2ln")
            .await
            .expect_err("MAS 拒绝的令牌必须直接失败");
        assert!(err.to_string().contains("MAS"), "错误必须来自 MAS 路径而不是本地回落：{err}");

        let harness2 = build_test_auth_service();
        let fallthrough = harness2.service.with_mas_validator(Arc::new(NotAMasToken));
        let err2 = fallthrough.validate_token("not-a-mas-token").await.expect_err("本地路径不认识这个令牌，仍应失败");
        assert!(!err2.to_string().contains("MAS"), "Ok(None) 必须回落本地路径：{err2}");
    }

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
