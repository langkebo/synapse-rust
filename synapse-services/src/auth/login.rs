use super::auth_generate_token;
use super::AuthService;
use chrono::Utc;
use std::sync::Arc;
use synapse_common::crypto::hash_password_with_params;
use synapse_common::*;
use synapse_storage::CreateAuditEventRequest;
use synapse_storage::User;
use uuid::Uuid;

impl AuthService {
    /// See [`login`].
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self.login_internal(username, password, device_id, initial_display_name).await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_login_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("auth_login_duration_seconds".to_string());
            hist.observe(duration);
        }

        if result.is_ok() {
            self.increment_counter("auth_login_success_total");
        } else {
            self.increment_counter("auth_login_failure_total");
        }
        result
    }

    async fn login_internal(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let user_opt = self
            .user_storage
            .get_user_by_identifier(username)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

        // P-007 fix: Matrix spec requires HTTP 401 + M_FORBIDDEN for invalid
        // credentials (wrong password or unknown user).
        let invalid = || ApiError::invalid_credentials();

        let (password_hash_owned, user_for_success) = match user_opt.as_ref() {
            Some(u) if !u.is_deactivated => match u.password_hash.as_deref() {
                Some(h) => (h.to_string(), Some(u.clone())),
                None => (Self::dummy_password_hash().to_string(), None),
            },
            _ => (Self::dummy_password_hash().to_string(), None),
        };

        let lock_user_id = user_opt.as_ref().map(|u| u.user_id.clone());
        let is_locked = match &lock_user_id {
            Some(uid) => self.is_account_locked(uid).await?,
            None => false,
        };

        let password_ok = self.verify_user_password(password, &password_hash_owned).await?;

        if is_locked {
            self.do_log_login_failure(username, "account_locked").await;
            return Err(ApiError::rate_limited(
                "Account is temporarily locked due to too many failed login attempts. Please try again later."
                    .to_string(),
            ));
        }

        let user = match user_for_success {
            Some(u) if password_ok => u,
            _ => {
                if let Some(uid) = lock_user_id.as_deref() {
                    self.record_login_failure(uid).await?;
                }
                self.do_log_login_failure(username, "invalid_credentials").await;
                return Err(invalid());
            }
        };

        self.clear_login_failures(&user.user_id).await?;

        if is_legacy_hash(&password_hash_owned) {
            if let Err(e) = self.migrate_password(&user.user_id, password).await {
                ::tracing::warn!(
                    target: "password_migration",
                    user_id = user.user_id,
                    error = %e,
                    "Failed to migrate legacy password hash"
                );
            }
        }

        let logout_marker = format!("user:logout_all:{}", user.user_id);
        self.cache.delete(&logout_marker).await;
        self.do_log_login_success(&user, device_id).await;

        let device_id = self.get_or_create_device_id(device_id, &user, initial_display_name).await?;

        let access_token = self.generate_access_token(&user.user_id, &device_id, user.is_admin).await?;
        // P2-12: link refresh_token to access_token so rotation can invalidate
        // the old access_token cache entry (Synapse v1.154 #19483).
        let refresh_token = self.generate_refresh_token(&user.user_id, &device_id, &access_token).await?;

        Ok((user, access_token, refresh_token, device_id))
    }

    #[allow(clippy::expect_used)]
    fn dummy_password_hash() -> &'static str {
        use std::sync::OnceLock;
        static DUMMY: OnceLock<String> = OnceLock::new();
        DUMMY
            .get_or_init(|| hash_password_with_params("no-such-user", 65536, 3, 1).expect("argon2 dummy hash"))
            .as_str()
    }

    async fn is_account_locked(&self, user_id: &str) -> ApiResult<bool> {
        let key = format!("auth:lockout:{user_id}");
        let lockout_until: Option<i64> = self.cache.get_checked(&key).await?;

        if let Some(timestamp) = lockout_until {
            if timestamp > Utc::now().timestamp() {
                return Ok(true);
            }
            self.cache.delete(&key).await;
        }
        Ok(false)
    }

    async fn record_login_failure(&self, user_id: &str) -> ApiResult<()> {
        let key = format!("auth:failures:{user_id}");
        let failures: i64 = self.cache.get(&key).await?.unwrap_or(0i64).saturating_add(1);

        if let Err(e) = self.cache.set(&key, &failures, self.login_lockout_duration_seconds).await {
            ::tracing::warn!(
                error = %e,
                user_id = %user_id,
                cache_key = %key,
                failure_count = failures,
                lockout_duration_seconds = self.login_lockout_duration_seconds,
                "Failed to update login failure count in cache"
            );
        }

        if failures >= self.login_failure_lockout_threshold as i64 {
            let lockout_until = Utc::now().timestamp() + self.login_lockout_duration_seconds as i64;
            let lockout_key = format!("auth:lockout:{user_id}");
            // 安全关键：锁定写入必须 fail-closed，Redis 故障时返回错误让登录失败，
            // 而非静默绕过账户锁定（审查 #13）。
            let lockout_str = lockout_until.to_string();
            self.cache.set_checked(&lockout_key, &lockout_str, self.login_lockout_duration_seconds).await?;

            ::tracing::warn!(
                target: "security_audit",
                event = "account_locked",
                user_id = user_id,
                failure_count = failures,
                lockout_duration_seconds = self.login_lockout_duration_seconds,
                "Account locked due to too many failed login attempts"
            );
            self.record_security_audit(
                "auth.account_locked",
                "user",
                user_id,
                "failure",
                serde_json::json!({
                    "failure_count": failures,
                    "lockout_duration_seconds": self.login_lockout_duration_seconds
                }),
            )
            .await;
        }

        Ok(())
    }

    async fn clear_login_failures(&self, user_id: &str) -> ApiResult<()> {
        let key = format!("auth:failures:{user_id}");
        self.cache.delete(&key).await;
        Ok(())
    }

    /// See [`verify_user_password`].
    pub(crate) async fn verify_user_password(&self, password: &str, password_hash: &str) -> ApiResult<bool> {
        let auth = Arc::new(self.clone());
        let password_str = password.to_string();
        let password_hash_str = password_hash.to_string();

        tokio::task::spawn_blocking(move || auth.verify_password(&password_str, &password_hash_str))
            .await
            .map_err(|e| ApiError::internal_with_context("Verification task panicked", &e))?
            .map_err(|e| ApiError::internal_with_context("Password verification failed", &e))
    }

    /// C1: Persist a security audit event to the tamper-evident audit_events table.
    /// Availability fail-soft: DB write failure does NOT block auth (fail-open), but
    /// DOES emit an error-level security_audit log so the gap surfaces in alerts.
    /// request_id is best-effort: D1 wires it as a field on the root http_request
    /// span, but reading from a Span without tracing_subscriber is not possible
    /// here, so we fall back to a generated uuid. Cross-table correlation by
    /// (user_id, time window) is the practical alternative.
    async fn record_security_audit(
        &self,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        result: &str,
        details: serde_json::Value,
    ) {
        // D1 sets request_id on the root span. We can't read it from synapse-services
        // (no tracing_subscriber dep), so leave it empty for now — production tracing
        // logs already carry the request_id field, which is the primary correlation path.
        let request_id = String::new();

        if let Some(storage) = &self.audit_storage {
            let req = CreateAuditEventRequest {
                actor_id: resource_id.to_string(),
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id: resource_id.to_string(),
                result: result.to_string(),
                request_id,
                details: Some(details),
            };
            if let Err(e) = storage.create_event(&Uuid::new_v4().to_string(), current_timestamp_millis(), &req).await {
                ::tracing::error!(
                    target: "security_audit",
                    event = action,
                    error = %e,
                    "FAILED to persist security audit event to audit_events (tamper-evident gap)"
                );
            }
        }
    }

    fn log_login_failure(username: &str, reason: &str) {
        ::tracing::warn!(
            target: "security_audit",
            event = "login_failure",
            username = username,
            reason = reason
        );
    }

    fn log_login_success(user: &User, device_id: Option<&str>) {
        ::tracing::info!(
            target: "security_audit",
            event = "login_success",
            user_id = user.user_id(),
            device_id = device_id
        );
    }

    async fn do_log_login_failure(&self, username: &str, reason: &str) {
        Self::log_login_failure(username, reason);
        self.record_security_audit(
            "auth.login_failed",
            "user",
            username,
            "failure",
            serde_json::json!({ "reason": reason }),
        )
        .await;
    }

    async fn do_log_login_success(&self, user: &User, device_id: Option<&str>) {
        Self::log_login_success(user, device_id);
        self.record_security_audit(
            "auth.login_success",
            "user",
            &user.user_id(),
            "success",
            serde_json::json!({ "device_id": device_id }),
        )
        .await;
    }

    /// See [`get_or_create_device_id`].
    pub(crate) async fn get_or_create_device_id(
        &self,
        device_id: Option<&str>,
        user: &User,
        initial_display_name: Option<&str>,
    ) -> ApiResult<String> {
        if let Some(name) = initial_display_name {
            if name.len() > 100 {
                return Err(ApiError::bad_request(
                    "initial_device_display_name must not exceed 100 characters".to_string(),
                ));
            }
        }

        let device_id = match device_id {
            Some(d) => d.to_string(),
            _ => auth_generate_token(16),
        };

        if let Some(existing_device) = self
            .device_storage
            .get_device_by_id(&device_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?
        {
            if existing_device.user_id != user.user_id {
                return Err(ApiError::forbidden("Device ID already belongs to a different user".to_string()));
            }
        } else {
            self.device_storage
                .create_device(&device_id, &user.user_id, initial_display_name)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to create device", &e))?;
        }

        Ok(device_id)
    }

    /// See [`increment_counter`].
    pub(crate) fn increment_counter(&self, name: &str) {
        if let Some(counter) = self.metrics.get_counter(name) {
            counter.inc();
        } else {
            let counter = self.metrics.register_counter(name.to_string());
            counter.inc();
        }
    }

    /// Verify a user's password without creating a new session or device.
    /// This is intended for UIA (User-Interactive Authentication) flows where
    /// password verification is needed without the side effects of `login()`.
    pub async fn verify_user_credentials(&self, user_id: &str, password: &str) -> ApiResult<()> {
        let user_opt = self
            .user_storage
            .get_user_by_identifier(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

        let user = user_opt.ok_or_else(ApiError::invalid_credentials)?;

        if user.is_deactivated {
            return Err(ApiError::invalid_credentials());
        }

        let password_hash = user.password_hash.ok_or_else(ApiError::invalid_credentials)?;

        let password_ok = self.verify_user_password(password, &password_hash).await?;

        if !password_ok {
            return Err(ApiError::invalid_credentials());
        }

        Ok(())
    }
}
