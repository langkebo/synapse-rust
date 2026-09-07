use super::AuthService;
use synapse_common::crypto::{
    hash_password_with_params, migrate_password_hash, verify_password as verify_password_common,
};
use synapse_common::*;
impl AuthService {
    /// MSC4204 / Matrix v1.3 spec: change the user's password.
    ///
    /// * `logout_devices = true` (spec default) — revoke **all** access and
    ///   refresh tokens belonging to the user after the password update.
    /// * `logout_devices = false` — keep tokens belonging to
    ///   `current_device_id` (when supplied); only revoke other devices.
    ///   Per spec, when `logout_devices = false` the client must pass the
    ///   current `device_id` so the server knows which session to keep.
    ///   Returning `400` on a `logout_devices = false` request without a
    ///   device id matches the documented client contract.
    pub async fn change_password(
        &self,
        user_id: &str,
        current_password: Option<&str>,
        new_password: &str,
        current_device_id: Option<&str>,
        logout_devices: bool,
    ) -> ApiResult<()> {
        if let Some(pwd) = current_password {
            let user = self
                .user_storage
                .get_user_by_id(user_id)
                .await
                .map_err(|e| ApiError::internal_with_context("Database error", &e))?
                .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

            let password_hash = user.password_hash.as_deref().ok_or_else(|| {
                ApiError::forbidden("Cannot verify current password: account has no password set".to_string())
            })?;

            if !self.verify_user_password(pwd, password_hash).await? {
                return Err(ApiError::unauthorized("Current password is incorrect".to_string()));
            }
        }

        if let Err(e) = self.validator.validate_password(new_password) {
            return Err(ApiError::bad_request(format!("Password does not meet policy requirements: {e}")));
        }

        // Spec says logout_devices=false MUST keep the current session; if the
        // client did not supply a device id we cannot honour that, so fail fast
        // with 400 M_MISSING_PARAM rather than silently fall back to the
        // "revoke all" branch.
        if !logout_devices && current_device_id.is_none() {
            return Err(ApiError::bad_request(
                "logout_devices=false requires an authenticated device (current_device_id)".to_string(),
            ));
        }

        let password_hash = self.hash_password(new_password)?;
        self.user_storage
            .update_password(user_id, &password_hash)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to update password", &e))?;

        // logout_devices = true  → revoke ALL tokens (regardless of device)
        // logout_devices = false → keep current_device_id, revoke everything else
        if !logout_devices {
            // The `None` case was rejected with 400 above, so this cannot be `None` here.
            // We use `if let` to satisfy clippy::expect_used / unwrap_used while
            // still being exhaustive at the type level.
            let Some(device_id) = current_device_id else {
                // Unreachable: the early-return above guarantees current_device_id is Some.
                return Err(ApiError::internal(
                    "logout_devices=false but current_device_id is missing — invariant violated".to_string(),
                ));
            };
            self.token_storage
                .delete_user_tokens_except_device(user_id, device_id)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to invalidate access tokens", &e))?;

            self.refresh_token_storage
                .revoke_all_user_tokens_except_device(user_id, device_id, "password_changed")
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to invalidate refresh tokens", &e))?;
        } else {
            self.token_storage
                .delete_user_tokens(user_id)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to invalidate access tokens", &e))?;

            self.refresh_token_storage
                .revoke_all_user_tokens(user_id, "password_changed")
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to invalidate refresh tokens", &e))?;
        }

        ::tracing::info!(
            target: "security_audit",
            event = "password_changed",
            user_id = user_id,
            logout_devices = logout_devices,
            "Password changed; access and refresh tokens revoked"
        );

        // S4: 同步失效该用户全部 token 的撤销检查标记，保证密码修改即时生效
        // （except-device 变体会多失效当前设备一次，代价仅为一次额外 DB 检查）。
        self.invalidate_revocation_ok_for_user(user_id).await;

        Ok(())
    }

    /// See [`deactivate_user`].
    /// See [`deactivate_user`].
    pub async fn deactivate_user(&self, user_id: &str) -> ApiResult<()> {
        self.user_storage
            .set_deactivation_status(user_id, true)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to deactivate user", &e))?;

        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete tokens", &e))?;

        if let Err(e) = self.refresh_token_storage.revoke_all_user_tokens(user_id, "account_deactivated").await {
            ::tracing::error!(
                target: "security_audit",
                event = "refresh_token_revoke_failed_after_deactivation",
                user_id = user_id,
                error = %e,
                "Failed to revoke refresh tokens during account deactivation"
            );
            return Err(ApiError::internal_with_context("Failed to invalidate refresh tokens", &e));
        }

        self.device_storage
            .delete_all_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete devices", &e))?;

        self.cache.delete(&format!("user:active:{user_id}")).await;
        self.cache.delete(&format!("user:admin:{user_id}")).await;
        // S4: 同步失效该用户全部 token 的撤销检查标记，保证停用即时生效。
        self.invalidate_revocation_ok_for_user(user_id).await;

        ::tracing::info!(
            target: "security_audit",
            event = "account_deactivated",
            user_id = user_id,
            "Account deactivated; all tokens and devices revoked"
        );

        Ok(())
    }

