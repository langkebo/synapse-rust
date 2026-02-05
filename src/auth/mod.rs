use crate::cache::*;
use crate::common::config::SecurityConfig;
use crate::common::crypto::{hash_password_with_params, verify_password as verify_password_common};
use crate::common::metrics::MetricsCollector;
use crate::common::validation::Validator;
use crate::common::*;
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
            user_storage: UserStorage::new(pool),
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
        let user = self
            .user_storage
            .get_user_by_identifier(username)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let user = match user {
            Some(u) => u,
            _ => return Err(ApiError::unauthorized("Invalid credentials".to_string())),
        };

        let password_hash = match &user.password_hash {
            Some(h) => h,
            _ => return Err(ApiError::unauthorized("Invalid credentials".to_string())),
        };

        // P1 Performance: Run CPU-intensive verification in spawn_blocking
        let auth = self.clone();
        let password_str = password.to_string();
        let password_hash_str = password_hash.to_string();
        let is_valid = tokio::task::spawn_blocking(move || {
            auth.verify_password(&password_str, &password_hash_str)
        })
        .await
        .map_err(|e| ApiError::internal(format!("Verification task panicked: {}", e)))??;

        if !is_valid {
            ::tracing::warn!(
                target: "security_audit",
                event = "login_failure",
                username = username,
                reason = "invalid_credentials"
            );
            return Err(ApiError::unauthorized("Invalid credentials".to_string()));
        }

        if user.deactivated == Some(true) {
            return Err(ApiError::unauthorized("User is deactivated".to_string()));
        }

        ::tracing::info!(
            target: "security_audit",
            event = "login_success",
            user_id = user.user_id(),
            device_id = device_id
        );

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

        let access_token = self
            .generate_access_token(&user.user_id, &device_id, user.is_admin.unwrap_or(false))
            .await?;
        let refresh_token = self
            .generate_refresh_token(&user.user_id, &device_id)
            .await?;

        Ok((user, access_token, refresh_token, device_id))
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
        let token_data = self
            .refresh_token_storage
            .get_refresh_token(refresh_token)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        match token_data {
            Some(t) => {
                if t.expires_ts > 0 && t.expires_ts < Utc::now().timestamp() {
                    return Err(ApiError::unauthorized("Refresh token expired".to_string()));
                }

                let user = self
                    .user_storage
                    .get_user_by_id(&t.user_id)
                    .await
                    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

                match user {
                    Some(u) => {
                        let new_access_token = self
                            .generate_access_token(
                                &u.user_id,
                                &t.device_id,
                                u.is_admin.unwrap_or(false),
                            )
                            .await?;
                        let new_refresh_token = self
                            .generate_refresh_token(&u.user_id, &t.device_id)
                            .await?;

                        self.refresh_token_storage
                            .delete_refresh_token(refresh_token)
                            .await
                            .map_err(|e| {
                                ApiError::internal(format!(
                                    "Failed to delete old refresh token: {}",
                                    e
                                ))
                            })?;

                        Ok((new_access_token, new_refresh_token, t.device_id))
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
                if !active {
                    return Err(ApiError::unauthorized(
                        "User not found or deactivated".to_string(),
                    ));
                }
                return Ok((claims.user_id, claims.device_id.clone(), claims.admin));
            }

            ::tracing::debug!(target: "token_validation", "Cache miss for user active status, querying DB");

            let user = self
                .user_storage
                .get_user_by_id(&claims.sub)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            match user {
                Some(u) => {
                    let is_active = u.deactivated != Some(true);
                    ::tracing::debug!(target: "token_validation", "User found, deactivated: {:?}, is_active: {}", u.deactivated, is_active);

                    self.cache.set_user_active(&claims.sub, is_active, 60).await;

                    if !is_active {
                        return Err(ApiError::unauthorized("User is deactivated".to_string()));
                    }

                    return Ok((claims.user_id, claims.device_id.clone(), claims.admin));
                }
                None => {
                    ::tracing::debug!(target: "token_validation", "User not found in database");
                    self.cache.set_user_active(&claims.sub, false, 60).await;
                    return Err(ApiError::unauthorized("User not found".to_string()));
                }
            }
        }

        ::tracing::debug!(target: "token_validation", "Token not found in cache, decoding from JWT");

        match self.decode_token(token) {
            Ok(claims) => {
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
                        ::tracing::debug!(target: "token_validation", "User found, deactivated: {:?}", u.deactivated);
                        if u.deactivated == Some(true) {
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
            Err(e) => {
                ::tracing::debug!(target: "token_validation", "Invalid token: {}", e);
                Err(ApiError::unauthorized(format!("Invalid token: {}", e)))
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

    async fn generate_access_token(
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
        let expiry_ts = Utc::now() + Duration::seconds(self.refresh_token_expiry);
        let expiry_timestamp = expiry_ts.timestamp();

        self.refresh_token_storage
            .create_refresh_token(&token, user_id, device_id, Some(expiry_timestamp))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to store refresh token: {}", e)))?;

        Ok(token)
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
        verify_password_common(password, password_hash).map_err(ApiError::internal)
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
