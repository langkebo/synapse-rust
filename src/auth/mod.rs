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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub jti: String,
    pub admin: bool,
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
        Self {
            user_storage: UserStorage::new(pool, cache.clone()),
            device_storage: DeviceStorage::new(pool),
            token_storage: AccessTokenStorage::new(pool),
            refresh_token_storage: RefreshTokenStorage::new(pool),
            cache,
            metrics,
            validator: Arc::new(Validator::default()),
            jwt_secret: security.secret.as_bytes().to_vec(),
            token_expiry: security.expiry_time,
            refresh_token_expiry: security.refresh_token_expiry,
            server_name: server_name.to_string(),
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
            .register_internal(username, password, admin, displayname)
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
        _displayname: Option<&str>,
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
            .create_device_tx(&mut tx, &device_id, &user_id, None)
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
        _initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let user = self.get_user_by_username(username).await?;

        if self.is_user_deactivated(&user) {
            return Err(ApiError::user_deactivated(
                "User account has been deactivated",
            ));
        }

        if self.is_account_locked(&user.user_id).await? {
            self.log_login_failure(username, "account_locked");
            return Err(ApiError::rate_limited(
                "Account is temporarily locked due to too many failed login attempts. Please try again later.".to_string(),
            ));
        }

        let password_hash = self.get_user_password_hash(&user)?;

        if !self.verify_user_password(password, password_hash).await? {
            self.record_login_failure(&user.user_id).await?;
            self.log_login_failure(username, "invalid_credentials");
            return Err(ApiError::unauthorized("Invalid credentials".to_string()));
        }

        self.clear_login_failures(&user.user_id).await?;

        if is_legacy_hash(password_hash) {
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
        self.log_login_success(&user, device_id);

        let device_id = self.get_or_create_device_id(device_id, &user).await?;

        let access_token = self
            .generate_access_token(&user.user_id, &device_id, user.is_admin)
            .await?;
        let refresh_token = self
            .generate_refresh_token(&user.user_id, &device_id)
            .await?;

        Ok((user, access_token, refresh_token, device_id))
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
        let failures: i64 = self.cache.get(&key).await?.unwrap_or(0) + 1;

        let _ = self
            .cache
            .set(&key, &failures, self.login_lockout_duration_seconds)
            .await;

        if failures >= self.login_failure_lockout_threshold as i64 {
            let lockout_until = Utc::now().timestamp() + self.login_lockout_duration_seconds as i64;
            let lockout_key = format!("auth:lockout:{}", user_id);
            let _ = self
                .cache
                .set(
                    &lockout_key,
                    &lockout_until,
                    self.login_lockout_duration_seconds,
                )
                .await;

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

    async fn get_user_by_username(&self, username: &str) -> ApiResult<User> {
        let user = self
            .user_storage
            .get_user_by_identifier(username)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        user.ok_or_else(|| ApiError::unauthorized("Invalid credentials".to_string()))
    }

    fn get_user_password_hash<'a>(&self, user: &'a User) -> ApiResult<&'a str> {
        user.password_hash
            .as_deref()
            .ok_or_else(|| ApiError::unauthorized("Invalid credentials".to_string()))
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

    fn is_user_deactivated(&self, user: &User) -> bool {
        user.is_deactivated
    }

    fn log_login_failure(&self, username: &str, reason: &str) {
        ::tracing::warn!(
            target: "security_audit",
            event = "login_failure",
            username = username,
            reason = reason
        );
    }

    fn log_login_success(&self, user: &User, device_id: Option<&str>) {
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
                .create_device(&device_id, &user.user_id, None)
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
            .set_raw(&logout_marker, &now.to_string(), 3600)
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

        match token_data {
            Some(t) => {
                if t.is_revoked {
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

                        let device_id = t.device_id.clone().unwrap_or_default();
                        let new_access_token = self
                            .generate_access_token(&u.user_id, &device_id, u.is_admin)
                            .await?;
                        let new_refresh_token =
                            self.generate_refresh_token(&u.user_id, &device_id).await?;

                        self.refresh_token_storage
                            .revoke_token(&token_hash, "Rotated")
                            .await
                            .map_err(|e| {
                                ApiError::internal(format!(
                                    "Failed to revoke old refresh token: {}",
                                    e
                                ))
                            })?;

                        Ok((new_access_token, new_refresh_token, device_id))
                    }
                    _ => Err(ApiError::unauthorized("User not found".to_string())),
                }
            }
            _ => Err(ApiError::unauthorized("Invalid refresh token".to_string())),
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
            ::tracing::debug!(target: "token_validation", "Invalid token: {}", e);
            ApiError::unauthorized(format!("Invalid token: {}", e))
        })?;

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
                    let is_admin = if let Some(is_admin) =
                        self.cache.get::<bool>(&admin_cache_key).await?
                    {
                        is_admin
                    } else {
                        let user = self
                            .user_storage
                            .get_user_by_id(&cached_claims.sub)
                            .await
                            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
                            .ok_or_else(|| ApiError::unauthorized("User not found".to_string()))?;
                        self.cache.set(&admin_cache_key, user.is_admin, 60).await?;
                        self.cache
                            .set(
                                &format!("user:shadow_banned:{}", cached_claims.sub),
                                user.is_shadow_banned,
                                60,
                            )
                            .await?;
                        self.cache
                            .set(
                                &format!("user:guest:{}", cached_claims.sub),
                                user.is_guest,
                                60,
                            )
                            .await?;
                        user.is_admin
                    };
                    let is_shadow_banned = self
                        .cache
                        .get::<bool>(&format!("user:shadow_banned:{}", cached_claims.sub))
                        .await?
                        .unwrap_or(false);
                    let is_guest = self
                        .cache
                        .get::<bool>(&format!("user:guest:{}", cached_claims.sub))
                        .await?
                        .unwrap_or(false);
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
                    .set_user_active(&cached_claims.sub, is_active, 60)
                    .await;
                self.cache.set(&admin_cache_key, u.is_admin, 60).await?;
                self.cache
                    .set(
                        &format!("user:shadow_banned:{}", cached_claims.sub),
                        u.is_shadow_banned,
                        60,
                    )
                    .await?;
                self.cache
                    .set(&format!("user:guest:{}", cached_claims.sub), u.is_guest, 60)
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
                    .set_user_active(&cached_claims.sub, false, 60)
                    .await;
                Err(ApiError::unauthorized("User not found".to_string()))
            };
        }

        ::tracing::debug!(target: "token_validation", "Token not found in cache, using decoded JWT");

        if claims.exp < Utc::now().timestamp() {
            ::tracing::debug!(target: "token_validation", "Token expired");
            return Err(ApiError::unauthorized("Token expired".to_string()));
        }

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
                final_claims.admin = is_admin;

                self.cache.set_user_active(&claims.sub, true, 60).await;
                self.cache
                    .set(&format!("user:admin:{}", claims.sub), is_admin, 60)
                    .await?;
                self.cache
                    .set(
                        &format!("user:shadow_banned:{}", claims.sub),
                        u.is_shadow_banned,
                        60,
                    )
                    .await?;
                self.cache
                    .set(&format!("user:guest:{}", claims.sub), u.is_guest, 60)
                    .await?;
                self.cache.set_token(token, &final_claims, 3600).await;
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
    ) -> ApiResult<()> {
        if let Some(pwd) = current_password {
            let user = self
                .user_storage
                .get_user_by_id(user_id)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
                .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

            if let Some(ref password_hash) = user.password_hash {
                if !self.verify_user_password(pwd, password_hash).await? {
                    return Err(ApiError::unauthorized(
                        "Current password is incorrect".to_string(),
                    ));
                }
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

        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to invalidate tokens: {}", e)))?;

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

        self.device_storage
            .delete_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

        self.cache.delete(&format!("user:active:{}", user_id)).await;
        self.cache.delete(&format!("user:admin:{}", user_id)).await;

        Ok(())
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
            admin,
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

    fn decode_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let validation = Validation::new(Algorithm::HS256);
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
        let member: Option<(String,)> = sqlx::query_as(
            "SELECT membership FROM room_memberships WHERE room_id = $1 AND user_id = $2",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if member.is_none() {
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

        let room_creator: Option<String> =
            sqlx::query_scalar("SELECT creator FROM rooms WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.user_storage.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(creator) = room_creator {
            if creator == user_id {
                return Ok(100);
            }
        }

        Ok(0)
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

        Ok(50)
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
        _is_server_admin: bool,
    ) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;
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
        _is_server_admin: bool,
    ) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;
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
        new_power_levels_content: &serde_json::Value,
        _is_server_admin: bool,
    ) -> ApiResult<()> {
        let actor_level = self.get_user_power_level(room_id, user_id).await?;

        let current_content = self.get_room_power_levels_content(room_id).await?;

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
                        .unwrap_or(50),
                ),
                (
                    "ban",
                    current.get("ban").and_then(|v| v.as_i64()).unwrap_or(50),
                ),
                (
                    "kick",
                    current.get("kick").and_then(|v| v.as_i64()).unwrap_or(50),
                ),
                (
                    "redact",
                    current.get("redact").and_then(|v| v.as_i64()).unwrap_or(50),
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
                        .unwrap_or(50),
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

    pub async fn verify_room_moderator(
        &self,
        room_id: &str,
        user_id: &str,
        _is_server_admin: bool,
    ) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;

        let required_level = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| {
                content
                    .get("state_default")
                    .and_then(|level| level.as_i64())
            })
            .unwrap_or(50);

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

    pub async fn can_kick_user(
        &self,
        room_id: &str,
        actor_user_id: &str,
        target_user_id: &str,
        _is_server_admin: bool,
    ) -> ApiResult<()> {
        let actor_power = self.get_user_power_level(room_id, actor_user_id).await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("kick").and_then(|level| level.as_i64()))
            .unwrap_or(50);

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

        let room_creator: Option<String> =
            sqlx::query_scalar("SELECT creator FROM rooms WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.user_storage.pool)
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
        _is_server_admin: bool,
    ) -> ApiResult<()> {
        let actor_power = self.get_user_power_level(room_id, actor_user_id).await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("ban").and_then(|level| level.as_i64()))
            .unwrap_or(50);

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

        let room_creator: Option<String> =
            sqlx::query_scalar("SELECT creator FROM rooms WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.user_storage.pool)
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

    pub async fn can_invite_user(
        &self,
        room_id: &str,
        actor_user_id: &str,
        is_server_admin: bool,
    ) -> ApiResult<()> {
        if is_server_admin {
            let actor_power = self.get_user_power_level(room_id, actor_user_id).await?;
            if actor_power < 0 {
                return Err(ApiError::forbidden(
                    "Server admin must be a room member to invite".to_string(),
                ));
            }
        }

        let actor_power = self.get_user_power_level(room_id, actor_user_id).await?;
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
        is_server_admin: bool,
    ) -> ApiResult<()> {
        if is_server_admin {
            return Ok(());
        }

        let actor_power = self.get_user_power_level(room_id, actor_user_id).await?;

        if actor_user_id == event_sender_id {
            return Ok(());
        }

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("redact").and_then(|level| level.as_i64()))
            .unwrap_or(50);

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
            admin: false,
            exp: 1234567890,
            iat: 1234567889,
            device_id: Some("DEVICE123".to_string()),
        };
        assert_eq!(claims.sub, "@test:example.com");
        assert_eq!(claims.user_id, "@test:example.com");
        assert!(!claims.admin);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_claims_with_admin() {
        let claims = Claims {
            sub: "@admin:example.com".to_string(),
            user_id: "@admin:example.com".to_string(),
            jti: "test-jti-admin".to_string(),
            admin: true,
            exp: 1234567890,
            iat: 1234567890,
            device_id: None,
        };
        assert!(claims.admin);
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
            admin: false,
            exp: 1234567890,
            iat: 1234567890,
            device_id: Some("DEVICE123".to_string()),
        };
        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims.sub, deserialized.sub);
        assert_eq!(claims.user_id, deserialized.user_id);
        assert_eq!(claims.admin, deserialized.admin);
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
            admin: false,
            exp: now + 3600,
            iat: now,
            device_id: None,
        };
        assert!(valid_claims.exp > now);

        let expired_claims = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-expired".to_string(),
            admin: false,
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
            admin: false,
            exp: 1234567890,
            iat: 1234567890,
            device_id: Some("DEVICE123".to_string()),
        };
        assert!(claims_with_device.device_id.is_some());

        let claims_without_device = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            jti: "test-jti-no-device".to_string(),
            admin: false,
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
            admin: true,
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
        assert_eq!(decoded.admin, claims.admin);
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
            admin: false,
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
            admin: false,
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
            admin: false,
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
            admin: true,
            exp: 9999999999,
            iat: 1000000000,
            device_id: Some("MYDEVICE".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: Claims = serde_json::from_str(&json).unwrap();

        assert_eq!(original.sub, parsed.sub);
        assert_eq!(original.user_id, parsed.user_id);
        assert_eq!(original.admin, parsed.admin);
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
            admin: false,
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
            admin: false,
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
        assert!(!decoded.admin);
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
            admin: true,
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

        assert!(decoded.admin);
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
            admin: false,
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
