use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::refresh_token::*;
use tracing::{info, instrument, warn};

pub struct RefreshTokenService {
    storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>,
    default_expiry_ms: i64,
}

impl RefreshTokenService {
    pub fn new(storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>, default_expiry_ms: i64) -> Self {
        Self { storage, default_expiry_ms }
    }

    pub fn hash_token(token: &str) -> String {
        synapse_common::crypto::hash_token(token)
    }

    pub fn hash_token_legacy(token: &str) -> String {
        synapse_common::crypto::hash_token_legacy(token)
    }

    pub fn generate_token() -> String {
        synapse_common::crypto::generate_token(32)
    }

    pub fn generate_family_id() -> String {
        synapse_common::crypto::generate_token(16)
    }

    #[instrument(skip(self))]
    pub async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, ApiError> {
        info!(user_id = %request.user_id, device_id = ?request.device_id, "Creating refresh token");

        let token = self
            .storage
            .create_token(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create refresh token", &e))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn validate_token(&self, token: &str) -> Result<RefreshToken, ApiError> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);

        let is_blacklisted = self
            .storage
            .is_blacklisted(&token_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check blacklist", &e))?;

        let is_legacy_blacklisted = if !is_blacklisted {
            self.storage
                .is_blacklisted(&legacy_hash)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to check blacklist", &e))?
        } else {
            true
        };

        if is_legacy_blacklisted {
            return Err(ApiError::unauthorized("Token has been revoked"));
        }

        let token_record = self
            .storage
            .get_token(&token_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get token", &e))?;

        let token_record = match token_record {
            Some(r) => r,
            None => self
                .storage
                .get_token(&legacy_hash)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get token", &e))?
                .ok_or_else(|| ApiError::unauthorized("Invalid refresh token"))?,
        };

        if token_record.is_revoked {
            return Err(ApiError::unauthorized("Token has been revoked"));
        }

        let now = chrono::Utc::now().timestamp_millis();
        if let Some(expires_at) = token_record.expires_at {
            if expires_at < now {
                return Err(ApiError::unauthorized("Token has expired"));
            }
        }

        Ok(token_record)
    }

    #[instrument(skip(self))]
    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
        new_access_token_id: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(String, RefreshToken), ApiError> {
        let old_token = self.validate_token(refresh_token).await?;

        info!(
            user_id = %old_token.user_id,
            device_id = ?old_token.device_id,
            access_token_id = %new_access_token_id,
            "Refreshing access token"
        );

        let new_refresh_token = Self::generate_token();
        let new_token_hash = Self::hash_token(&new_refresh_token);
        let old_token_hash = old_token.token_hash.clone();

        let family_id = match self.storage.get_rotations(&old_token.user_id).await {
            Ok(rotations) if !rotations.is_empty() => rotations[0].family_id.clone(),
            _ => {
                let family_id = Self::generate_family_id();
                if let Err(e) =
                    self.storage.create_family(&family_id, &old_token.user_id, old_token.device_id.as_deref()).await
                {
                    warn!(
                        error = %e,
                        family_id = %family_id,
                        user_id = %old_token.user_id,
                        device_id = ?old_token.device_id,
                        "Failed to create token family"
                    );
                }
                family_id
            }
        };

        let rotations = self.storage.get_rotations(&family_id).await.unwrap_or_default();
        if let Some(last_rotation) = rotations.first() {
            if last_rotation.new_token_hash != old_token_hash {
                warn!(
                    user_id = %old_token.user_id,
                    family_id = %family_id,
                    device_id = ?old_token.device_id,
                    "Potential token replay attack detected"
                );

                self.storage
                    .mark_family_compromised(&family_id)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to mark family compromised", &e))?;

                self.storage
                    .revoke_all_user_tokens(&old_token.user_id, "Potential token replay attack")
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to revoke tokens", &e))?;

                return Err(ApiError::unauthorized("Token reuse detected. All tokens revoked."));
            }
        }

        let revoked = self
            .storage
            .revoke_token_cas(&old_token_hash, "Rotated")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to revoke old token", &e))?;

        if !revoked {
            // CAS failed — token was already revoked. Distinguish between:
            //   1. Concurrent retry (revoked_reason == "Rotated"): a legitimate
            //      parallel refresh or network retry. Do NOT nuke the family.
            //   2. Actual replay attack (revoked_reason is null or different):
            //      revoke the entire family as before.
            let current_token = self.storage.get_token(&old_token_hash).await
                .map_err(|e| ApiError::internal_with_log("Failed to re-read token after CAS miss", &e))?;

            if let Some(ref t) = current_token {
                if t.revoked_reason.as_deref() == Some("Rotated") {
                    // Benign race: another refresh already rotated this token.
                    warn!(
                        user_id = %old_token.user_id,
                        family_id = %family_id,
                        device_id = ?old_token.device_id,
                        "Token already rotated by a concurrent refresh; returning benign error"
                    );
                    return Err(ApiError::unauthorized(
                        "Refresh token has already been used. Please use the new token."
                    ));
                }
            }

            // Genuine replay or unknown state — revoke the family.
            warn!(
                user_id = %old_token.user_id,
                family_id = %family_id,
                device_id = ?old_token.device_id,
                "Token reuse detected (CAS miss with non-Rotated reason); revoking family"
            );

            self.storage
                .mark_family_compromised(&family_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to mark family compromised", &e))?;

            self.storage
                .revoke_all_user_tokens(&old_token.user_id, "Token rotation race condition")
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to revoke tokens", &e))?;

            return Err(ApiError::unauthorized("Token reuse detected. All tokens revoked."));
        }

        let new_token_record = self
            .storage
            .create_token(CreateRefreshTokenRequest {
                token_hash: new_token_hash.clone(),
                user_id: old_token.user_id.clone(),
                device_id: old_token.device_id.clone(),
                access_token_id: Some(new_access_token_id.to_string()),
                scope: old_token.scope.clone(),
                expires_at: chrono::Utc::now().timestamp_millis() + self.default_expiry_ms,
                client_info: old_token.client_info.clone(),
                ip_address: ip_address.map(|s| s.to_string()),
                user_agent: user_agent.map(|s| s.to_string()),
            })
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create new token", &e))?;

        self.storage
            .record_rotation(&family_id, Some(&old_token_hash), &new_token_hash, "refresh")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record rotation", &e))?;

        let usage_request = RecordUsageRequest::new(old_token.id, &old_token.user_id, new_access_token_id, true);

        let usage_request = if let Some(old_at) = &old_token.access_token_id {
            usage_request.old_access_token_id(old_at)
        } else {
            usage_request
        };

        let usage_request = if let Some(ip) = ip_address { usage_request.ip_address(ip) } else { usage_request };

        let usage_request = if let Some(ua) = user_agent { usage_request.user_agent(ua) } else { usage_request };

        self.storage.record_usage(&usage_request).await.ok();

        Ok((new_refresh_token, new_token_record))
    }

    #[instrument(skip(self))]
    pub async fn revoke_token(&self, token: &str, reason: &str) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);

        self.storage
            .revoke_token(&token_hash, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to revoke token", &e))?;

        self.storage
            .revoke_token(&legacy_hash, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to revoke token", &e))?;

        info!(token_hash_prefix = %&token_hash[..token_hash.len().min(8)], "Refresh token revoked");

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), ApiError> {
        self.storage
            .revoke_token_by_id(id, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to revoke token", &e))?;

        info!(token_id = id, "Refresh token revoked by id");

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, ApiError> {
        info!(user_id = %user_id, reason = %reason, "Revoking all refresh tokens for user");

        let count = self
            .storage
            .revoke_all_user_tokens(user_id, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to revoke tokens", &e))?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, ApiError> {
        let tokens = self
            .storage
            .get_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get tokens", &e))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, ApiError> {
        let tokens = self
            .storage
            .get_active_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get active tokens", &e))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, ApiError> {
        let stats = self
            .storage
            .get_user_stats(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get stats", &e))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, ApiError> {
        let history = self
            .storage
            .get_usage_history(user_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get usage history", &e))?;

        Ok(history)
    }

    #[instrument(skip(self))]
    pub async fn add_to_blacklist(
        &self,
        token: &str,
        token_type: &str,
        user_id: &str,
        expires_at: i64,
        reason: Option<&str>,
    ) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);

        self.storage
            .add_to_blacklist(&token_hash, token_type, user_id, expires_at, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add to blacklist", &e))?;

        self.storage
            .add_to_blacklist(&legacy_hash, token_type, user_id, expires_at, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add to blacklist", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, ApiError> {
        info!("Cleaning up expired refresh tokens");

        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup tokens", &e))?;

        let blacklist_count = self
            .storage
            .cleanup_blacklist()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup blacklist", &e))?;

        info!(
            expired_token_count = count,
            blacklist_cleanup_count = blacklist_count,
            "Cleaned up expired refresh tokens"
        );

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn delete_token(&self, token: &str) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);

        self.storage
            .delete_token(&token_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete token", &e))?;

        self.storage
            .delete_token(&legacy_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete token", &e))?;

        Ok(())
    }

    pub fn get_default_expiry_ms(&self) -> i64 {
        self.default_expiry_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::refresh_token::CreateRefreshTokenRequest;
    use synapse_storage::test_mocks::InMemoryRefreshTokenStore;

    const DEFAULT_EXPIRY_MS: i64 = 3_600_000; // 1 hour

    fn test_service() -> RefreshTokenService {
        RefreshTokenService::new(Arc::new(InMemoryRefreshTokenStore::new()), DEFAULT_EXPIRY_MS)
    }

    fn make_request(token_hash: &str, user_id: &str, expires_at: i64) -> CreateRefreshTokenRequest {
        CreateRefreshTokenRequest {
            token_hash: token_hash.to_string(),
            user_id: user_id.to_string(),
            device_id: Some("DEV1".to_string()),
            access_token_id: None,
            scope: None,
            expires_at,
            client_info: None,
            ip_address: None,
            user_agent: None,
        }
    }

    #[test]
    fn test_hash_token() {
        let token = "test_token_123";
        let hash1 = RefreshTokenService::hash_token(token);
        let hash2 = RefreshTokenService::hash_token(token);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 43);
    }

    #[test]
    fn test_hash_token_different() {
        let hash1 = RefreshTokenService::hash_token("token1");
        let hash2 = RefreshTokenService::hash_token("token2");

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_generate_token() {
        let token1 = RefreshTokenService::generate_token();
        let token2 = RefreshTokenService::generate_token();

        assert_ne!(token1, token2);
        assert!(!token1.is_empty());
        assert_eq!(token1.len(), 43);
    }

    #[test]
    fn test_generate_family_id() {
        let id1 = RefreshTokenService::generate_family_id();
        let id2 = RefreshTokenService::generate_family_id();

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        assert_eq!(id1.len(), 22);
    }

    #[test]
    fn test_hash_token_empty() {
        let hash = RefreshTokenService::hash_token("");
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_hash_token_special_chars() {
        let token = "token/with+special=chars";
        let hash = RefreshTokenService::hash_token(token);

        assert!(!hash.contains('/'));
        assert!(!hash.contains('+'));
        assert!(!hash.contains('='));
    }

    // ── Trait-rewired DB-free unit tests (ARC-13 InMemory Mock) ──────────
    // These tests exercise RefreshTokenService business logic via
    // InMemoryRefreshTokenStore without touching PostgreSQL.
    // Ref: TDD落地执行清单 §8.3 ARC-13.

    #[tokio::test]
    async fn create_token_returns_token_with_correct_fields() {
        let svc = test_service();
        let now = chrono::Utc::now().timestamp_millis();
        let token_hash = RefreshTokenService::hash_token("raw-token-1");
        let request = make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS);

        let token = svc.create_token(request).await.unwrap();

        assert_eq!(token.token_hash, token_hash);
        assert_eq!(token.user_id, "@alice:example.com");
        assert_eq!(token.device_id.as_deref(), Some("DEV1"));
        assert!(!token.is_revoked);
        assert_eq!(token.expires_at, Some(now + DEFAULT_EXPIRY_MS));
    }

    #[tokio::test]
    async fn validate_token_returns_token_record_for_valid_token() {
        let svc = test_service();
        let raw = "valid-raw-token-abc";
        let token_hash = RefreshTokenService::hash_token(raw);
        let now = chrono::Utc::now().timestamp_millis();
        svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        let token = svc.validate_token(raw).await.unwrap();

        assert_eq!(token.user_id, "@alice:example.com");
        assert!(!token.is_revoked);
    }

    #[tokio::test]
    async fn validate_token_revoked_returns_unauthorized() {
        let svc = test_service();
        let raw = "to-be-revoked";
        let token_hash = RefreshTokenService::hash_token(raw);
        let now = chrono::Utc::now().timestamp_millis();
        svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        svc.revoke_token(raw, "user logged out").await.unwrap();

        let result = svc.validate_token(raw).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_unauthorized(), "expected Unauthorized, got {err:?}");
    }

    #[tokio::test]
    async fn validate_token_missing_returns_unauthorized() {
        let svc = test_service();
        let result = svc.validate_token("never-issued-token").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_unauthorized());
    }

    #[tokio::test]
    async fn validate_token_blacklisted_returns_unauthorized() {
        let svc = test_service();
        let raw = "blacklisted-token";
        let now = chrono::Utc::now().timestamp_millis();

        // Blacklist the token first, then attempt validation — should be rejected
        // even though no token record exists.
        svc.add_to_blacklist(raw, "refresh", "@alice:example.com", now + DEFAULT_EXPIRY_MS, Some("compromised"))
            .await
            .unwrap();

        let result = svc.validate_token(raw).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_unauthorized());
    }

    #[tokio::test]
    async fn validate_token_expired_returns_unauthorized() {
        let svc = test_service();
        let raw = "expired-token";
        let token_hash = RefreshTokenService::hash_token(raw);
        // Create a token with an expiry in the past.
        let past = chrono::Utc::now().timestamp_millis() - 1000;
        svc.create_token(make_request(&token_hash, "@alice:example.com", past)).await.unwrap();

        let result = svc.validate_token(raw).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_unauthorized());
    }

    #[tokio::test]
    async fn revoke_token_marks_token_revoked() {
        let svc = test_service();
        let raw = "revoke-me";
        let token_hash = RefreshTokenService::hash_token(raw);
        let now = chrono::Utc::now().timestamp_millis();
        svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        svc.revoke_token(raw, "rotation").await.unwrap();

        let tokens = svc.get_user_tokens("@alice:example.com").await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(tokens[0].is_revoked);
        assert_eq!(tokens[0].revoked_reason.as_deref(), Some("rotation"));
    }

    #[tokio::test]
    async fn revoke_token_by_id_marks_token_revoked() {
        let svc = test_service();
        let token_hash = RefreshTokenService::hash_token("revoke-by-id");
        let now = chrono::Utc::now().timestamp_millis();
        let token =
            svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        svc.revoke_token_by_id(token.id, "admin_action").await.unwrap();

        let tokens = svc.get_user_tokens("@alice:example.com").await.unwrap();
        assert!(tokens[0].is_revoked);
        assert_eq!(tokens[0].revoked_reason.as_deref(), Some("admin_action"));
    }

    #[tokio::test]
    async fn revoke_all_user_tokens_returns_revoked_count() {
        let svc = test_service();
        let now = chrono::Utc::now().timestamp_millis();
        let user = "@bulk:example.com";

        for i in 0..3 {
            let hash = RefreshTokenService::hash_token(&format!("bulk-{i}"));
            svc.create_token(make_request(&hash, user, now + DEFAULT_EXPIRY_MS)).await.unwrap();
        }
        // Create one token for a different user to ensure cross-user isolation.
        let other_hash = RefreshTokenService::hash_token("other-user-token");
        svc.create_token(make_request(&other_hash, "@other:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        let count = svc.revoke_all_user_tokens(user, "logout_all").await.unwrap();
        assert_eq!(count, 3, "should revoke exactly 3 tokens for the target user");

        let user_tokens = svc.get_user_tokens(user).await.unwrap();
        assert!(user_tokens.iter().all(|t| t.is_revoked));

        let other_tokens = svc.get_user_tokens("@other:example.com").await.unwrap();
        assert!(other_tokens.iter().all(|t| !t.is_revoked), "other user tokens must remain active");
    }

    #[tokio::test]
    async fn get_user_tokens_returns_all_tokens_for_user() {
        let svc = test_service();
        let now = chrono::Utc::now().timestamp_millis();
        let user = "@alice:example.com";

        for i in 0..3 {
            let hash = RefreshTokenService::hash_token(&format!("list-{i}"));
            svc.create_token(make_request(&hash, user, now + DEFAULT_EXPIRY_MS)).await.unwrap();
        }

        let tokens = svc.get_user_tokens(user).await.unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|t| t.user_id == user));
    }

    #[tokio::test]
    async fn get_active_tokens_excludes_revoked_and_expired() {
        let svc = test_service();
        let now = chrono::Utc::now().timestamp_millis();
        let user = "@alice:example.com";

        // Active token.
        let active_hash = RefreshTokenService::hash_token("active");
        svc.create_token(make_request(&active_hash, user, now + DEFAULT_EXPIRY_MS)).await.unwrap();

        // Revoked token.
        let revoked_hash = RefreshTokenService::hash_token("revoked");
        svc.create_token(make_request(&revoked_hash, user, now + DEFAULT_EXPIRY_MS)).await.unwrap();
        svc.revoke_token("revoked", "test").await.unwrap();

        // Expired token.
        let expired_hash = RefreshTokenService::hash_token("expired");
        svc.create_token(make_request(&expired_hash, user, now - 1000)).await.unwrap();

        let active = svc.get_active_tokens(user).await.unwrap();
        assert_eq!(active.len(), 1, "only the non-revoked, non-expired token should remain");
        assert_eq!(active[0].token_hash, active_hash);
    }

    #[tokio::test]
    async fn delete_token_removes_token_completely() {
        let svc = test_service();
        let raw = "delete-me";
        let token_hash = RefreshTokenService::hash_token(raw);
        let now = chrono::Utc::now().timestamp_millis();
        svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        svc.delete_token(raw).await.unwrap();

        let tokens = svc.get_user_tokens("@alice:example.com").await.unwrap();
        assert!(tokens.is_empty(), "token should be deleted");
    }

    #[tokio::test]
    async fn cleanup_expired_tokens_removes_only_expired_non_revoked() {
        let svc = test_service();
        let now = chrono::Utc::now().timestamp_millis();
        let user = "@alice:example.com";

        // Expired, non-revoked — should be cleaned up.
        let expired_hash = RefreshTokenService::hash_token("expired");
        svc.create_token(make_request(&expired_hash, user, now - 1000)).await.unwrap();

        // Expired but revoked — should NOT be cleaned up (only !is_revoked are removed).
        let expired_revoked_hash = RefreshTokenService::hash_token("expired-revoked");
        svc.create_token(make_request(&expired_revoked_hash, user, now - 1000)).await.unwrap();
        svc.revoke_token("expired-revoked", "test").await.unwrap();

        // Active — should remain.
        let active_hash = RefreshTokenService::hash_token("active");
        svc.create_token(make_request(&active_hash, user, now + DEFAULT_EXPIRY_MS)).await.unwrap();

        let removed = svc.cleanup_expired_tokens().await.unwrap();
        assert_eq!(removed, 1, "only the expired non-revoked token should be removed");

        let tokens = svc.get_user_tokens(user).await.unwrap();
        assert_eq!(tokens.len(), 2, "revoked-expired and active tokens should remain");
    }

    #[tokio::test]
    async fn refresh_access_token_rotates_token_and_revokes_old() {
        let svc = test_service();
        let raw = "rotate-me";
        let token_hash = RefreshTokenService::hash_token(raw);
        let now = chrono::Utc::now().timestamp_millis();
        let old_token =
            svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        let (new_raw, new_token) =
            svc.refresh_access_token(raw, "new_access_id", Some("127.0.0.1"), Some("Mozilla/5.0")).await.unwrap();

        // The new token string must differ from the original.
        assert_ne!(new_raw, raw);
        // The new token record should be active and belong to the same user.
        assert_eq!(new_token.user_id, "@alice:example.com");
        assert!(!new_token.is_revoked);
        assert_eq!(new_token.access_token_id.as_deref(), Some("new_access_id"));

        // The old token must be revoked.
        let old_after = svc.validate_token(raw).await;
        assert!(old_after.is_err(), "old token should no longer validate");
        let user_tokens = svc.get_user_tokens("@alice:example.com").await.unwrap();
        let old_record = user_tokens.iter().find(|t| t.id == old_token.id).unwrap();
        assert!(old_record.is_revoked);

        // The new token must validate.
        let validated = svc.validate_token(&new_raw).await.unwrap();
        assert_eq!(validated.id, new_token.id);
    }

    #[tokio::test]
    async fn refresh_access_token_with_revoked_token_returns_unauthorized() {
        let svc = test_service();
        let raw = "already-revoked";
        let token_hash = RefreshTokenService::hash_token(raw);
        let now = chrono::Utc::now().timestamp_millis();
        svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();
        svc.revoke_token(raw, "prior_logout").await.unwrap();

        let result = svc.refresh_access_token(raw, "new_access_id", None, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_unauthorized());
    }

    #[tokio::test]
    async fn refresh_access_token_with_missing_token_returns_unauthorized() {
        let svc = test_service();
        let result = svc.refresh_access_token("never-issued", "new_access_id", None, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_unauthorized());
    }

    #[tokio::test]
    async fn get_default_expiry_ms_returns_configured_value() {
        let svc = test_service();
        assert_eq!(svc.get_default_expiry_ms(), DEFAULT_EXPIRY_MS);
    }

    #[tokio::test]
    async fn add_to_blacklist_blocks_future_validation() {
        let svc = test_service();
        let raw = "will-be-blacklisted";
        let token_hash = RefreshTokenService::hash_token(raw);
        let now = chrono::Utc::now().timestamp_millis();

        // Create a valid token first.
        svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        // Blacklist it.
        svc.add_to_blacklist(raw, "refresh", "@alice:example.com", now + DEFAULT_EXPIRY_MS, Some("compromised"))
            .await
            .unwrap();

        // Validation should now fail due to blacklist.
        let result = svc.validate_token(raw).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_unauthorized());
    }

    #[tokio::test]
    async fn get_usage_history_returns_empty_for_unmocked_user() {
        // InMemoryRefreshTokenStore returns empty Vec for usage history.
        let svc = test_service();
        let history = svc.get_usage_history("@alice:example.com", 10).await.unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn get_user_stats_returns_none_for_unmocked_user() {
        // InMemoryRefreshTokenStore returns None for stats.
        let svc = test_service();
        let stats = svc.get_user_stats("@alice:example.com").await.unwrap();
        assert!(stats.is_none());
    }
}
