mod account;
mod login;
mod power_levels;
mod register;
mod session;
#[cfg(test)]
mod tests;
mod token;

use crate::cache::*;
use crate::common::config::SecurityConfig;
use crate::common::metrics::MetricsCollector;
use crate::common::validation::Validator;
use crate::storage::refresh_token::RefreshTokenStorage;
use crate::storage::*;
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

/// Builder for constructing [`Claims`] instances with a fluent API.
///
/// Eliminates the scattered `Claims { … }` struct literals across
/// `auth/token.rs`, `cache/mod.rs`, and `auth/tests.rs`, ensuring
/// consistent default values (e.g. `jti` auto-generated, `iat` = now).
///
/// # Example
///
/// ```ignore
/// let claims = ClaimsBuilder::new()
///     .sub("@alice:example.com")
///     .user_id("@alice:example.com")
///     .is_admin(false)
///     .exp(now + 3600)
///     .iat(now)
///     .device_id(Some("DEVICE1".to_string()))
///     .build();
/// ```
pub struct ClaimsBuilder {
    sub: Option<String>,
    user_id: Option<String>,
    jti: Option<String>,
    is_admin: bool,
    exp: Option<i64>,
    iat: Option<i64>,
    device_id: Option<String>,
}

impl ClaimsBuilder {
    pub fn new() -> Self {
        Self { sub: None, user_id: None, jti: None, is_admin: false, exp: None, iat: None, device_id: None }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn sub(mut self, sub: impl Into<String>) -> Self {
        self.sub = Some(sub.into());
        self
    }

    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn jti(mut self, jti: impl Into<String>) -> Self {
        self.jti = Some(jti.into());
        self
    }

    pub fn is_admin(mut self, is_admin: bool) -> Self {
        self.is_admin = is_admin;
        self
    }

    pub fn exp(mut self, exp: i64) -> Self {
        self.exp = Some(exp);
        self
    }

    pub fn iat(mut self, iat: i64) -> Self {
        self.iat = Some(iat);
        self
    }

    pub fn device_id(mut self, device_id: Option<String>) -> Self {
        self.device_id = device_id;
        self
    }

    /// Build the `Claims`, auto-generating `jti` and `iat` if not set.
    /// Panics if `sub` or `exp` are missing.
    pub fn build(self) -> Claims {
        let now = chrono::Utc::now().timestamp();
        let sub = self.sub.expect("ClaimsBuilder: sub is required");
        Claims {
            sub: sub.clone(),
            user_id: self.user_id.unwrap_or(sub),
            jti: self.jti.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            is_admin: self.is_admin,
            exp: self.exp.expect("ClaimsBuilder: exp is required"),
            iat: self.iat.unwrap_or(now),
            device_id: self.device_id,
        }
    }
}

impl Default for ClaimsBuilder {
    fn default() -> Self {
        Self::new()
    }
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
        Self::new_with_lifetime(pool, cache, metrics, security, server_name, security.expiry_time)
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
