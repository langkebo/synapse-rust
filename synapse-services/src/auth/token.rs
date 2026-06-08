use super::AuthService;
use super::ADMIN_CACHE_TTL_SECS;
use super::TOKEN_CACHE_TTL_SECS;
use super::USER_ACTIVE_CACHE_TTL_SECS;
use synapse_common::*;
use synapse_storage::refresh_token::CreateRefreshTokenRequest;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

impl AuthService {
    pub async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)> {
        ::tracing::debug!(target: "token_validation", "Validating token");

        if self
            .token_storage
            .is_in_blacklist(token)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check token blacklist", &e))?
        {
            ::tracing::debug!(target: "token_validation", "Token found in blacklist");
            return Err(ApiError::unauthorized("Token has been revoked".to_string()));
        }

        if self
            .token_storage
            .is_token_revoked(token)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check token status", &e))?
        {
            ::tracing::debug!(target: "token_validation", "Token has been revoked in database");
            return Err(ApiError::unauthorized("Token has been revoked".to_string()));
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
                                .map_err(|e| ApiError::internal_with_log("Database error", &e))?
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
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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

    pub async fn generate_access_token(&self, user_id: &str, device_id: &str, admin: bool) -> ApiResult<String> {
        let now = Utc::now();
        let jti = uuid::Uuid::new_v4().to_string();
        let claims = super::Claims {
            sub: user_id.to_string(),
            user_id: user_id.to_string(),
            jti,
            is_admin: admin,
            exp: (now + Duration::seconds(self.token_expiry)).timestamp(),
            iat: now.timestamp(),
            device_id: Some(device_id.to_string()),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(&self.jwt_secret))
            .map_err(|e| ApiError::internal_with_log("Failed to generate token", &e))?;

        let expires_at = (now + Duration::seconds(self.token_expiry)).timestamp_millis();

        self.token_storage
            .create_token(&token, user_id, Some(device_id), Some(expires_at))
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to store token", &e))?;

        Ok(token)
    }

    pub async fn generate_refresh_token(&self, user_id: &str, device_id: &str) -> ApiResult<String> {
        let token = super::auth_generate_token(32);
        let token_hash = Self::hash_token(&token);
        let expiry_ts = Utc::now().timestamp_millis() + (self.refresh_token_expiry * 1000);

        let request = CreateRefreshTokenRequest {
            token_hash: token_hash.clone(),
            user_id: user_id.to_string(),
            device_id: Some(device_id.to_string()),
            access_token_id: None,
            scope: None,
            expires_at: expiry_ts,
            client_info: None,
            ip_address: None,
            user_agent: None,
        };

        self.refresh_token_storage
            .create_token(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to store refresh token", &e))?;

        Ok(token)
    }

    pub(crate) fn hash_token(token: &str) -> String {
        synapse_common::crypto::hash_token(token)
    }

    pub(crate) fn hash_token_legacy(token: &str) -> String {
        synapse_common::crypto::hash_token_legacy(token)
    }

    pub(crate) fn decode_token(&self, token: &str) -> Result<super::Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5;
        validation.set_required_spec_claims(&["exp", "iat", "sub"]);
        jsonwebtoken::decode(token, &DecodingKey::from_secret(&self.jwt_secret), &validation).map(|e| e.claims)
    }
}
