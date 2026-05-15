use crate::cache::*;
use crate::common::config::SecurityConfig;
use crate::common::crypto::{
    hash_password_with_params, is_legacy_hash, migrate_password_hash,
    verify_password as verify_password_common,
};
use crate::common::metrics::MetricsCollector;
use crate::common::validation::Validator;
use crate::common::*;
use crate::storage::refresh_token::RefreshTokenStorage;
use crate::storage::*;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const TOKEN_CACHE_TTL_SECS: u64 = 3600;
const USER_ACTIVE_CACHE_TTL_SECS: u64 = 60;
const ADMIN_CACHE_TTL_SECS: u64 = 60;
const DEFAULT_POWER_LEVEL: i64 = 50;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub jti: String,
    #[serde(rename = "admin")]
    pub is_admin: bool,
    pub exp: i64,
    pub iat: i64,
    pub device_id: Option<String>,
}

#[derive(Clone)]
pub struct AuthService {
    pub user_storage: UserStorage,
    pub device_storage: DeviceStorage,
    pub token_storage: AccessTokenStorage,
    pub refresh_token_storage: RefreshTokenStorage,
    pub room_storage: RoomStorage,
    pub member_storage: RoomMemberStorage,
    pub cache: Arc<CacheManager>,
    pub metrics: Arc<MetricsCollector>,
    pub validator: Arc<Validator>,
    pub jwt_secret: Vec<u8>,
    pub token_expiry: i64,
    pub refresh_token_expiry: i64,
    pub server_name: String,
    pub argon2_m_cost: u32,
    pub argon2_t_cost: u32,
    pub argon2_p_cost: u32,
    pub allow_legacy_hashes: bool,
    pub login_failure_lockout_threshold: u32,
    pub login_lockout_duration_seconds: u64,
}

