use super::auth_generate_token;
use super::AuthService;
use std::sync::Arc;
use synapse_common::crypto::hash_password_with_params;
use synapse_common::password_hash_pool::{PasswordHashError, PasswordHashPool};
use synapse_common::*;
use synapse_storage::CreateAuditEventRequest;
use synapse_storage::User;
use uuid::Uuid;

impl AuthService {
    /// See [`login`].
    ///
    /// 账户级失败计数与锁定**不在这里**实现：唯一实现是
    /// `synapse-web/src/utils/login_lockout.rs`（总开关、IP allowlist、
    /// (ip, username) 与 per-IP 双桶、指数退避）。HTTP 登录入口
    /// (`synapse-web/src/routes/auth_compat.rs`) 在调用本方法前后完成
    /// `check_login_lockout` / `record_login_failure` / `clear_login_failures`。
    /// 直接调用本方法的非 HTTP 路径（内部任务、测试）因此不做锁定。
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
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        // Matrix spec (login.yaml) requires HTTP 403 + M_FORBIDDEN when the
        // login attempt fails (wrong password or unknown user).
        let invalid = || ApiError::invalid_credentials();

        let (password_hash_owned, user_for_success) = match user_opt.as_ref() {
            Some(u) if !u.is_deactivated => match u.password_hash.as_deref() {
                Some(h) => (h.to_string(), Some(u.clone())),
                None => (Self::dummy_password_hash().to_string(), None),
            },
            _ => (Self::dummy_password_hash().to_string(), None),
        };

        let password_ok = self.verify_user_password(password, &password_hash_owned).await?;

        let user = match user_for_success {
            Some(u) if password_ok => u,
            _ => {
                self.do_log_login_failure(username, "invalid_credentials").await;
                return Err(invalid());
            }
        };

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

    #[allow(
        clippy::expect_used,
        reason = "argon2 hashing with fixed params cannot fail for the constant dummy password"
    )]
    fn dummy_password_hash() -> &'static str {
        use std::sync::OnceLock;
        static DUMMY: OnceLock<String> = OnceLock::new();
        DUMMY
            .get_or_init(|| hash_password_with_params("no-such-user", 65536, 3, 1).expect("argon2 dummy hash"))
            .as_str()
    }

    /// See [`verify_user_password`].
    pub(crate) async fn verify_user_password(&self, password: &str, password_hash: &str) -> ApiResult<bool> {
        // P1-02: Argon2 verification runs through the bounded password-hash pool
        // so a burst of concurrent logins queues behind a CPU-count limit instead
        // of spawning an unbounded number of blocking computations that saturate
        // every core. Legacy (non-Argon2) hashes are cheap and keep the direct path.
        if password_hash.starts_with("$argon2") {
            let pool = PasswordHashPool::get_or_init_default();
            return match pool.verify_password_queued(password, password_hash).await {
                Ok(ok) => Ok(ok),
                Err(PasswordHashError::Timeout) => Err(ApiError::rate_limited_with_retry(1000)),
                Err(e) => Err(ApiError::internal_with_cause("Password verification failed", e)),
            };
        }

        let auth = Arc::new(self.clone());
        let password_str = password.to_string();
        let password_hash_str = password_hash.to_string();

        tokio::task::spawn_blocking(move || auth.verify_password(&password_str, &password_hash_str))
            .await
            .map_err(|e| ApiError::internal_with_cause("Verification task panicked", e))?
            .map_err(|e| ApiError::internal_with_cause("Password verification failed", e))
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
        Self::validate_device_display_name(initial_display_name)?;

        let device_id = match device_id {
            Some(d) => d.to_string(),
            _ => auth_generate_token(16),
        };

        if let Some(existing_device) = self
            .device_storage
            .get_device_by_id(&device_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?
        {
            if existing_device.user_id != user.user_id {
                return Err(ApiError::forbidden("Device ID already belongs to a different user".to_string()));
            }
        } else {
            self.device_storage
                .create_device(&device_id, &user.user_id, initial_display_name)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to create device", e))?;
        }

        Ok(device_id)
    }

    /// 校验 `initial_device_display_name` 的长度上限。
    ///
    /// 抽成独立函数是因为注册路径（PERF-02）要在**建用户之前**就失败，
    /// 避免为一个注定失败的请求创建账号。
    pub(crate) fn validate_device_display_name(name: Option<&str>) -> ApiResult<()> {
        if let Some(name) = name {
            if name.len() > 100 {
                return Err(ApiError::bad_request(
                    "initial_device_display_name must not exceed 100 characters".to_string(),
                ));
            }
        }
        Ok(())
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
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

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
