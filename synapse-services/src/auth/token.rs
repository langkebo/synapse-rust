use super::AuthService;
use super::ADMIN_CACHE_TTL_SECS;
use super::REVOCATION_CHECK_CACHE_TTL_SECS;
use super::TOKEN_CACHE_TTL_SECS;
use super::USER_ACTIVE_CACHE_TTL_SECS;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use synapse_common::current_timestamp_millis;
use synapse_common::*;
use synapse_storage::refresh_token::CreateRefreshTokenRequest;

impl AuthService {
    /// See [`validate_token`].
    pub async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)> {
        ::tracing::debug!(target: "token_validation", "Validating token");

        // MSC3861: When a MAS token validator is configured, first try the
        // MAS path. MAS tokens are RS256/ES256/EdDSA JWTs issued by the
        // external MAS provider; the validator returns `Ok(Some(claims))`
        // for valid MAS tokens, `Ok(None)` for non-MAS tokens (fall back to
        // local HS256 path), and `Err` for tokens that look like MAS tokens
        // but fail verification (reject — do NOT fall back, to prevent
        // confusion between providers).
        if let Some(mas_validator) = &self.mas_validator {
            match mas_validator.validate(token).await {
                Ok(Some(claims)) => {
                    ::tracing::debug!(
                        target: "token_validation",
                        user_id = %claims.user_id,
                        "MAS token validated successfully"
                    );
                    return Ok((claims.user_id, claims.device_id, claims.is_admin, false, claims.is_guest));
                }
                Ok(None) => {
                    ::tracing::debug!(target: "token_validation", "Token is not a MAS token, falling back to local HS256 path");
                }
                Err(e) => {
                    ::tracing::warn!(
                        target: "token_validation",
                        error = %e,
                        "MAS token validation failed for a token that appears to be a MAS token"
                    );
                    return Err(e);
                }
            }
        }

        // S4 修复：撤销/黑名单状态短 TTL 缓存。此前每个认证请求都在此固定
        // 执行 2 次串行 DB 查询（is_in_blacklist + is_token_revoked），挂在
        // 全站最热路径（sync、发消息等所有 C-S 调用）上。撤销状态变更频率
        // 远低于请求频率：DB 检查通过后写入 `token:revocation_ok:{hash}`
        // 标记（TTL 30s），命中即跳过 DB；所有撤销写入路径（logout /
        // logout_all / change_password / deactivate_user / revoke_device(s)）
        // 主动删除对应标记保证即时生效，TTL 作为异常路径的兜底上限。
        // 注意：拒绝结果（已撤销/已拉黑）不缓存，避免负缓存放大。
        let revocation_ok_key = Self::revocation_ok_key(token);
        if self.cache.get_raw(&revocation_ok_key).is_none() {
            if self
                .token_storage
                .is_in_blacklist(token)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to check token blacklist", &e))?
            {
                ::tracing::debug!(target: "token_validation", "Token found in blacklist");
                return Err(ApiError::unauthorized("Token has been revoked".to_string()));
            }

            if self
                .token_storage
                .is_token_revoked(token)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to check token status", &e))?
            {
                ::tracing::debug!(target: "token_validation", "Token has been revoked in database");
                return Err(ApiError::unauthorized("Token has been revoked".to_string()));
            }

            self.cache.set_raw(&revocation_ok_key, "1", REVOCATION_CHECK_CACHE_TTL_SECS).await;
        }

        let claims = self.decode_token(token).map_err(|e| {
            ::tracing::debug!(target: "token_validation", "Token validation failed: {}", e);
            ApiError::unauthorized("Invalid token".to_string())
        })?;

        if claims.exp < Utc::now().timestamp() {
            ::tracing::debug!(target: "token_validation", "Token expired");
            return Err(ApiError::unauthorized("Token expired".to_string()));
        }

        let logout_marker = format!("user:logout_all:{}", claims.sub);
        if let Some(marker_val) = self.cache.get_raw(&logout_marker) {
            if let Ok(logout_ts) = marker_val.parse::<i64>() {
                if claims.iat < logout_ts {
                    ::tracing::debug!(target: "token_validation", "User has been logged out from all devices (token issued before logout)");
                    return Err(ApiError::unauthorized("Token has been revoked".to_string()));
                }
            }
        }

        let cached_token = self.cache.get_token(token).await;
        if let Some(cached_claims) = cached_token {
            ::tracing::debug!(target: "token_validation", "Found cached token for user: {}",
                cached_claims.sub);
            let admin_cache_key = format!("user:admin:{}", cached_claims.sub);

            if let Some(active) = self.cache.is_user_active(&cached_claims.sub).await {
                ::tracing::debug!(target: "token_validation", "Cache hit for user active: {:?}", active);
                return if active {
                    let shadow_key = format!("user:shadow_banned:{}", cached_claims.sub);
                    let guest_key = format!("user:guest:{}", cached_claims.sub);

                    let cached_admin = self.cache.get::<bool>(&admin_cache_key).await?;
                    let cached_shadow = self.cache.get::<bool>(&shadow_key).await?;
                    let cached_guest = self.cache.get::<bool>(&guest_key).await?;

                    let (is_admin, is_shadow_banned, is_guest) = match (cached_admin, cached_shadow, cached_guest) {
                        (Some(a), Some(s), Some(g)) => (a, s, g),
                        _ => {
                            let user = self
                                .user_storage
                                .get_user_by_id(&cached_claims.sub)
                                .await
                                .map_err(|e| ApiError::internal_with_context("Database error", &e))?
                                .ok_or_else(|| ApiError::unauthorized("User not found".to_string()))?;
                            self.cache.set(&admin_cache_key, user.is_admin, ADMIN_CACHE_TTL_SECS).await?;
                            self.cache.set(&shadow_key, user.is_shadow_banned, USER_ACTIVE_CACHE_TTL_SECS).await?;
                            self.cache.set(&guest_key, user.is_guest, USER_ACTIVE_CACHE_TTL_SECS).await?;
                            (user.is_admin, user.is_shadow_banned, user.is_guest)
                        }
                    };

                    Ok((cached_claims.user_id, cached_claims.device_id.clone(), is_admin, is_shadow_banned, is_guest))
                } else {
                    Err(ApiError::unauthorized("User not found or deactivated".to_string()))
                };
            }

            ::tracing::debug!(target: "token_validation", "Cache miss for user active status, querying DB");

            let user = self
                .user_storage
                .get_user_by_id(&cached_claims.sub)
                .await
                .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

            return if let Some(u) = user {
                let is_active = !u.is_deactivated;
                ::tracing::debug!(target: "token_validation", "User found, is_deactivated: {:?}, is_active: {}", u.is_deactivated, is_active);

                self.cache.set_user_active(&cached_claims.sub, is_active, USER_ACTIVE_CACHE_TTL_SECS).await;
                self.cache.set(&admin_cache_key, u.is_admin, ADMIN_CACHE_TTL_SECS).await?;
                self.cache
                    .set(
                        &format!("user:shadow_banned:{}", cached_claims.sub),
                        u.is_shadow_banned,
                        USER_ACTIVE_CACHE_TTL_SECS,
                    )
                    .await?;
                self.cache
                    .set(&format!("user:guest:{}", cached_claims.sub), u.is_guest, USER_ACTIVE_CACHE_TTL_SECS)
                    .await?;

                if is_active {
                    Ok((
                        cached_claims.user_id,
                        cached_claims.device_id.clone(),
                        u.is_admin,
                        u.is_shadow_banned,
                        u.is_guest,
                    ))
                } else {
                    Err(ApiError::unauthorized("User is deactivated".to_string()))
                }
            } else {
                ::tracing::debug!(target: "token_validation", "User not found in database");
                self.cache.set_user_active(&cached_claims.sub, false, USER_ACTIVE_CACHE_TTL_SECS).await;
                Err(ApiError::unauthorized("User not found".to_string()))
            };
        }

        ::tracing::debug!(target: "token_validation", "Token not found in cache, using decoded JWT");

        ::tracing::debug!(target: "token_validation", "Decoded JWT for user: {}", claims.sub);

        let user = self
            .user_storage
            .get_user_by_id(&claims.sub)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

        match user {
            Some(u) => {
                ::tracing::debug!(target: "token_validation", "User found, is_deactivated: {:?}", u.is_deactivated);
                if u.is_deactivated {
                    ::tracing::debug!(target: "token_validation", "User is deactivated, rejecting token");
                    return Err(ApiError::user_deactivated("User is deactivated"));
                }
                let is_admin = u.is_admin;
                let mut final_claims = claims.clone();
                final_claims.is_admin = is_admin;

                self.cache.set_user_active(&claims.sub, true, USER_ACTIVE_CACHE_TTL_SECS).await;
                self.cache.set(&format!("user:admin:{}", claims.sub), is_admin, ADMIN_CACHE_TTL_SECS).await?;
                self.cache
                    .set(&format!("user:shadow_banned:{}", claims.sub), u.is_shadow_banned, USER_ACTIVE_CACHE_TTL_SECS)
                    .await?;
                self.cache.set(&format!("user:guest:{}", claims.sub), u.is_guest, USER_ACTIVE_CACHE_TTL_SECS).await?;
                self.cache.set_token(token, &final_claims, TOKEN_CACHE_TTL_SECS).await;
                Ok((final_claims.user_id, final_claims.device_id.clone(), is_admin, u.is_shadow_banned, u.is_guest))
            }
            None => {
                ::tracing::debug!(target: "token_validation", "User not found in database");
                Err(ApiError::unauthorized("User not found".to_string()))
            }
        }
    }

    /// See [`generate_access_token`].
    pub async fn generate_access_token(&self, user_id: &str, device_id: &str, admin: bool) -> ApiResult<String> {
        let now = Utc::now();
        let claims = super::ClaimsBuilder::new()
            .sub(user_id.to_string())
            .user_id(user_id.to_string())
            .is_admin(admin)
            .exp((now + Duration::seconds(self.token_expiry)).timestamp())
            .iat(now.timestamp())
            .device_id(Some(device_id.to_string()))
            .iss(&self.server_name)
            .aud(&self.server_name)
            .build();

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(&self.jwt_secret))
            .map_err(|e| ApiError::internal_with_context("Failed to generate token", &e))?;

        let expires_at = (now + Duration::seconds(self.token_expiry)).timestamp_millis();

        self.token_storage
            .create_token(&token, user_id, Some(device_id), Some(expires_at))
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to store token", &e))?;

        Ok(token)
    }

    /// Generate a new refresh token, linked to `access_token` for cache
    /// invalidation during rotation (P2-12, Synapse v1.154 #19483).
    ///
    /// The `access_token` string is stored in `refresh_tokens.access_token_id`
    /// so that `refresh_token()` can look it up and call `cache.delete_token()`
    /// when the token is rotated. Without this linkage, a stale cache entry
    /// would allow the old (post-rotation invalid) access_token to pass
    /// validation until its TTL expires.
    pub async fn generate_refresh_token(
        &self,
        user_id: &str,
        device_id: &str,
        access_token: &str,
    ) -> ApiResult<String> {
        let token = super::auth_generate_token(32);
        let token_hash = Self::hash_token(&token);
        let expiry_ts = current_timestamp_millis() + (self.refresh_token_expiry * 1000);

        let request = CreateRefreshTokenRequest {
            token_hash: token_hash.clone(),
            user_id: user_id.to_string(),
            device_id: Some(device_id.to_string()),
            // P2-12: persist the linked access_token so refresh_token() can
            // invalidate its cache entry on rotation.
            access_token_id: Some(access_token.to_string()),
            scope: None,
            expires_at: expiry_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };

        self.refresh_token_storage
            .create_token(request)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to store refresh token", &e))?;

        Ok(token)
    }

    /// See [`hash_token`].
    pub(crate) fn hash_token(token: &str) -> String {
        synapse_common::crypto::hash_token(token)
    }

    /// See [`hash_token_legacy`].
    pub(crate) fn hash_token_legacy(token: &str) -> String {
        synapse_common::crypto::hash_token_legacy(token)
    }

    /// S4：撤销检查缓存键（按 token 哈希，避免在缓存键中暴露原始 token）。
    pub(crate) fn revocation_ok_key(token: &str) -> String {
        Self::revocation_ok_key_for_hash(&Self::hash_token(token))
    }

    /// See [`revocation_ok_key_for_hash`].
    pub(crate) fn revocation_ok_key_for_hash(token_hash: &str) -> String {
        format!("token:revocation_ok:{token_hash}")
    }

    /// S4：单 token 撤销（logout）后调用，立即失效其撤销检查标记。
    pub(crate) async fn invalidate_revocation_ok_by_hash(&self, token_hash: &str) {
        self.cache.delete(&Self::revocation_ok_key_for_hash(token_hash)).await;
    }

    /// S4：用户级撤销（logout_all / change_password / deactivate_user /
    /// revoke_device(s)）后调用，清除该用户全部 access token 的撤销检查标记。
    /// 枚举失败不阻断主流程——标记在 TTL（30s）内自然过期兜底。
    pub(crate) async fn invalidate_revocation_ok_for_user(&self, user_id: &str) {
        match self.token_storage.get_user_tokens(user_id).await {
            Ok(tokens) => {
                for token in tokens {
                    self.invalidate_revocation_ok_by_hash(&token.token_hash).await;
                }
            }
            Err(e) => {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "revocation_cache_invalidation_failed",
                    user_id = %user_id,
                    error = %e,
                    "Failed to enumerate tokens for revocation-cache invalidation; entries expire within TTL"
                );
            }
        }
    }

    /// See [`decode_token`].
    pub(crate) fn decode_token(&self, token: &str) -> Result<super::Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5;
        validation.set_required_spec_claims(&["exp", "iat", "sub"]);
        // P1-18: Validate issuer and audience to prevent token confusion when
        // jwt_secret is reused across services. server_name is both issuer and
        // audience for self-issued access tokens.
        validation.set_issuer(&[&self.server_name]);
        validation.set_audience(&[&self.server_name]);
        jsonwebtoken::decode(token, &DecodingKey::from_secret(&self.jwt_secret), &validation).map(|e| e.claims)
    }
}

