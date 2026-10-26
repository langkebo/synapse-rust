use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_storage::refresh_token::*;
use tracing::{info, instrument, warn};

/// The `RefreshTokenService` struct.
pub struct RefreshTokenService {
    storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>,
    default_expiry_ms: i64,
}

impl RefreshTokenService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>, default_expiry_ms: i64) -> Self {
        Self { storage, default_expiry_ms }
    }

    /// See [`hash_token`].
    pub fn hash_token(token: &str) -> String {
        synapse_common::crypto::hash_token(token)
    }

    /// See [`hash_token_legacy`].
    pub fn hash_token_legacy(token: &str) -> String {
        synapse_common::crypto::hash_token_legacy(token)
    }

    /// See [`generate_token`].
    pub fn generate_token() -> String {
        synapse_common::crypto::generate_token(32)
    }

    /// See [`generate_family_id`].
    pub fn generate_family_id() -> String {
        synapse_common::crypto::generate_token(16)
    }

    /// See [`create_token`].
    #[instrument(skip(self))]
    pub async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, ApiError> {
        info!(user_id = %request.user_id, device_id = ?request.device_id, "Creating refresh token");

        let token = self
            .storage
            .create_token(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create refresh token", e))?;

        Ok(token)
    }

    /// See [`validate_token`].
    #[instrument(skip(self))]
    pub async fn validate_token(&self, token: &str) -> Result<RefreshToken, ApiError> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);

        let is_blacklisted = self
            .storage
            .is_blacklisted(&token_hash)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check blacklist", e))?;

        let is_legacy_blacklisted = if !is_blacklisted {
            self.storage
                .is_blacklisted(&legacy_hash)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to check blacklist", e))?
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
            .map_err(|e| ApiError::internal_with_cause("Failed to get token", e))?;

        let token_record = match token_record {
            Some(r) => r,
            None => self
                .storage
                .get_token(&legacy_hash)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to get token", e))?
                .ok_or_else(|| ApiError::unauthorized("Invalid refresh token"))?,
        };

        if token_record.is_revoked {
            return Err(ApiError::unauthorized("Token has been revoked"));
        }

        let now = current_timestamp_millis();
        if let Some(expires_at) = token_record.expires_at {
            if expires_at < now {
                return Err(ApiError::unauthorized("Token has expired"));
            }
        }

        Ok(token_record)
    }

    /// See [`refresh_access_token`].
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

        // SECURITY (S-9 遗留修复)：此前这里误把 `user_id` 当 `family_id` 传给 `get_rotations`，
        // 而族是按 `(family_id, user_id, device_id)` 建的 ⇒ 查询恒 0 行 ⇒ 每次都新建一族，
        // 且下面第 155 行的重放检查永远拿到空列表（`docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2.5）。
        // 改用按设备查族；**读取失败必须 fail-closed**（500）—— 把"读不到"当成"还没有族"
        // 会静默跳过重放检测，那正是 S-9 要消除的形态。
        let family_id = match self
            .storage
            .get_family_id(&old_token.user_id, old_token.device_id.as_deref())
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to look up token family", e))?
        {
            Some(family_id) => family_id,
            None => {
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

        // SECURITY (S-9 / A+ W1): token-replay detection must fail-closed. The previous
        // `unwrap_or_default()` turned a transient DB error into an empty rotation list,
        // silently skipping the reuse check below and letting a replayed token pass.
        // Propagate as 500 instead so the refresh is refused rather than trusted.
        let rotations = self
            .storage
            .get_rotations(&family_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to read token rotations for replay check", e))?;
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
                    .map_err(|e| ApiError::internal_with_cause("Failed to mark family compromised", e))?;

                self.storage
                    .revoke_all_user_tokens(&old_token.user_id, "Potential token replay attack")
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to revoke tokens", e))?;

                return Err(ApiError::unauthorized("Token reuse detected. All tokens revoked."));
            }
        }

        // CONC-02: revoke-old / insert-new / record-rotation happen in one
        // transaction. A `None` result means the CAS lost (the old token was
        // already revoked) and nothing was written.
        let new_token_record = self
            .storage
            .rotate_token_atomic(
                &old_token_hash,
                "Rotated",
                CreateRefreshTokenRequest {
                    token_hash: new_token_hash,
                    user_id: old_token.user_id.clone(),
                    device_id: old_token.device_id.clone(),
                    access_token_id: Some(new_access_token_id.to_string()),
                    scope: old_token.scope.clone(),
                    expires_at: current_timestamp_millis() + self.default_expiry_ms,
                    client_info: old_token.client_info.clone(),
                    ip_address: ip_address.map(|s| s.to_string()),
                    user_agent: user_agent.map(|s| s.to_string()),
                },
                &family_id,
                "refresh",
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to rotate refresh token", e))?;

        let new_token_record = match new_token_record {
            Some(record) => record,
            None => {
                // CAS failed — token was already revoked. Distinguish between:
                //   1. Concurrent retry (revoked_reason == "Rotated"): a legitimate
                //      parallel refresh or network retry. Do NOT nuke the family.
                //   2. Actual replay attack (revoked_reason is null or different):
                //      revoke the entire family as before.
                let current_token = self
                    .storage
                    .get_token(&old_token_hash)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to re-read token after CAS miss", e))?;

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
                            "Refresh token has already been used. Please use the new token.",
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
                    .map_err(|e| ApiError::internal_with_cause("Failed to mark family compromised", e))?;

                self.storage
                    .revoke_all_user_tokens(&old_token.user_id, "Token rotation race condition")
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to revoke tokens", e))?;

                return Err(ApiError::unauthorized("Token reuse detected. All tokens revoked."));
            }
        };

        let usage_request = RecordUsageRequest::new(old_token.id, &old_token.user_id, new_access_token_id, true);

        let usage_request = if let Some(old_at) = &old_token.access_token_id {
            usage_request.old_access_token_id(old_at)
        } else {
            usage_request
        };

        let usage_request = if let Some(ip) = ip_address { usage_request.ip_address(ip) } else { usage_request };

        let usage_request = if let Some(ua) = user_agent { usage_request.user_agent(ua) } else { usage_request };

        if let Err(e) = self.storage.record_usage(&usage_request).await {
            // Usage history is auxiliary audit data: a failure here must not fail
            // the rotation (the caller already holds a valid new token), but it
            // must not disappear silently either.
            warn!(
                refresh_token_id = usage_request.refresh_token_id,
                user_id = %usage_request.user_id,
                error = %e,
                "Failed to record refresh-token usage"
            );
        }

        Ok((new_refresh_token, new_token_record))
    }

    /// See [`revoke_token`].
    #[instrument(skip(self))]
    pub async fn revoke_token(&self, token: &str, reason: &str) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);

        self.storage
            .revoke_token(&token_hash, reason)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to revoke token", e))?;

        self.storage
            .revoke_token(&legacy_hash, reason)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to revoke token", e))?;

        info!(token_hash_prefix = %&token_hash[..token_hash.len().min(8)], "Refresh token revoked");

        Ok(())
    }

    /// See [`revoke_token_by_id`].
    #[instrument(skip(self))]
    pub async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), ApiError> {
        self.storage
            .revoke_token_by_id(id, reason)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to revoke token", e))?;

        info!(token_id = id, "Refresh token revoked by id");

        Ok(())
    }

    /// See [`revoke_all_user_tokens`].
    #[instrument(skip(self))]
    pub async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, ApiError> {
        info!(user_id = %user_id, reason = %reason, "Revoking all refresh tokens for user");

        let count = self
            .storage
            .revoke_all_user_tokens(user_id, reason)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to revoke tokens", e))?;

        Ok(count)
    }

    /// See [`get_user_tokens`].
    #[instrument(skip(self))]
    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, ApiError> {
        let tokens = self
            .storage
            .get_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get tokens", e))?;

        Ok(tokens)
    }

    /// See [`get_active_tokens`].
    #[instrument(skip(self))]
    pub async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, ApiError> {
        let tokens = self
            .storage
            .get_active_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get active tokens", e))?;

        Ok(tokens)
    }

    /// See [`get_user_stats`].
    #[instrument(skip(self))]
    pub async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, ApiError> {
        let stats = self
            .storage
            .get_user_stats(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get stats", e))?;

        Ok(stats)
    }

    /// See [`get_usage_history`].
    #[instrument(skip(self))]
    pub async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, ApiError> {
        let history = self
            .storage
            .get_usage_history(user_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get usage history", e))?;

        Ok(history)
    }

    /// See [`add_to_blacklist`].
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
            .map_err(|e| ApiError::internal_with_cause("Failed to add to blacklist", e))?;

        self.storage
            .add_to_blacklist(&legacy_hash, token_type, user_id, expires_at, reason)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to add to blacklist", e))?;

        Ok(())
    }

    /// See [`cleanup_expired_tokens`].
    #[instrument(skip(self))]
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, ApiError> {
        info!("Cleaning up expired refresh tokens");

        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to cleanup tokens", e))?;

        let blacklist_count = self
            .storage
            .cleanup_blacklist()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to cleanup blacklist", e))?;

        info!(
            expired_token_count = count,
            blacklist_cleanup_count = blacklist_count,
            "Cleaned up expired refresh tokens"
        );

        Ok(count)
    }

    /// See [`delete_token`].
    #[instrument(skip(self))]
    pub async fn delete_token(&self, token: &str) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);
        let legacy_hash = Self::hash_token_legacy(token);

        self.storage
            .delete_token(&token_hash)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete token", e))?;

        self.storage
            .delete_token(&legacy_hash)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete token", e))?;

        Ok(())
    }

    /// See [`get_default_expiry_ms`].
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

    /// **记录缺陷**：重放同一个 refresh token 会被拒绝，但**不会撤销整个族**。
    ///
    /// 这是 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2.5 的可执行证据：
    /// `refresh_access_token` 的第一步 `validate_token` 就对 `is_revoked` 返回
    /// `401 "Token has been revoked"`，于是**走不到**后面两段族撤销代码
    /// （第 1 层前置检查 130–179、第 2 层 CAS-miss 206–254）。
    ///
    /// 实测（2026-10-26，修掉 `get_family_id` 传参问题之后仍然是这个结果）：
    /// 第二次 refresh 的错误是 `"Token has been revoked"`，而不是 `"All tokens revoked"`。
    ///
    /// **修复方向**（需要设计决策，未实施）：让 refresh 路径跳过 `is_revoked` 的前置拒绝，
    /// 在 CAS-miss 处用**时间窗口**区分"并发刷新/网络重试"与"真重放"（超出窗口即撤族）。
    /// 修好之后本测试的第 2 条断言会**转红**，届时应把断言改为
    /// `"All tokens revoked"` 且 `validate_token(&first_new).is_err()`。
    #[tokio::test]
    async fn replayed_refresh_token_is_refused_without_revoking_the_family() {
        let store = Arc::new(InMemoryRefreshTokenStore::new());
        let service = RefreshTokenService::new(store, DEFAULT_EXPIRY_MS);
        let user = "@alice:example.com";

        let raw = RefreshTokenService::generate_token();
        let hash = RefreshTokenService::hash_token(&raw);
        service
            .create_token(make_request(&hash, user, current_timestamp_millis() + DEFAULT_EXPIRY_MS))
            .await
            .expect("create_token");

        let (first_new, _) =
            service.refresh_access_token(&raw, "AT1", None, None).await.expect("第一次 refresh 应当成功");
        assert_ne!(first_new, raw);

        // 第二次仍然用旧 token ⇒ 被拒（这一层是好的）。
        let error = service.refresh_access_token(&raw, "AT2", None, None).await.expect_err("重放的 token 必须被拒绝");
        assert_eq!(
            error.message(),
            "Token has been revoked",
            "当前实现是在 validate_token 就拒绝；若变成 \"All tokens revoked\" 说明族撤销已接通，请更新本测试"
        );

        // 缺陷的可观测证据：族**没有**被撤销 ⇒ 刚发出的新 token 依然有效。
        assert!(
            service.validate_token(&first_new).await.is_ok(),
            "同族的新 token 仍有效 ⇒ 族未被封禁（§2.5）。修好后此断言应反转为 is_err()"
        );
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();

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
        let past = current_timestamp_millis() - 1000;
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
        svc.create_token(make_request(&token_hash, "@alice:example.com", now + DEFAULT_EXPIRY_MS)).await.unwrap();

        svc.delete_token(raw).await.unwrap();

        let tokens = svc.get_user_tokens("@alice:example.com").await.unwrap();
        assert!(tokens.is_empty(), "token should be deleted");
    }

    #[tokio::test]
    async fn cleanup_expired_tokens_removes_only_expired_non_revoked() {
        let svc = test_service();
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();

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
