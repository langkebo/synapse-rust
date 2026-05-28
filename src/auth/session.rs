use super::AuthService;
use crate::common::*;
use chrono::Utc;

impl AuthService {
    pub async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()> {
        let claims = self.decode_token(access_token).ok();
        let user_id = claims.as_ref().map_or("unknown", |c| c.sub.as_str());

        self.token_storage
            .add_to_blacklist(access_token, user_id, Some("User logout"))
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add token to blacklist", &e))?;

        self.token_storage
            .delete_token(access_token)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete token", &e))?;

        if let Some(d_id) = device_id {
            self.token_storage
                .delete_device_tokens(d_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to delete device tokens", &e))?;

            if let Some(c) = claims.as_ref() {
                if let Err(e) = self
                    .refresh_token_storage
                    .revoke_device_tokens(&c.sub, d_id, "user_logout")
                    .await
                {
                    ::tracing::error!(
                        target: "security_audit",
                        event = "refresh_token_revoke_failed_after_logout",
                        user_id = c.sub.as_str(),
                        device_id = d_id,
                        error = %e,
                        "Failed to revoke device refresh tokens during logout"
                    );
                    return Err(ApiError::internal_with_log("Failed to invalidate refresh tokens", &e));
                }
            }
        }

        ::tracing::info!(
            target: "security_audit",
            event = "user_logout",
            user_id = user_id,
            device_id = device_id,
            "User logged out, token blacklisted"
        );

        Ok(())
    }

    pub async fn logout_all(&self, user_id: &str) -> ApiResult<()> {
        let tokens = self
            .token_storage
            .get_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user tokens", &e))?;

        for token in &tokens {
            if let Err(e) = self
                .token_storage
                .add_hash_to_blacklist(&token.token_hash, user_id, Some("Logout all devices"))
                .await
            {
                ::tracing::warn!("Failed to add token to blacklist during logout_all: {}", e);
            }
        }

        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete tokens", &e))?;

        self.refresh_token_storage
            .revoke_all_user_tokens(user_id, "Logout all devices")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to revoke refresh tokens", &e))?;

        self.device_storage
            .delete_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete devices", &e))?;

        let logout_marker = format!("user:logout_all:{user_id}");
        let now = Utc::now().timestamp();
        self.cache
            .set_raw(&logout_marker, &now.to_string(), super::TOKEN_CACHE_TTL_SECS)
            .await;

        Ok(())
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)> {
        let token_hash = Self::hash_token(refresh_token);

        let token_data = self
            .refresh_token_storage
            .get_token(&token_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let (token_data, token_hash) = match token_data {
            Some(t) => (t, token_hash),
            None => {
                let legacy_hash = Self::hash_token_legacy(refresh_token);
                let legacy_data = self
                    .refresh_token_storage
                    .get_token(&legacy_hash)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
                match legacy_data {
                    Some(t) => (t, legacy_hash),
                    None => {
                        return Err(ApiError::unauthorized("Invalid refresh token".to_string()));
                    }
                }
            }
        };
        let t = token_data;
        if t.is_revoked {
            if let Err(e) = self
                .refresh_token_storage
                .revoke_all_user_tokens(&t.user_id, "refresh_token_reuse_detected")
                .await
            {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "refresh_token_reuse_revoke_failed",
                    user_id = t.user_id.as_str(),
                    error = %e,
                    "Failed to revoke user tokens after reuse detection"
                );
            }
            ::tracing::warn!(
                target: "security_audit",
                event = "refresh_token_reuse_detected",
                user_id = t.user_id.as_str(),
                "Revoked refresh token replayed; revoking all user refresh tokens"
            );
            return Err(ApiError::unauthorized(
                "Refresh token has been revoked".to_string(),
            ));
        }

        if let Some(expires_at) = t.expires_at {
            if expires_at < Utc::now().timestamp_millis() {
                return Err(ApiError::unauthorized("Refresh token expired".to_string()));
            }
        }

        let user = self
            .user_storage
            .get_user_by_id(&t.user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        match user {
            Some(u) => {
                if u.is_deactivated {
                    return Err(ApiError::user_deactivated(
                        "User account has been deactivated",
                    ));
                }

                let claimed = self
                    .refresh_token_storage
                    .revoke_token_cas(&token_hash, "Rotated")
                    .await
                    .map_err(|e| {
                        ApiError::internal_with_log("Failed to claim refresh token", &e)
                    })?;
                if !claimed {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "refresh_token_concurrent_use",
                        user_id = u.user_id.as_str(),
                        "Concurrent refresh of the same token rejected"
                    );
                    return Err(ApiError::unauthorized(
                        "Refresh token has been revoked".to_string(),
                    ));
                }

                let device_id = match t.device_id.clone() {
                    Some(d) if !d.is_empty() => d,
                    _ => {
                        return Err(ApiError::unauthorized(
                            "Refresh token has no associated device".to_string(),
                        ));
                    }
                };
                let new_access_token = self
                    .generate_access_token(&u.user_id, &device_id, u.is_admin)
                    .await?;
                let new_refresh_token = self.generate_refresh_token(&u.user_id, &device_id).await?;

                Ok((new_access_token, new_refresh_token, device_id))
            }
            _ => Err(ApiError::unauthorized("User not found".to_string())),
        }
    }
}