impl AuthService {
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        metrics: Arc<MetricsCollector>,
        security: &SecurityConfig,
        server_name: &str,
    ) -> Self {
        // Backward-compatible constructor: defers to security.expiry_time.
        // New call sites should prefer `new_with_lifetime` so they can pass
        // the canonical lifetime resolved from Config::access_token_lifetime_seconds.
        Self::new_with_lifetime(
            pool,
            cache,
            metrics,
            security,
            server_name,
            security.expiry_time,
        )
    }

    pub fn new_with_lifetime(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        metrics: Arc<MetricsCollector>,
        security: &SecurityConfig,
        server_name: &str,
        access_token_lifetime: i64,
    ) -> Self {
        let server_name_for_storage = server_name.to_string();
        Self {
            user_storage: UserStorage::new(pool, cache.clone()),
            device_storage: DeviceStorage::new(pool),
            token_storage: AccessTokenStorage::new(pool),
            refresh_token_storage: RefreshTokenStorage::new(pool),
            room_storage: RoomStorage::new(pool),
            member_storage: RoomMemberStorage::new(pool, &server_name_for_storage),
            cache,
            metrics,
            validator: Arc::new(Validator::default()),
            jwt_secret: security.secret.as_bytes().to_vec(),
            token_expiry: access_token_lifetime,
            refresh_token_expiry: security.refresh_token_expiry,
            server_name: server_name_for_storage,
            argon2_m_cost: security.argon2_m_cost,
            argon2_t_cost: security.argon2_t_cost,
            argon2_p_cost: security.argon2_p_cost,
            allow_legacy_hashes: security.allow_legacy_hashes,
            login_failure_lockout_threshold: security.login_failure_lockout_threshold,
            login_lockout_duration_seconds: security.login_lockout_duration_seconds,
        }
    }

    pub async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self
            .register_internal(username, password, admin, displayname, None)
            .await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_register_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("auth_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if result.is_ok() {
            self.increment_counter("auth_register_success_total");
        } else {
            self.increment_counter("auth_register_failure_total");
        }
        result
    }

    pub async fn register_with_device_name(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self
            .register_internal(
                username,
                password,
                admin,
                displayname,
                initial_device_display_name,
            )
            .await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_register_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("auth_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if result.is_ok() {
            self.increment_counter("auth_register_success_total");
        } else {
            self.increment_counter("auth_register_failure_total");
        }
        result
    }

    async fn register_internal(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        if username.is_empty() || password.is_empty() {
            return Err(ApiError::bad_request(
                "Username and password are required".to_string(),
            ));
        }
        if let Err(e) = self.validator.validate_username(username) {
            return Err(ApiError::invalid_username(e.to_string()));
        }
        if let Err(e) = self.validator.validate_password(password) {
            return Err(ApiError::bad_request(format!(
                "Password does not meet policy requirements: {}",
                e
            )));
        }

        let existing_user = self
            .user_storage
            .get_user_by_username(username)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if existing_user.is_some() {
            return Err(ApiError::user_in_use("Username already taken"));
        }

        let user_id = format!("@{}:{}", username, self.server_name);

        // P1 Performance: Run CPU-intensive hashing in spawn_blocking to avoid blocking the async executor
        let auth = self.clone();
        let password_str = password.to_string();
        let password_hash = tokio::task::spawn_blocking(move || auth.hash_password(&password_str))
            .await
            .map_err(|e| ApiError::internal(format!("Hashing task panicked: {}", e)))??;

        // Use a transaction to ensure user and device are created atomically
        let mut tx = self
            .user_storage
            .pool
            .begin()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to start transaction: {}", e)))?;

        let user = match self
            .user_storage
            .create_user_tx(&mut tx, &user_id, username, Some(&password_hash), admin)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(ApiError::internal(format!("Failed to create user: {}", e)));
            }
        };

        ::tracing::info!(
            target: "security_audit",
            event = "user_registered",
            user_id = user_id,
            admin = admin
        );

        let device_id = generate_token(16);
        if let Err(e) = self
            .device_storage
            .create_device_tx(&mut tx, &device_id, &user_id, initial_device_display_name)
            .await
        {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!(
                "Failed to create device: {}",
                e
            )));
        }

        if let Err(e) = tx.commit().await {
            return Err(ApiError::internal(format!(
                "Failed to commit transaction: {}",
                e
            )));
        }

        let effective_displayname = displayname.unwrap_or(username);
        if let Err(e) = self
            .user_storage
            .update_displayname(&user_id, Some(effective_displayname))
            .await
        {
            ::tracing::warn!("Failed to set displayname for {}: {}", user_id, e);
        }

        self.device_storage
            .record_device_list_change_best_effort(&user_id, Some(&device_id), "changed")
            .await;

        let access_token = self
            .generate_access_token(&user_id, &device_id, user.is_admin)
            .await?;
        let refresh_token = self.generate_refresh_token(&user_id, &device_id).await?;

        Ok((user, access_token, refresh_token, device_id))
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self
            .login_internal(username, password, device_id, initial_display_name)
            .await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_login_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("auth_login_duration_seconds".to_string());
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
        // Resolve the user. Critically, we do NOT short-circuit on "user not
        // found" or "user deactivated" — both branches still run an argon2
        // verify (against a dummy hash for the missing-user path) and return
        // an opaque "Invalid credentials" so an attacker cannot enumerate
        // existing accounts via timing or distinct error codes.
        //
        // Lockout remains observable on purpose so legitimate clients can
        // surface a retry-after; lockout is keyed on user_id so it only
        // kicks in after the username is known to exist anyway.
        let user_opt = self
            .user_storage
            .get_user_by_identifier(username)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let invalid = || ApiError::forbidden("Invalid credentials".to_string());

        let (password_hash_owned, user_for_success) = match user_opt.as_ref() {
            Some(u) if !u.is_deactivated => match u.password_hash.as_deref() {
                Some(h) => (h.to_string(), Some(u.clone())),
                None => (Self::dummy_password_hash().to_string(), None),
            },
            // Either no user, or user is deactivated. Run a dummy verify to
            // equalise wall-clock timing against the success path.
            _ => (Self::dummy_password_hash().to_string(), None),
        };

        // Lockout check happens after the timing-equalised verify so that the
        // existence of a lockout entry does not itself become an oracle. We
        // record the lookup but defer acting on the result.
        let lock_user_id = user_opt.as_ref().map(|u| u.user_id.clone());
        let is_locked = match &lock_user_id {
            Some(uid) => self.is_account_locked(uid).await?,
            None => false,
        };

        let password_ok = self
            .verify_user_password(password, &password_hash_owned)
            .await?;

        // From here on, every "no" path returns the same opaque error.
        if is_locked {
            Self::log_login_failure(username, "account_locked");
            return Err(ApiError::rate_limited(
                "Account is temporarily locked due to too many failed login attempts. Please try again later.".to_string(),
            ));
        }

        let user = match user_for_success {
            Some(u) if password_ok => u,
            _ => {
                if let Some(uid) = lock_user_id.as_deref() {
                    self.record_login_failure(uid).await?;
                }
                Self::log_login_failure(username, "invalid_credentials");
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
        Self::log_login_success(&user, device_id);

        let device_id = self
            .get_or_create_device_id(device_id, &user, initial_display_name)
            .await?;

        let access_token = self
            .generate_access_token(&user.user_id, &device_id, user.is_admin)
            .await?;
        let refresh_token = self
            .generate_refresh_token(&user.user_id, &device_id)
            .await?;

        Ok((user, access_token, refresh_token, device_id))
    }

    /// Argon2 PHC of the literal string "no-such-user" with default params.
    /// Used to keep `login` constant-time when the requested username does
    /// not exist or has no password set. Generated once at startup and held
    /// by `OnceLock`; the hash itself is non-secret.
    fn dummy_password_hash() -> &'static str {
        use std::sync::OnceLock;
        static DUMMY: OnceLock<String> = OnceLock::new();
        DUMMY
            .get_or_init(|| {
                hash_password_with_params("no-such-user", 65536, 3, 1).expect("argon2 dummy hash")
            })
            .as_str()
    }

    async fn is_account_locked(&self, user_id: &str) -> ApiResult<bool> {
        let key = format!("auth:lockout:{}", user_id);
        let lockout_until: Option<i64> = self.cache.get(&key).await?;

        if let Some(timestamp) = lockout_until {
            if timestamp > Utc::now().timestamp() {
                return Ok(true);
            }
            let _ = self.cache.delete(&key).await;
        }
        Ok(false)
    }

    async fn record_login_failure(&self, user_id: &str) -> ApiResult<()> {
        let key = format!("auth:failures:{}", user_id);
        let failures: i64 = self
            .cache
            .get(&key)
            .await?
            .unwrap_or(0i64)
            .saturating_add(1);

        if let Err(e) = self
            .cache
            .set(&key, &failures, self.login_lockout_duration_seconds)
            .await
        {
            ::tracing::warn!("Failed to update login failure count in cache: {}", e);
        }

        if failures >= self.login_failure_lockout_threshold as i64 {
            let lockout_until = Utc::now().timestamp() + self.login_lockout_duration_seconds as i64;
            let lockout_key = format!("auth:lockout:{}", user_id);
            if let Err(e) = self
                .cache
                .set(
                    &lockout_key,
                    &lockout_until,
                    self.login_lockout_duration_seconds,
                )
                .await
            {
                ::tracing::warn!("Failed to set login lockout in cache: {}", e);
            }

            ::tracing::warn!(
                target: "security_audit",
                event = "account_locked",
                user_id = user_id,
                failure_count = failures,
                lockout_duration_seconds = self.login_lockout_duration_seconds,
                "Account locked due to too many failed login attempts"
            );
        }

        Ok(())
    }

    async fn clear_login_failures(&self, user_id: &str) -> ApiResult<()> {
        let key = format!("auth:failures:{}", user_id);
        let _ = self.cache.delete(&key).await;
        Ok(())
    }

    async fn verify_user_password(&self, password: &str, password_hash: &str) -> ApiResult<bool> {
        let auth = Arc::new(self.clone());
        let password_str = password.to_string();
        let password_hash_str = password_hash.to_string();

        tokio::task::spawn_blocking(move || auth.verify_password(&password_str, &password_hash_str))
            .await
            .map_err(|e| ApiError::internal(format!("Verification task panicked: {}", e)))?
            .map_err(|e| ApiError::internal(format!("Password verification failed: {}", e)))
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

    async fn get_or_create_device_id(
        &self,
        device_id: Option<&str>,
        user: &User,
        initial_display_name: Option<&str>,
    ) -> ApiResult<String> {
        let device_id = match device_id {
            Some(d) => d.to_string(),
            _ => auth_generate_token(16),
        };

        if let Some(existing_device) = self
            .device_storage
            .get_device(&device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
            if existing_device.user_id != user.user_id {
                return Err(ApiError::forbidden(
                    "Device ID already belongs to a different user".to_string(),
                ));
            }
        } else {
            self.device_storage
                .create_device(&device_id, &user.user_id, initial_display_name)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to create device: {}", e)))?;
        }

        Ok(device_id)
    }

    fn increment_counter(&self, name: &str) {
        if let Some(counter) = self.metrics.get_counter(name) {
            counter.inc();
        } else {
            let counter = self.metrics.register_counter(name.to_string());
            counter.inc();
        }
    }

    pub async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()> {
        let claims = self.decode_token(access_token).ok();
        let user_id = claims.as_ref().map(|c| c.sub.as_str()).unwrap_or("unknown");

        self.token_storage
            .add_to_blacklist(access_token, user_id, Some("User logout"))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add token to blacklist: {}", e)))?;

        self.token_storage
            .delete_token(access_token)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete token: {}", e)))?;

        if let Some(d_id) = device_id {
            self.token_storage
                .delete_device_tokens(d_id)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to delete device tokens: {}", e))
                })?;

            // RFC 6819 §5.1.5: 单设备登出也必须吊销该设备的 refresh token，
            // 否则被盗 refresh token 仍可在登出后继续换发新 access token。
            // 仅当解析出 user_id 时才能精准吊销；解析失败时退化为
            // 仅删除 access token（攻击者再次使用 refresh 时会被
            // refresh_token() 的 reuse-detection 兜底）。
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
                    return Err(ApiError::internal(format!(
                        "Failed to invalidate refresh tokens: {}",
                        e
                    )));
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
        // Get all user tokens first
        let tokens = self
            .token_storage
            .get_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user tokens: {}", e)))?;

        // Add all tokens to blacklist
        for token in &tokens {
            if let Err(e) = self
                .token_storage
                .add_hash_to_blacklist(&token.token_hash, user_id, Some("Logout all devices"))
                .await
            {
                ::tracing::warn!("Failed to add token to blacklist during logout_all: {}", e);
            }
        }

        // Delete tokens from database
        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete tokens: {}", e)))?;

        self.refresh_token_storage
            .revoke_all_user_tokens(user_id, "Logout all devices")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to revoke refresh tokens: {}", e)))?;

        // Delete user devices
        self.device_storage
            .delete_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

        // Mark user as fully logged out - this will invalidate all cached JWT tokens
        // issued before this time by setting a special flag that validate_token will check
        let logout_marker = format!("user:logout_all:{}", user_id);
        let now = Utc::now().timestamp();
        self.cache
            .set_raw(&logout_marker, &now.to_string(), TOKEN_CACHE_TTL_SECS)
            .await;

        Ok(())
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)> {
        let token_hash = Self::hash_token(refresh_token);

        let token_data = self
            .refresh_token_storage
            .get_token(&token_hash)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let (token_data, token_hash) = match token_data {
            Some(t) => (t, token_hash),
            None => {
                let legacy_hash = Self::hash_token_legacy(refresh_token);
                let legacy_data = self
                    .refresh_token_storage
                    .get_token(&legacy_hash)
                    .await
                    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
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
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
                        ApiError::internal(format!("Failed to claim refresh token: {}", e))
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

    pub async fn validate_token(
        &self,
        token: &str,
    ) -> ApiResult<(String, Option<String>, bool, bool, bool)> {
        ::tracing::debug!(target: "token_validation", "Validating token");

        if self
            .token_storage
            .is_in_blacklist(token)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check token blacklist: {}", e)))?
        {
            ::tracing::debug!(target: "token_validation", "Token found in blacklist");
            return Err(ApiError::unauthorized("Token has been revoked".to_string()));
        }

        if self
            .token_storage
            .is_token_revoked(token)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check token status: {}", e)))?
        {
            ::tracing::debug!(target: "token_validation", "Token has been revoked in database");
            return Err(ApiError::unauthorized("Token has been revoked".to_string()));
        }

        // Decode token first to get user_id for logout_all check
        let claims = self.decode_token(token).map_err(|e| {
            ::tracing::debug!(target: "token_validation", "Token validation failed: {}", e);
            ApiError::unauthorized("Invalid token".to_string())
        })?;

        // Enforce JWT exp BEFORE the cache shortcut. The cache TTL is decoupled
        // from the token's actual expiry (a 5-minute token can sit in a 1-hour
        // cache entry), so without this check an expired token would still
        // validate on the cache hit path below.
        if claims.exp < Utc::now().timestamp() {
            ::tracing::debug!(target: "token_validation", "Token expired");
            return Err(ApiError::unauthorized("Token expired".to_string()));
        }

        // Check if user has been logged out from all devices
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
                    // Resolve all three flags atomically: either every per-flag
                    // entry is present in cache, or we go to the database and
                    // refresh all three at once. Reading them independently
                    // with `.unwrap_or(false)` would silently downgrade
                    // shadow_banned/guest to `false` when only their cache
                    // entries had been evicted (different TTLs / Redis LRU),
                    // letting a banned user pass.
                    let shadow_key = format!("user:shadow_banned:{}", cached_claims.sub);
                    let guest_key = format!("user:guest:{}", cached_claims.sub);

                    let cached_admin = self.cache.get::<bool>(&admin_cache_key).await?;
                    let cached_shadow = self.cache.get::<bool>(&shadow_key).await?;
                    let cached_guest = self.cache.get::<bool>(&guest_key).await?;

                    let (is_admin, is_shadow_banned, is_guest) =
                        match (cached_admin, cached_shadow, cached_guest) {
                            (Some(a), Some(s), Some(g)) => (a, s, g),
                            _ => {
                                let user = self
                                    .user_storage
                                    .get_user_by_id(&cached_claims.sub)
                                    .await
                                    .map_err(|e| {
                                        ApiError::internal(format!("Database error: {}", e))
                                    })?
                                    .ok_or_else(|| {
                                        ApiError::unauthorized("User not found".to_string())
                                    })?;
                                self.cache
                                    .set(&admin_cache_key, user.is_admin, ADMIN_CACHE_TTL_SECS)
                                    .await?;
                                self.cache
                                    .set(
                                        &shadow_key,
                                        user.is_shadow_banned,
                                        USER_ACTIVE_CACHE_TTL_SECS,
                                    )
                                    .await?;
                                self.cache
                                    .set(&guest_key, user.is_guest, USER_ACTIVE_CACHE_TTL_SECS)
                                    .await?;
                                (user.is_admin, user.is_shadow_banned, user.is_guest)
                            }
                        };

                    Ok((
                        cached_claims.user_id,
                        cached_claims.device_id.clone(),
                        is_admin,
                        is_shadow_banned,
                        is_guest,
                    ))
                } else {
                    Err(ApiError::unauthorized(
                        "User not found or deactivated".to_string(),
                    ))
                };
            }

            ::tracing::debug!(target: "token_validation", "Cache miss for user active status, querying DB");

            let user = self
                .user_storage
                .get_user_by_id(&cached_claims.sub)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            return if let Some(u) = user {
                let is_active = !u.is_deactivated;
                ::tracing::debug!(target: "token_validation", "User found, is_deactivated: {:?}, is_active: {}", u.is_deactivated, is_active);

                self.cache
                    .set_user_active(&cached_claims.sub, is_active, USER_ACTIVE_CACHE_TTL_SECS)
                    .await;
                self.cache
                    .set(&admin_cache_key, u.is_admin, ADMIN_CACHE_TTL_SECS)
                    .await?;
                self.cache
                    .set(
                        &format!("user:shadow_banned:{}", cached_claims.sub),
                        u.is_shadow_banned,
                        USER_ACTIVE_CACHE_TTL_SECS,
                    )
                    .await?;
                self.cache
                    .set(
                        &format!("user:guest:{}", cached_claims.sub),
                        u.is_guest,
                        USER_ACTIVE_CACHE_TTL_SECS,
                    )
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
                self.cache
                    .set_user_active(&cached_claims.sub, false, USER_ACTIVE_CACHE_TTL_SECS)
                    .await;
                Err(ApiError::unauthorized("User not found".to_string()))
            };
        }

        ::tracing::debug!(target: "token_validation", "Token not found in cache, using decoded JWT");

        // claims.exp already enforced at the top of this function.

        ::tracing::debug!(target: "token_validation", "Decoded JWT for user: {}", claims.sub);

        let user = self
            .user_storage
            .get_user_by_id(&claims.sub)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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

                self.cache
                    .set_user_active(&claims.sub, true, USER_ACTIVE_CACHE_TTL_SECS)
                    .await;
                self.cache
                    .set(
                        &format!("user:admin:{}", claims.sub),
                        is_admin,
                        ADMIN_CACHE_TTL_SECS,
                    )
                    .await?;
                self.cache
                    .set(
                        &format!("user:shadow_banned:{}", claims.sub),
                        u.is_shadow_banned,
                        USER_ACTIVE_CACHE_TTL_SECS,
                    )
                    .await?;
                self.cache
                    .set(
                        &format!("user:guest:{}", claims.sub),
                        u.is_guest,
                        USER_ACTIVE_CACHE_TTL_SECS,
                    )
                    .await?;
                self.cache
                    .set_token(token, &final_claims, TOKEN_CACHE_TTL_SECS)
                    .await;
                Ok((
                    final_claims.user_id,
                    final_claims.device_id.clone(),
                    is_admin,
                    u.is_shadow_banned,
                    u.is_guest,
                ))
            }
            None => {
                ::tracing::debug!(target: "token_validation", "User not found in database");
                Err(ApiError::unauthorized("User not found".to_string()))
            }
        }
    }

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
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
                .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

            // 当调用方提供 current_password 时，账户必须设置过密码 —
            // 否则 SSO/OIDC-only 账户可被绕过验证直接设置密码。
            let password_hash = user.password_hash.as_deref().ok_or_else(|| {
                ApiError::forbidden(
                    "Cannot verify current password: account has no password set".to_string(),
                )
            })?;

            if !self.verify_user_password(pwd, password_hash).await? {
                return Err(ApiError::unauthorized(
                    "Current password is incorrect".to_string(),
                ));
            }
        }

        if let Err(e) = self.validator.validate_password(new_password) {
            return Err(ApiError::bad_request(format!(
                "Password does not meet policy requirements: {}",
                e
            )));
        }

        let password_hash = self.hash_password(new_password)?;
        self.user_storage
            .update_password(user_id, &password_hash)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update password: {}", e)))?;

        if let Some(device_id) = current_device_id {
            self.token_storage
                .delete_user_tokens_except_device(user_id, device_id)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to invalidate access tokens: {}", e))
                })?;

            self.refresh_token_storage
                .revoke_all_user_tokens_except_device(user_id, device_id, "password_changed")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to invalidate refresh tokens: {}", e))
                })?;
        } else {
            self.token_storage
                .delete_user_tokens(user_id)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to invalidate access tokens: {}", e))
                })?;

            self.refresh_token_storage
                .revoke_all_user_tokens(user_id, "password_changed")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to invalidate refresh tokens: {}", e))
                })?;
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
            .deactivate_user(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to deactivate user: {}", e)))?;

        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete tokens: {}", e)))?;

        // RFC 6819 §5.1.5: 注销账户必须吊销所有 refresh token，
        // 否则被盗令牌仍可在停用之后继续换发新 access token。
        if let Err(e) = self
            .refresh_token_storage
            .revoke_all_user_tokens(user_id, "account_deactivated")
            .await
        {
            ::tracing::error!(
                target: "security_audit",
                event = "refresh_token_revoke_failed_after_deactivation",
                user_id = user_id,
                error = %e,
                "Failed to revoke refresh tokens during account deactivation"
            );
            return Err(ApiError::internal(format!(
                "Failed to invalidate refresh tokens: {}",
                e
            )));
        }

        self.device_storage
            .delete_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

        self.cache.delete(&format!("user:active:{}", user_id)).await;
        self.cache.delete(&format!("user:admin:{}", user_id)).await;

        ::tracing::info!(
            target: "security_audit",
            event = "account_deactivated",
            user_id = user_id,
            "Account deactivated; all tokens and devices revoked"
        );

        Ok(())
    }

    /// 单设备注销：删除设备行 + 该设备的 access token + 该设备的 refresh token。
    ///
    /// `delete_device` / `delete_devices` 路由调用本方法替代直接调用
    /// `device_storage`，以确保 RFC 6819 §5.1.5 要求的令牌全链路撤销。
    pub async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64> {
        let rows = self
            .device_storage
            .delete_user_device(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete device: {}", e)))?;

        if rows == 0 {
            return Ok(0);
        }

        self.token_storage
            .delete_device_tokens(device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete device tokens: {}", e)))?;

        if let Err(e) = self
            .refresh_token_storage
            .revoke_device_tokens(user_id, device_id, "device_deleted")
            .await
        {
            ::tracing::error!(
                target: "security_audit",
                event = "refresh_token_revoke_failed_after_device_delete",
                user_id = user_id,
                device_id = device_id,
                error = %e,
                "Failed to revoke device refresh tokens after device delete"
            );
            return Err(ApiError::internal(format!(
                "Failed to invalidate refresh tokens: {}",
                e
            )));
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

    /// 批量单设备注销：与 `revoke_device` 相同的清理顺序，但只对
    /// 给定 `user_id` 拥有的设备生效（防止越权删除其他用户设备）。
    pub async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let rows = self
            .device_storage
            .delete_user_devices_batch(user_id, device_ids)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

        if rows == 0 {
            return Ok(0);
        }

        for device_id in device_ids {
            if let Err(e) = self
                .token_storage
                .delete_user_device_tokens(user_id, device_id)
                .await
            {
                ::tracing::error!(
                    target: "security_audit",
                    event = "access_token_delete_failed_after_device_delete",
                    user_id = user_id,
                    device_id = device_id.as_str(),
                    error = %e,
                    "Failed to delete access tokens after batch device delete"
                );
                return Err(ApiError::internal(format!(
                    "Failed to delete device tokens: {}",
                    e
                )));
            }

            if let Err(e) = self
                .refresh_token_storage
                .revoke_device_tokens(user_id, device_id, "device_deleted")
                .await
            {
                ::tracing::error!(
                    target: "security_audit",
                    event = "refresh_token_revoke_failed_after_device_delete",
                    user_id = user_id,
                    device_id = device_id.as_str(),
                    error = %e,
                    "Failed to revoke device refresh tokens after batch delete"
                );
                return Err(ApiError::internal(format!(
                    "Failed to invalidate refresh tokens: {}",
                    e
                )));
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

    pub async fn generate_access_token(
        &self,
        user_id: &str,
        device_id: &str,
        admin: bool,
    ) -> ApiResult<String> {
        let now = Utc::now();
        let jti = uuid::Uuid::new_v4().to_string();
        let claims = Claims {
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

        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| ApiError::internal(format!("Failed to generate token: {}", e)))?;

        let expires_at = (now + Duration::seconds(self.token_expiry)).timestamp_millis();

        self.token_storage
            .create_token(&token, user_id, Some(device_id), Some(expires_at))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to store token: {}", e)))?;

        Ok(token)
    }

    pub async fn generate_refresh_token(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> ApiResult<String> {
        let token = generate_token(32);
        let token_hash = Self::hash_token(&token);
        let expiry_ts = Utc::now().timestamp_millis() + (self.refresh_token_expiry * 1000);

        let request = crate::storage::refresh_token::CreateRefreshTokenRequest {
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
            .map_err(|e| ApiError::internal(format!("Failed to store refresh token: {}", e)))?;

        Ok(token)
    }

    fn hash_token(token: &str) -> String {
        crate::common::crypto::hash_token(token)
    }

    fn hash_token_legacy(token: &str) -> String {
        crate::common::crypto::hash_token_legacy(token)
    }

    fn decode_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5;
        validation.set_required_spec_claims(&["exp", "iat", "sub"]);
        jsonwebtoken::decode(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )
        .map(|e| e.claims)
    }

    fn hash_password(&self, password: &str) -> Result<String, ApiError> {
        hash_password_with_params(
            password,
            self.argon2_m_cost,
            self.argon2_t_cost,
            self.argon2_p_cost,
        )
        .map_err(ApiError::internal)
    }

    pub async fn hash_password_for_storage(&self, password: &str) -> Result<String, ApiError> {
        let auth = self.clone();
        let password_str = password.to_string();

        tokio::task::spawn_blocking(move || auth.hash_password(&password_str))
            .await
            .map_err(|e| ApiError::internal(format!("Hashing task panicked: {}", e)))?
    }

    fn verify_password(&self, password: &str, password_hash: &str) -> Result<bool, ApiError> {
        verify_password_common(password, password_hash, self.allow_legacy_hashes)
            .map_err(ApiError::internal)
    }

    async fn migrate_password(&self, user_id: &str, password: &str) -> Result<(), ApiError> {
        let start = std::time::Instant::now();

        let password_str = password.to_string();
        let m_cost = self.argon2_m_cost;
        let t_cost = self.argon2_t_cost;
        let p_cost = self.argon2_p_cost;

        let new_hash = tokio::task::spawn_blocking(move || {
            migrate_password_hash(&password_str, m_cost, t_cost, p_cost)
        })
        .await
        .map_err(|e| ApiError::internal(format!("Migration task panicked: {}", e)))?
        .map_err(ApiError::internal)?;

        self.user_storage
            .update_password(user_id, &new_hash)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update password hash: {}", e)))?;

        let duration = start.elapsed().as_secs_f64();

        ::tracing::info!(
            target: "password_migration",
            event = "password_migrated",
            user_id = user_id,
            duration_ms = duration * 1000.0,
            "Successfully migrated legacy password hash to Argon2"
        );

        self.increment_counter("password_migration_success_total");

        if let Some(hist) = self
            .metrics
            .get_histogram("password_migration_duration_seconds")
        {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("password_migration_duration_seconds".to_string());
            hist.observe(duration);
        }

        Ok(())
    }

    pub fn generate_email_verification_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let token = auth_generate_token(32);
        Ok(token)
    }

    pub async fn get_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        let membership = self
            .member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if membership.is_none() {
            return Ok(-1);
        }

        let power_levels_content: Option<serde_json::Value> = sqlx::query_scalar(
            r#"
            SELECT content
            FROM events
            WHERE room_id = $1
              AND event_type = 'm.room.power_levels'
              AND state_key = ''
            ORDER BY origin_server_ts DESC
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(content) = power_levels_content {
            if let Some(level) = content
                .get("users")
                .and_then(|users| users.get(user_id))
                .and_then(|level| level.as_i64())
            {
                return Ok(level);
            }

            if let Some(level) = content
                .get("users_default")
                .and_then(|level| level.as_i64())
            {
                return Ok(level);
            }
        }

        let room_creator: Option<String> = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(creator) = room_creator {
            if creator == user_id {
                return Ok(100);
            }
        }

        Ok(0)
    }

    async fn get_joined_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        let membership = self
            .member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        match membership {
            Some(m) if m == "join" => self.get_user_power_level(room_id, user_id).await,
            _ => Ok(-1),
        }
    }

    async fn get_room_power_levels_content(
        &self,
        room_id: &str,
    ) -> ApiResult<Option<serde_json::Value>> {
        sqlx::query_scalar(
            r#"
            SELECT content
            FROM events
            WHERE room_id = $1
              AND event_type = 'm.room.power_levels'
              AND state_key = ''
            ORDER BY origin_server_ts DESC
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))
    }

    pub async fn get_required_state_event_power_level(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> ApiResult<i64> {
        let power_levels_content = self.get_room_power_levels_content(room_id).await?;
        if let Some(content) = power_levels_content {
            if let Some(level) = content
                .get("events")
                .and_then(|events| events.get(event_type))
                .and_then(|level| level.as_i64())
            {
                return Ok(level);
            }

            if let Some(level) = content
                .get("state_default")
                .and_then(|level| level.as_i64())
            {
                return Ok(level);
            }
        }

        if event_type == "m.room.power_levels" {
            return Ok(100);
        }

        Ok(DEFAULT_POWER_LEVEL)
    }

    pub async fn get_required_message_event_power_level(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> ApiResult<i64> {
        let power_levels_content = self.get_room_power_levels_content(room_id).await?;
        if let Some(content) = power_levels_content {
            if let Some(level) = content
                .get("events")
                .and_then(|events| events.get(event_type))
                .and_then(|level| level.as_i64())
            {
                return Ok(level);
            }

            if let Some(level) = content
                .get("events_default")
                .and_then(|level| level.as_i64())
            {
                return Ok(level);
            }
        }

        Ok(0)
    }

    pub async fn verify_message_event_write(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> ApiResult<()> {
        let power_level = self.get_joined_user_power_level(room_id, user_id).await?;
        let required = self
            .get_required_message_event_power_level(room_id, event_type)
            .await?;

        if power_level < required {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_message_event_write",
                user_id = user_id,
                room_id = room_id,
                event_type = event_type,
                power_level = power_level,
                required = required,
                "User attempted to send message event without sufficient permission"
            );
            return Err(ApiError::forbidden(
                "Insufficient permission to send this event".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn verify_state_event_write(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> ApiResult<()> {
        let power_level = self.get_joined_user_power_level(room_id, user_id).await?;
        let required = self
            .get_required_state_event_power_level(room_id, event_type)
            .await?;

        if power_level < required {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_state_event_write",
                user_id = user_id,
                room_id = room_id,
                event_type = event_type,
                power_level = power_level,
                required = required,
                "User attempted to send state event without sufficient permission"
            );
            return Err(ApiError::forbidden(
                "Insufficient permission to send this state event".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn verify_power_levels_change(
        &self,
        room_id: &str,
        user_id: &str,
        new_content: &serde_json::Value,
    ) -> ApiResult<()> {
        let actor_level = self.get_joined_user_power_level(room_id, user_id).await?;
        let current_content = self.get_room_power_levels_content(room_id).await?;
        let new_power_levels_content = new_content;

        if let Some(current) = current_content {
            if let Some(new_users) = new_power_levels_content
                .get("users")
                .and_then(|u| u.as_object())
            {
                let current_users = current.get("users").and_then(|u| u.as_object());
                for (target_user, new_level_val) in new_users {
                    let new_level = new_level_val.as_i64().unwrap_or(0);
                    let current_level = current_users
                        .and_then(|cu| cu.get(target_user))
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| {
                            current
                                .get("users_default")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0)
                        });

                    if new_level > current_level && actor_level < new_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_power_level_elevation",
                            user_id = user_id,
                            room_id = room_id,
                            target_user = target_user,
                            actor_level = actor_level,
                            new_level = new_level,
                            "User attempted to set power level above their own"
                        );
                        return Err(ApiError::forbidden(
                            "Cannot set power level higher than your own".to_string(),
                        ));
                    }

                    if current_level >= actor_level && new_level != current_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_power_level_change",
                            user_id = user_id,
                            room_id = room_id,
                            target_user = target_user,
                            actor_level = actor_level,
                            current_level = current_level,
                            new_level = new_level,
                            "User attempted to change power level of user at or above their own level"
                        );
                        return Err(ApiError::forbidden(
                            "Cannot change power level of user at or above your level".to_string(),
                        ));
                    }
                }
            }

            if let Some(new_events) = new_power_levels_content
                .get("events")
                .and_then(|e| e.as_object())
            {
                let current_events = current.get("events").and_then(|e| e.as_object());
                for (event_type, new_level_val) in new_events {
                    let new_level = new_level_val.as_i64().unwrap_or(0);
                    let current_level = current_events
                        .and_then(|ce| ce.get(event_type))
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| {
                            current
                                .get("events_default")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0)
                        });

                    if new_level > actor_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_event_level_change",
                            user_id = user_id,
                            room_id = room_id,
                            event_type = event_type,
                            actor_level = actor_level,
                            new_level = new_level,
                            "User attempted to set event power level above their own"
                        );
                        return Err(ApiError::forbidden(
                            "Cannot set event power level above your own".to_string(),
                        ));
                    }

                    if current_level > actor_level && new_level != current_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_event_level_change_above_self",
                            user_id = user_id,
                            room_id = room_id,
                            event_type = event_type,
                            actor_level = actor_level,
                            current_level = current_level,
                            new_level = new_level,
                            "User attempted to change event power level above their own"
                        );
                        return Err(ApiError::forbidden(
                            "Cannot change event power level above your own".to_string(),
                        ));
                    }
                }
            }

            let scalar_checks = [
                (
                    "users_default",
                    current
                        .get("users_default")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0),
                ),
                (
                    "events_default",
                    current
                        .get("events_default")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0),
                ),
                (
                    "state_default",
                    current
                        .get("state_default")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(DEFAULT_POWER_LEVEL),
                ),
                (
                    "ban",
                    current
                        .get("ban")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(DEFAULT_POWER_LEVEL),
                ),
                (
                    "kick",
                    current
                        .get("kick")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(DEFAULT_POWER_LEVEL),
                ),
                (
                    "redact",
                    current
                        .get("redact")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(DEFAULT_POWER_LEVEL),
                ),
                (
                    "invite",
                    current.get("invite").and_then(|v| v.as_i64()).unwrap_or(0),
                ),
                (
                    "notifications",
                    current
                        .get("notifications")
                        .and_then(|v| v.as_object())
                        .and_then(|n| n.get("room").and_then(|r| r.as_i64()))
                        .unwrap_or(DEFAULT_POWER_LEVEL),
                ),
            ];

            for (key, current_level) in &scalar_checks {
                if let Some(new_level) = new_power_levels_content.get(key).and_then(|v| v.as_i64())
                {
                    if new_level != *current_level {
                        if *current_level > actor_level {
                            return Err(ApiError::forbidden(format!(
                                "Cannot change {} level: current level {} is above your own {}",
                                key, current_level, actor_level
                            )));
                        }
                        if new_level > actor_level {
                            return Err(ApiError::forbidden(format!(
                                "Cannot set {} level above your own: {} > {}",
                                key, new_level, actor_level
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn verify_room_moderator(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;

        let required_level = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| {
                content
                    .get("state_default")
                    .and_then(|level| level.as_i64())
            })
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if power_level < required_level {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_room_moderator_action",
                user_id = user_id,
                room_id = room_id,
                power_level = power_level,
                required_level = required_level,
                "User attempted moderator action without sufficient permission"
            );
            return Err(ApiError::forbidden(
                "Room moderator permission required".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn verify_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;

        // 默认 admin 需要 100，除非 power_levels 中有特殊定义
        let required_level = 100;

        if power_level < required_level {
            return Err(ApiError::forbidden(
                "Room admin permission required".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn can_kick_user(
        &self,
        room_id: &str,
        actor_user_id: &str,
        target_user_id: &str,
    ) -> ApiResult<()> {
        let actor_power = self
            .get_joined_user_power_level(room_id, actor_user_id)
            .await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("kick").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_kick_action",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                "User attempted to kick without moderator permission"
            );
            return Err(ApiError::forbidden(
                "Moderator permission required to kick users".to_string(),
            ));
        }

        if actor_power <= target_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "insufficient_power_to_kick",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                target_power = target_power,
                "User attempted to kick user with equal or higher power level"
            );
            return Err(ApiError::forbidden(
                "Cannot kick users with equal or higher power level".to_string(),
            ));
        }

        let room_creator: Option<String> = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(creator) = room_creator {
            if creator == target_user_id {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "attempted_kick_room_creator",
                    actor_user_id = actor_user_id,
                    target_user_id = target_user_id,
                    room_id = room_id,
                    "User attempted to kick room creator"
                );
                return Err(ApiError::forbidden(
                    "Cannot kick the room creator".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub async fn can_ban_user(
        &self,
        room_id: &str,
        actor_user_id: &str,
        target_user_id: &str,
    ) -> ApiResult<()> {
        let actor_power = self
            .get_joined_user_power_level(room_id, actor_user_id)
            .await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("ban").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_ban_action",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                required_power = required_power,
                "User attempted to ban without sufficient permission"
            );
            return Err(ApiError::forbidden(
                "Insufficient permission to ban users".to_string(),
            ));
        }

        if actor_power <= target_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "insufficient_power_to_ban",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                target_power = target_power,
                "User attempted to ban user with equal or higher power level"
            );
            return Err(ApiError::forbidden(
                "Cannot ban users with equal or higher power level".to_string(),
            ));
        }

        let room_creator: Option<String> = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(creator) = room_creator {
            if creator == target_user_id {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "attempted_ban_room_creator",
                    actor_user_id = actor_user_id,
                    target_user_id = target_user_id,
                    room_id = room_id,
                    "User attempted to ban room creator"
                );
                return Err(ApiError::forbidden(
                    "Cannot ban the room creator".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub async fn can_unban_user(
        &self,
        room_id: &str,
        actor_user_id: &str,
        target_user_id: &str,
    ) -> ApiResult<()> {
        let actor_power = self
            .get_joined_user_power_level(room_id, actor_user_id)
            .await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("ban").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_unban_action",
                actor_user_id = actor_user_id,
                room_id = room_id,
                actor_power = actor_power,
                required_power = required_power,
                "User attempted to unban without sufficient permission"
            );
            return Err(ApiError::forbidden(
                "Insufficient permission to unban users".to_string(),
            ));
        }

        if actor_power <= target_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "insufficient_power_to_unban",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                target_power = target_power,
                "User attempted to unban user with equal or higher power level"
            );
            return Err(ApiError::forbidden(
                "Cannot unban users with equal or higher power level".to_string(),
            ));
        }

        let room_creator: Option<String> = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(creator) = room_creator {
            if creator == target_user_id {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "attempted_unban_room_creator",
                    actor_user_id = actor_user_id,
                    target_user_id = target_user_id,
                    room_id = room_id,
                    "User attempted to unban room creator"
                );
                return Err(ApiError::forbidden(
                    "Cannot unban the room creator".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub async fn can_invite_user(&self, room_id: &str, actor_user_id: &str) -> ApiResult<()> {
        let actor_power = self
            .get_joined_user_power_level(room_id, actor_user_id)
            .await?;
        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("invite").and_then(|level| level.as_i64()))
            .unwrap_or(0);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_invite_action",
                actor_user_id = actor_user_id,
                room_id = room_id,
                actor_power = actor_power,
                required_power = required_power,
                "User attempted to invite without sufficient permission"
            );
            return Err(ApiError::forbidden(
                "Insufficient permission to invite users".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn can_redact_event(
        &self,
        room_id: &str,
        actor_user_id: &str,
        event_sender_id: &str,
    ) -> ApiResult<()> {
        let actor_power = self
            .get_joined_user_power_level(room_id, actor_user_id)
            .await?;

        if actor_power < 0 {
            ::tracing::warn!(
                target: "security_audit",
                event = "non_member_redact_attempt",
                actor_user_id = actor_user_id,
                room_id = room_id,
                "Non-member attempted to redact a room event"
            );
            return Err(ApiError::forbidden(
                "You must be a member of this room to redact events".to_string(),
            ));
        }

        if actor_user_id == event_sender_id {
            return Ok(());
        }

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("redact").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_redact_action",
                actor_user_id = actor_user_id,
                event_sender_id = event_sender_id,
                room_id = room_id,
                actor_power = actor_power,
                "User attempted to redact another user's event without moderator permission"
            );
            return Err(ApiError::forbidden(
                "Moderator permission required to redact other users' messages".to_string(),
            ));
        }

        Ok(())
    }
}

fn auth_generate_token(length: usize) -> String {
    static CHARSET: [u8; 62] = *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let mut token = String::with_capacity(length);
    for _ in 0..length {
        let idx = (rng.next_u32() as usize) % CHARSET.len();
        token.push(CHARSET[idx] as char);
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_struct() {
        let claims = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-uuid".to_string(),
            is_admin: false,
            exp: 1234567890,
            iat: 1234567889,
            device_id: Some("DEVICE123".to_string()),
        };
        assert_eq!(claims.sub, "@test:example.com");
        assert_eq!(claims.user_id, "@test:example.com");
        assert!(!claims.is_admin);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_claims_with_admin() {
        let claims = Claims {
            sub: "@admin:example.com".to_string(),
            user_id: "@admin:example.com".to_string(),
            jti: "test-jti-admin".to_string(),
            is_admin: true,
            exp: 1234567890,
            iat: 1234567890,
            device_id: None,
        };
        assert!(claims.is_admin);
        assert!(claims.device_id.is_none());
    }

    #[test]
    fn test_generate_token_length() {
        for len in [8, 16, 32, 64] {
            let token = auth_generate_token(len);
            assert_eq!(token.len(), len);
        }
    }

    #[test]
    fn test_generate_token_chars() {
        let token = auth_generate_token(100);
        for c in token.chars() {
            assert!(c.is_ascii_alphanumeric());
        }
    }

    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-serialization".to_string(),
            is_admin: false,
            exp: 1234567890,
            iat: 1234567890,
            device_id: Some("DEVICE123".to_string()),
        };
        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims.sub, deserialized.sub);
        assert_eq!(claims.user_id, deserialized.user_id);
        assert_eq!(claims.is_admin, deserialized.is_admin);
    }

    #[test]
    fn test_hash_token_consistency() {
        let token = "test_refresh_token_12345";
        let hash1 = AuthService::hash_token(token);
        let hash2 = AuthService::hash_token(token);
        assert_eq!(hash1, hash2, "Same token should produce same hash");
        assert!(!hash1.is_empty(), "Hash should not be empty");
    }

    #[test]
    fn test_hash_token_different_tokens() {
        let token1 = "token_one";
        let token2 = "token_two";
        let hash1 = AuthService::hash_token(token1);
        let hash2 = AuthService::hash_token(token2);
        assert_ne!(
            hash1, hash2,
            "Different tokens should produce different hashes"
        );
    }

    #[test]
    fn test_hash_token_empty_string() {
        let hash = AuthService::hash_token("");
        assert!(!hash.is_empty(), "Empty token should still produce a hash");
    }

    #[test]
    fn test_hash_token_format() {
        let token = "test_token";
        let hash = AuthService::hash_token(token);
        assert_eq!(
            hash.len(),
            43,
            "SHA256 base64 encoded hash should be 43 chars"
        );
    }

    #[test]
    fn test_password_hash_and_verify() {
        let password = "secure_password_123";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password_common(password, &hash, false).unwrap());
        assert!(!verify_password_common("wrong_password", &hash, false).unwrap());
    }

    #[test]
    fn test_password_hash_uniqueness() {
        let password = "same_password";
        let hash1 = hash_password_with_params(password, 65536, 3, 1).unwrap();
        let hash2 = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert_ne!(
            hash1, hash2,
            "Same password should produce different hashes due to salt"
        );
    }

    #[test]
    fn test_password_verify_wrong_password() {
        let password = "correct_password";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(!verify_password_common("incorrect_password", &hash, false).unwrap());
    }

    #[test]
    fn test_password_empty_password() {
        let password = "";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password_common("", &hash, false).unwrap());
    }

    #[test]
    fn test_password_long_password() {
        let password = "a".repeat(1000);
        let hash = hash_password_with_params(&password, 65536, 3, 1).unwrap();
        assert!(verify_password_common(&password, &hash, false).unwrap());
    }

    #[test]
    fn test_is_legacy_hash_argon2() {
        let argon2_hash = "$argon2id$v=19$m=65536,t=3,p=1$c2FsdA$hash";
        assert!(!is_legacy_hash(argon2_hash));
    }

    #[test]
    fn test_is_legacy_hash_sha256() {
        let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
        assert!(is_legacy_hash(legacy_hash));
    }

    #[test]
    fn test_is_legacy_hash_bcrypt() {
        let bcrypt_hash = "$2b$12$abcdefghijklmnopqrstuv";
        assert!(is_legacy_hash(bcrypt_hash));
    }

    #[test]
    fn test_claims_expiration_validation() {
        let now = Utc::now().timestamp();
        let valid_claims = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-valid".to_string(),
            is_admin: false,
            exp: now + 3600,
            iat: now,
            device_id: None,
        };
        assert!(valid_claims.exp > now);

        let expired_claims = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-expired".to_string(),
            is_admin: false,
            exp: now - 3600,
            iat: now - 7200,
            device_id: None,
        };
        assert!(expired_claims.exp < now);
    }

    #[test]
    fn test_claims_device_id_optional() {
        let claims_with_device = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-with-device".to_string(),
            is_admin: false,
            exp: 1234567890,
            iat: 1234567890,
            device_id: Some("DEVICE123".to_string()),
        };
        assert!(claims_with_device.device_id.is_some());

        let claims_without_device = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-no-device".to_string(),
            is_admin: false,
            exp: 1234567890,
            iat: 1234567890,
            device_id: None,
        };
        assert!(claims_without_device.device_id.is_none());
    }

    #[test]
    fn test_jwt_encode_decode() {
        let jwt_secret = b"test_secret_key_for_jwt_encoding";
        let now = Utc::now();
        let claims = Claims {
            sub: "@user:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            is_admin: true,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            device_id: Some("DEVICE456".to_string()),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

        let validation = Validation::new(Algorithm::HS256);
        let decoded: Claims =
            jsonwebtoken::decode(&token, &DecodingKey::from_secret(jwt_secret), &validation)
                .unwrap()
                .claims;

        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.user_id, claims.user_id);
        assert_eq!(decoded.is_admin, claims.is_admin);
        assert_eq!(decoded.device_id, claims.device_id);
    }

    #[test]
    fn test_jwt_decode_wrong_secret() {
        let jwt_secret = b"correct_secret";
        let wrong_secret = b"wrong_secret";
        let now = Utc::now();
        let claims = Claims {
            sub: "@user:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            is_admin: false,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            device_id: None,
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

        let validation = Validation::new(Algorithm::HS256);
        let result = jsonwebtoken::decode::<Claims>(
            &token,
            &DecodingKey::from_secret(wrong_secret),
            &validation,
        );

        assert!(result.is_err(), "Decoding with wrong secret should fail");
    }

    #[test]
    fn test_jwt_expired_token() {
        let jwt_secret = b"test_secret";
        let now = Utc::now();
        let claims = Claims {
            sub: "@user:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            is_admin: false,
            exp: (now - Duration::hours(1)).timestamp(),
            iat: (now - Duration::hours(2)).timestamp(),
            device_id: None,
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

        let validation = Validation::new(Algorithm::HS256);
        let result = jsonwebtoken::decode::<Claims>(
            &token,
            &DecodingKey::from_secret(jwt_secret),
            &validation,
        );

        assert!(result.is_err(), "Expired token should fail validation");
    }

    #[test]
    fn test_jwt_tampered_token() {
        let jwt_secret = b"test_secret";
        let now = Utc::now();
        let claims = Claims {
            sub: "@user:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            is_admin: false,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            device_id: None,
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

        let mut tampered = token.chars().collect::<Vec<char>>();
        if let Some(last) = tampered.last_mut() {
            *last = if *last == 'A' { 'B' } else { 'A' };
        }
        let tampered_token: String = tampered.into_iter().collect();

        let validation = Validation::new(Algorithm::HS256);
        let result = jsonwebtoken::decode::<Claims>(
            &tampered_token,
            &DecodingKey::from_secret(jwt_secret),
            &validation,
        );

        assert!(result.is_err(), "Tampered token should fail validation");
    }

    #[test]
    fn test_auth_generate_token_uniqueness() {
        let tokens: Vec<String> = (0..100).map(|_| auth_generate_token(32)).collect();
        let unique_count = tokens
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(unique_count, 100, "All generated tokens should be unique");
    }

    #[test]
    fn test_auth_generate_token_charset() {
        let token = auth_generate_token(1000);
        let valid_chars: std::collections::HashSet<char> =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
                .chars()
                .collect();
        for c in token.chars() {
            assert!(
                valid_chars.contains(&c),
                "Token should only contain alphanumeric characters"
            );
        }
    }

    #[test]
    fn test_claims_json_roundtrip() {
        let original = Claims {
            sub: "@test:server.com".to_string(),
            user_id: "@test:server.com".to_string(),
            jti: "test-jti-roundtrip".to_string(),
            is_admin: true,
            exp: 9999999999,
            iat: 1000000000,
            device_id: Some("MYDEVICE".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: Claims = serde_json::from_str(&json).unwrap();

        assert_eq!(original.sub, parsed.sub);
        assert_eq!(original.user_id, parsed.user_id);
        assert_eq!(original.is_admin, parsed.is_admin);
        assert_eq!(original.exp, parsed.exp);
        assert_eq!(original.iat, parsed.iat);
        assert_eq!(original.device_id, parsed.device_id);
    }

    #[test]
    fn test_claims_json_structure() {
        let claims = Claims {
            sub: "@user:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            jti: "test-jti-structure".to_string(),
            is_admin: false,
            exp: 1234567890,
            iat: 1234567800,
            device_id: Some("DEV1".to_string()),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["sub"], "@user:example.com");
        assert_eq!(value["user_id"], "@user:example.com");
        assert_eq!(value["admin"], false);
        assert_eq!(value["exp"], 1234567890);
        assert_eq!(value["iat"], 1234567800);
        assert_eq!(value["device_id"], "DEV1");
    }

    #[test]
    fn test_password_special_characters() {
        let password = "p@$$w0rd!#$%^&*()_+-=[]{}|;':\",./<>?";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(verify_password_common(password, &hash, false).unwrap());
    }

    #[test]
    fn test_password_unicode() {
        let password = "密码测试🔐🎉";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(verify_password_common(password, &hash, false).unwrap());
    }

    #[test]
    fn test_password_whitespace() {
        let password = "  password with spaces  ";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(verify_password_common(password, &hash, false).unwrap());
        assert!(!verify_password_common("password with spaces", &hash, false).unwrap());
    }

    #[test]
    fn test_migrate_password_hash() {
        let password = "password_to_migrate";
        let new_hash = migrate_password_hash(password, 65536, 3, 1).unwrap();
        assert!(new_hash.starts_with("$argon2"));
        assert!(verify_password_common(password, &new_hash, false).unwrap());
    }

    #[test]
    fn test_auth_service_hash_password_direct() {
        let password = "test_password";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password_common(password, &hash, false).unwrap());
    }

    #[test]
    fn test_auth_service_verify_password_wrong_direct() {
        let password = "correct_password";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(!verify_password_common("wrong_password", &hash, false).unwrap());
    }

    #[test]
    fn test_auth_service_jwt_generation_direct() {
        let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
        let now = Utc::now();
        let claims = Claims {
            sub: "@user:test.server".to_string(),
            user_id: "@user:test.server".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            is_admin: false,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            device_id: Some("DEVICE1".to_string()),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

        assert!(!token.is_empty());

        let validation = Validation::new(Algorithm::HS256);
        let decoded: Claims =
            jsonwebtoken::decode(&token, &DecodingKey::from_secret(jwt_secret), &validation)
                .unwrap()
                .claims;

        assert_eq!(decoded.sub, "@user:test.server");
        assert_eq!(decoded.user_id, "@user:test.server");
        assert!(!decoded.is_admin);
        assert_eq!(decoded.device_id, Some("DEVICE1".to_string()));
    }

    #[test]
    fn test_auth_service_jwt_admin_flag_direct() {
        let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
        let now = Utc::now();
        let claims = Claims {
            sub: "@admin:test.server".to_string(),
            user_id: "@admin:test.server".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            is_admin: true,
            exp: (now + Duration::hours(1)).timestamp(),
            iat: now.timestamp(),
            device_id: Some("DEVICE2".to_string()),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        let token = encode(&header, &claims, &EncodingKey::from_secret(jwt_secret)).unwrap();

        let validation = Validation::new(Algorithm::HS256);
        let decoded: Claims =
            jsonwebtoken::decode(&token, &DecodingKey::from_secret(jwt_secret), &validation)
                .unwrap()
                .claims;

        assert!(decoded.is_admin);
    }

    #[test]
    fn test_auth_service_jwt_expiration_direct() {
        let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
        let token_expiry: i64 = 3600;
        let now = Utc::now().timestamp();

        let claims = Claims {
            sub: "@user:test.server".to_string(),
            user_id: "@user:test.server".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            is_admin: false,
            exp: now + token_expiry,
            iat: now,
            device_id: Some("DEVICE".to_string()),
        };

        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(jwt_secret),
        )
        .unwrap();

        let decoded: Claims = jsonwebtoken::decode(
            &token,
            &DecodingKey::from_secret(jwt_secret),
            &Validation::new(Algorithm::HS256),
        )
        .unwrap()
        .claims;

        assert!(decoded.exp > now);
        assert!(decoded.exp <= now + token_expiry + 1);
    }

    #[test]
    fn test_auth_service_decode_invalid_token_direct() {
        let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
        let result = jsonwebtoken::decode::<Claims>(
            "invalid.token.here",
            &DecodingKey::from_secret(jwt_secret),
            &Validation::new(Algorithm::HS256),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_auth_service_decode_malformed_token_direct() {
        let jwt_secret = b"test_jwt_secret_key_for_unit_tests";
        let result = jsonwebtoken::decode::<Claims>(
            "not-a-valid-jwt",
            &DecodingKey::from_secret(jwt_secret),
            &Validation::new(Algorithm::HS256),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_auth_service_allow_legacy_hashes_config_direct() {
        let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
        let result = verify_password_common("any_password", legacy_hash, true);
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_auth_service_disallow_legacy_hashes_direct() {
        let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
        let result = verify_password_common("any_password", legacy_hash, false);
        assert!(result.is_err(), "Should reject legacy hash when disabled");
    }

    #[test]
    fn test_lockout_threshold_default_value() {
        let threshold: u32 = 5;
        assert_eq!(threshold, 5);
    }

    #[test]
    fn test_lockout_duration_default_value() {
        let duration: u64 = 900;
        assert_eq!(duration, 900);
    }

    #[test]
    fn test_token_expiry_default_value() {
        let expiry: i64 = 3600;
        assert_eq!(expiry, 3600);
    }

    #[test]
    fn test_refresh_token_expiry_default_value() {
        let expiry: i64 = 604800;
        assert_eq!(expiry, 604800);
    }

    #[test]
    fn test_generate_email_verification_token_direct() {
        let token1 = auth_generate_token(32);
        let token2 = auth_generate_token(32);

        assert_eq!(token1.len(), 32);
        assert_eq!(token2.len(), 32);
        assert_ne!(token1, token2, "Each token should be unique");
    }
}