    /// See [`revoke_device`].
    /// See [`revoke_device`].
    pub async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64> {
        let rows = self
            .device_storage
            .delete_device_returning_count(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete device", &e))?;

        if rows == 0 {
            return Ok(0);
        }

        self.token_storage
            .delete_device_tokens(device_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete device tokens", &e))?;

        if let Err(e) = self.refresh_token_storage.revoke_device_tokens(user_id, device_id, "device_deleted").await {
            ::tracing::error!(
                target: "security_audit",
                event = "refresh_token_revoke_failed_after_device_delete",
                user_id = user_id,
                device_id = device_id,
                error = %e,
                "Failed to revoke device refresh tokens after device delete"
            );
            return Err(ApiError::internal_with_context("Failed to invalidate refresh tokens", &e));
        }

        ::tracing::info!(
            target: "security_audit",
            event = "device_revoked",
            user_id = user_id,
            device_id = device_id,
            "Device deleted; tokens revoked"
        );

        // S4: 同步失效该用户全部 token 的撤销检查标记（按用户粒度失效，
        // 代价仅为被保留设备下一次请求多一次 DB 检查）。
        self.invalidate_revocation_ok_for_user(user_id).await;

        Ok(rows)
    }

    /// See [`revoke_devices`].
    /// See [`revoke_devices`].
    pub async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let rows = self
            .device_storage
            .delete_user_devices_batch(user_id, device_ids)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete devices", &e))?;

        if rows == 0 {
            return Ok(0);
        }

        for device_id in device_ids {
            if let Err(e) = self.token_storage.delete_user_device_tokens(user_id, device_id).await {
                ::tracing::error!(
                    target: "security_audit",
                    event = "access_token_delete_failed_after_device_delete",
                    user_id = user_id,
                    device_id = device_id.as_str(),
                    error = %e,
                    "Failed to delete access tokens after batch device delete"
                );
                return Err(ApiError::internal_with_context("Failed to delete device tokens", &e));
            }

            if let Err(e) = self.refresh_token_storage.revoke_device_tokens(user_id, device_id, "device_deleted").await
            {
                ::tracing::error!(
                    target: "security_audit",
                    event = "refresh_token_revoke_failed_after_device_delete",
                    user_id = user_id,
                    device_id = device_id.as_str(),
                    error = %e,
                    "Failed to revoke device refresh tokens after batch delete"
                );
                return Err(ApiError::internal_with_context("Failed to invalidate refresh tokens", &e));
            }
        }

        ::tracing::info!(
            target: "security_audit",
            event = "devices_revoked",
            user_id = user_id,
            count = device_ids.len(),
            "Devices deleted; tokens revoked"
        );

        // S4: 同步失效该用户全部 token 的撤销检查标记。
        self.invalidate_revocation_ok_for_user(user_id).await;

        Ok(rows)
    }

    /// See [`hash_password`].
    /// See [`hash_password`].
    pub(crate) fn hash_password(&self, password: &str) -> Result<String, ApiError> {
        hash_password_with_params(password, self.argon2_m_cost, self.argon2_t_cost, self.argon2_p_cost)
            .map_err(ApiError::internal)
    }

    /// See [`hash_password_for_storage`].
    /// See [`hash_password_for_storage`].
    pub async fn hash_password_for_storage(&self, password: &str) -> Result<String, ApiError> {
        let auth = self.clone();
        let password_str = password.to_string();

        tokio::task::spawn_blocking(move || auth.hash_password(&password_str))
            .await
            .map_err(|e| ApiError::internal_with_context("Hashing task panicked", &e))?
    }

    /// See [`verify_password`].
    /// See [`verify_password`].
    pub(crate) fn verify_password(&self, password: &str, password_hash: &str) -> Result<bool, ApiError> {
        verify_password_common(password, password_hash, self.allow_legacy_hashes).map_err(ApiError::internal)
    }

    /// See [`migrate_password`].
    /// See [`migrate_password`].
    pub(crate) async fn migrate_password(&self, user_id: &str, password: &str) -> Result<(), ApiError> {
        let start = std::time::Instant::now();

        let password_str = password.to_string();
        let m_cost = self.argon2_m_cost;
        let t_cost = self.argon2_t_cost;
        let p_cost = self.argon2_p_cost;

        let new_hash =
            tokio::task::spawn_blocking(move || migrate_password_hash(&password_str, m_cost, t_cost, p_cost))
                .await
                .map_err(|e| ApiError::internal_with_context("Migration task panicked", &e))?
                .map_err(ApiError::internal)?;

        self.user_storage
            .update_password(user_id, &new_hash)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to update password hash", &e))?;

        let duration = start.elapsed().as_secs_f64();

        ::tracing::info!(
            target: "password_migration",
            event = "password_migrated",
            user_id = user_id,
            duration_ms = duration * 1000.0,
            "Successfully migrated legacy password hash to Argon2"
        );

        self.increment_counter("password_migration_success_total");

        if let Some(hist) = self.metrics.get_histogram("password_migration_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("password_migration_duration_seconds".to_string());
            hist.observe(duration);
        }

        Ok(())
    }

    /// See [`generate_email_verification_token`].
    /// See [`generate_email_verification_token`].
    pub fn generate_email_verification_token(&self) -> ApiResult<String> {
        let token = super::auth_generate_token(32);
        Ok(token)
    }
}
