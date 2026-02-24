use crate::cache::*;
use crate::common::config::SecurityConfig;
use crate::common::crypto::{hash_password_with_params, verify_password as verify_password_common};
use crate::common::metrics::MetricsCollector;
use crate::common::validation::Validator;
use crate::common::*;
use crate::storage::refresh_token::RefreshTokenStorage;
use crate::storage::*;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: String,
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
            return Err(e.into());
        }

        let existing_user = self
            .user_storage
            .get_user_by_username(username)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if existing_user.is_some() {
            return Err(ApiError::conflict("Username already taken".to_string()));
        }

        let user_id = format!("@{}:{}", username, self.server_name);

        // P1 Performance: Run CPU-intensive hashing in spawn_blocking to avoid blocking the async executor
        let auth = self.clone();
        let password_str = password.to_string();
        let password_hash = tokio::task::spawn_blocking(move || auth.hash_password(&password_str))
            .await
            .map_err(|e| ApiError::internal(format!("Hashing task panicked: {}", e)))??;

        let user = self
            .user_storage
            .create_user(&user_id, username, Some(&password_hash), admin)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create user: {}", e)))?;

        ::tracing::info!(
            target: "security_audit",
            event = "user_registered",
            user_id = user_id,
            admin = admin
        );

        let device_id = generate_token(16);
        self.device_storage
            .create_device(&device_id, &user_id, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create device: {}", e)))?;

        let access_token = self
            .generate_access_token(&user_id, &device_id, user.is_admin.unwrap_or(false))
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

        if self.is_user_deactivated(&user) {
            return Err(ApiError::forbidden("User is deactivated".to_string()));
        }

        self.log_login_success(&user, device_id);

        let device_id = self.get_or_create_device_id(device_id, &user).await?;

        let access_token = self
            .generate_access_token(&user.user_id, &device_id, user.is_admin.unwrap_or(false))
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
        user.is_deactivated == Some(true)
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

        if !self
            .device_storage
            .device_exists(&device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
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

        Ok(())
    }

    pub async fn logout_all(&self, user_id: &str) -> ApiResult<()> {
        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete tokens: {}", e)))?;

        self.device_storage
            .delete_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

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
                        let device_id = t.device_id.clone().unwrap_or_default();
                        let new_access_token = self
                            .generate_access_token(
                                &u.user_id,
                                &device_id,
                                u.is_admin.unwrap_or(false),
                            )
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

    pub async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool)> {
        ::tracing::debug!(target: "token_validation", "Validating token");

        let cached_token = self.cache.get_token(token).await;
        if let Some(claims) = cached_token {
            ::tracing::debug!(target: "token_validation", "Found cached token for user: {}", 
                claims.sub);

            if let Some(active) = self.cache.is_user_active(&claims.sub).await {
                ::tracing::debug!(target: "token_validation", "Cache hit for user active: {:?}", active);
                return if active {
                    Ok((claims.user_id, claims.device_id.clone(), claims.admin))
                } else {
                    Err(ApiError::unauthorized(
                        "User not found or deactivated".to_string(),
                    ))
                };
            }

            ::tracing::debug!(target: "token_validation", "Cache miss for user active status, querying DB");

            let user = self
                .user_storage
                .get_user_by_id(&claims.sub)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            return if let Some(u) = user {
                let is_active = u.is_deactivated != Some(true);
                ::tracing::debug!(target: "token_validation", "User found, is_deactivated: {:?}, is_active: {}", u.is_deactivated, is_active);

                self.cache.set_user_active(&claims.sub, is_active, 60).await;

                if is_active {
                    Ok((claims.user_id, claims.device_id.clone(), claims.admin))
                } else {
                    Err(ApiError::unauthorized("User is deactivated".to_string()))
                }
            } else {
                ::tracing::debug!(target: "token_validation", "User not found in database");
                self.cache.set_user_active(&claims.sub, false, 60).await;
                Err(ApiError::unauthorized("User not found".to_string()))
            };
        }

        ::tracing::debug!(target: "token_validation", "Token not found in cache, decoding from JWT");

        let claims = self.decode_token(token).map_err(|e| {
            ::tracing::debug!(target: "token_validation", "Invalid token: {}", e);
            ApiError::unauthorized(format!("Invalid token: {}", e))
        })?;

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
                if u.is_deactivated == Some(true) {
                    ::tracing::debug!(target: "token_validation", "User is deactivated, rejecting token");
                    return Err(ApiError::unauthorized("User is deactivated".to_string()));
                }
                let is_admin = u.is_admin.unwrap_or(false);
                let mut final_claims = claims.clone();
                final_claims.admin = is_admin;

                self.cache.set_token(token, &final_claims, 3600).await;
                Ok((
                    final_claims.user_id,
                    final_claims.device_id.clone(),
                    is_admin,
                ))
            }
            None => {
                ::tracing::debug!(target: "token_validation", "User not found in database");
                Err(ApiError::unauthorized("User not found".to_string()))
            }
        }
    }

    pub async fn change_password(&self, user_id: &str, new_password: &str) -> ApiResult<()> {
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

        Ok(())
    }

    pub async fn generate_access_token(
        &self,
        user_id: &str,
        device_id: &str,
        admin: bool,
    ) -> ApiResult<String> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            user_id: user_id.to_string(),
            admin,
            exp: (now + Duration::seconds(self.token_expiry)).timestamp(),
            iat: now.timestamp(),
            device_id: Some(device_id.to_string()),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| ApiError::internal(format!("Failed to generate token: {}", e)))
    }

    async fn generate_refresh_token(&self, user_id: &str, device_id: &str) -> ApiResult<String> {
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
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let result = hasher.finalize();
        URL_SAFE_NO_PAD.encode(result)
    }

    fn decode_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        jsonwebtoken::decode(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &Validation::default(),
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

    pub fn generate_email_verification_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let token = auth_generate_token(32);
        Ok(token)
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
}