#[cfg(test)]
mod s4_revocation_cache_tests {
    //! S4 修复的 TDD 测试：令牌撤销/黑名单检查结果的短 TTL 缓存。
    //!
    //! 背景：此前 `validate_token` 在任何缓存判断之前，对每个认证请求固定
    //! 执行 2 次串行 DB 查询（is_in_blacklist + is_token_revoked），挂在全站
    //! 最热路径上。修复后：DB 检查通过的结果写入 `token:revocation_ok:{hash}`
    //! 标记（短 TTL），命中即跳过 DB；所有撤销写入路径主动失效标记。

    use super::super::test_harness::build_test_auth_service;
    use synapse_storage::token::AccessTokenStoreApi;

    #[tokio::test]
    async fn revocation_checks_are_cached_after_first_db_pass() {
        let h = build_test_auth_service();
        let token = h.service.generate_access_token("@alice:example.com", "DEV1", false).await.unwrap();

        // 首次校验：走 DB 检查，通过后写入撤销检查标记。
        h.service.validate_token(&token).await.unwrap();
        let key = super::AuthService::revocation_ok_key(&token);
        assert!(h.cache.get_raw(&key).is_some(), "S4: 首次校验通过后必须写入撤销检查标记");

        // 绕过 logout 直接在 mock 存储中拉黑（无失效钩子）。
        // 若实现仍每次查库，本次校验必然失败；命中缓存则通过——证明 DB 检查被跳过。
        h.token_store.add_to_blacklist(&token, "@alice:example.com", None).await.unwrap();
        h.service
            .validate_token(&token)
            .await
            .unwrap_or_else(|e| panic!("S4: 标记命中时应跳过 DB 撤销检查，却得到错误: {e:?}"));
    }

