use super::AuthService;
use synapse_common::crypto::{
    hash_password_with_params, migrate_password_hash, verify_password as verify_password_common,
};
use synapse_common::*;
impl AuthService {
    pub async fn change_password(
        &self,
        user_id: &str,
        current_password: Option<&str>,
        new_password: &str,
        current_device_id: Option<&str>,
    ) -> ApiResult<()> {
        if let Some(pwd) = current_password {
            let user = self
                .user_storage
                .get_user_by_id(user_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?
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

        let password_hash = self.hash_password(new_password)?;
        self.user_storage
            .update_password(user_id, &password_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update password", &e))?;

        if let Some(device_id) = current_device_id {
            self.token_storage
                .delete_user_tokens_except_device(user_id, device_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to invalidate access tokens", &e))?;

            self.refresh_token_storage
                .revoke_all_user_tokens_except_device(user_id, device_id, "password_changed")
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to invalidate refresh tokens", &e))?;
        } else {
            self.token_storage
                .delete_user_tokens(user_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to invalidate access tokens", &e))?;

            self.refresh_token_storage
                .revoke_all_user_tokens(user_id, "password_changed")
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to invalidate refresh tokens", &e))?;
        }

        ::tracing::info!(
            target: "security_audit",
            event = "password_changed",
            user_id = user_id,
            "Password changed; access and refresh tokens revoked"
        );

        Ok(())
    }

    pub async fn deactivate_user(&self, user_id: &str) -> ApiResult<()> {
        self.user_storage
            .set_deactivation_status(user_id, true)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to deactivate user", &e))?;

        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete tokens", &e))?;

        if let Err(e) = self.refresh_token_storage.revoke_all_user_tokens(user_id, "account_deactivated").await {
            ::tracing::error!(
                target: "security_audit",
                event = "refresh_token_revoke_failed_after_deactivation",
                user_id = user_id,
                error = %e,
                "Failed to revoke refresh tokens during account deactivation"
            );
            return Err(ApiError::internal_with_log("Failed to invalidate refresh tokens", &e));
        }

        self.device_storage
            .delete_all_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete devices", &e))?;

        self.cache.delete(&format!("user:active:{user_id}")).await;
        self.cache.delete(&format!("user:admin:{user_id}")).await;

        ::tracing::info!(
            target: "security_audit",
            event = "account_deactivated",
            user_id = user_id,
            "Account deactivated; all tokens and devices revoked"
        );

        Ok(())
    }

    pub async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64> {
        let rows = self
            .device_storage
            .delete_device_returning_count(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete device", &e))?;

        if rows == 0 {
            return Ok(0);
        }

        self.token_storage
            .delete_device_tokens(device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete device tokens", &e))?;

        if let Err(e) = self.refresh_token_storage.revoke_device_tokens(user_id, device_id, "device_deleted").await {
            ::tracing::error!(
                target: "security_audit",
                event = "refresh_token_revoke_failed_after_device_delete",
                user_id = user_id,
                device_id = device_id,
                error = %e,
                "Failed to revoke device refresh tokens after device delete"
            );
            return Err(ApiError::internal_with_log("Failed to invalidate refresh tokens", &e));
        }

        ::tracing::info!(
            target: "security_audit",
            event = "device_revoked",
            user_id = user_id,
            device_id = device_id,
            "Device deleted; tokens revoked"
        );

        Ok(rows)
    }

    pub async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let rows = self
            .device_storage
            .delete_user_devices_batch(user_id, device_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete devices", &e))?;

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
                return Err(ApiError::internal_with_log("Failed to delete device tokens", &e));
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
                return Err(ApiError::internal_with_log("Failed to invalidate refresh tokens", &e));
            }
        }

        ::tracing::info!(
            target: "security_audit",
            event = "devices_revoked",
            user_id = user_id,
            count = device_ids.len(),
            "Devices deleted; tokens revoked"
        );

        Ok(rows)
    }

    pub(crate) fn hash_password(&self, password: &str) -> Result<String, ApiError> {
        hash_password_with_params(password, self.argon2_m_cost, self.argon2_t_cost, self.argon2_p_cost)
            .map_err(ApiError::internal)
    }

    pub async fn hash_password_for_storage(&self, password: &str) -> Result<String, ApiError> {
        let auth = self.clone();
        let password_str = password.to_string();

        tokio::task::spawn_blocking(move || auth.hash_password(&password_str))
            .await
            .map_err(|e| ApiError::internal_with_log("Hashing task panicked", &e))?
    }

    pub(crate) fn verify_password(&self, password: &str, password_hash: &str) -> Result<bool, ApiError> {
        verify_password_common(password, password_hash, self.allow_legacy_hashes).map_err(ApiError::internal)
    }

    pub(crate) async fn migrate_password(&self, user_id: &str, password: &str) -> Result<(), ApiError> {
        let start = std::time::Instant::now();

        let password_str = password.to_string();
        let m_cost = self.argon2_m_cost;
        let t_cost = self.argon2_t_cost;
        let p_cost = self.argon2_p_cost;

        let new_hash =
            tokio::task::spawn_blocking(move || migrate_password_hash(&password_str, m_cost, t_cost, p_cost))
                .await
                .map_err(|e| ApiError::internal_with_log("Migration task panicked", &e))?
                .map_err(ApiError::internal)?;

        self.user_storage
            .update_password(user_id, &new_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update password hash", &e))?;

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

    pub fn generate_email_verification_token(&self) -> ApiResult<String> {
        let token = super::auth_generate_token(32);
        Ok(token)
    }
}