    #[tokio::test]
    async fn logout_invalidates_revocation_cache_immediately() {
        let h = build_test_auth_service();
        let token = h.service.generate_access_token("@alice:example.com", "DEV1", false).await.unwrap();
        h.service.validate_token(&token).await.unwrap();
        assert!(h.cache.get_raw(&super::AuthService::revocation_ok_key(&token)).is_some());

        h.service.logout(&token, None).await.unwrap();

        assert!(
            h.cache.get_raw(&super::AuthService::revocation_ok_key(&token)).is_none(),
            "S4: logout 后撤销检查标记必须立即失效"
        );
        let result = h.service.validate_token(&token).await;
        assert!(result.is_err(), "logout 后 token 必须立即失效，却因缓存残留而通过");
    }

    #[tokio::test]
    async fn logout_all_invalidates_revocation_cache_for_all_user_tokens() {
        let h = build_test_auth_service();
        let token_a = h.service.generate_access_token("@alice:example.com", "DEV-A", false).await.unwrap();
        let token_b = h.service.generate_access_token("@alice:example.com", "DEV-B", false).await.unwrap();
        h.service.validate_token(&token_a).await.unwrap();
        h.service.validate_token(&token_b).await.unwrap();

        h.service.logout_all("@alice:example.com").await.unwrap();

        assert!(h.service.validate_token(&token_a).await.is_err(), "logout_all 后 token_a 必须失效");
        assert!(h.service.validate_token(&token_b).await.is_err(), "logout_all 后 token_b 必须失效");
    }

    #[tokio::test]
    async fn blacklisted_token_rejected_and_no_marker_written() {
        let h = build_test_auth_service();
        let token = h.service.generate_access_token("@alice:example.com", "DEV1", false).await.unwrap();
        h.token_store.add_to_blacklist(&token, "@alice:example.com", None).await.unwrap();

        let result = h.service.validate_token(&token).await;
        assert!(result.is_err(), "已拉黑的 token 必须被拒绝");
        assert!(
            h.cache.get_raw(&super::AuthService::revocation_ok_key(&token)).is_none(),
            "S4: 被撤销的 token 不得写入撤销检查标记（负结果不缓存）"
        );
    }
}
