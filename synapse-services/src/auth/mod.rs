mod account;
pub mod guest;
mod login;
pub mod password_policy;
mod power_levels;
mod register;
mod session;
#[cfg(test)]
mod tests;
mod token;
pub mod r#trait;

use rand::RngCore;
use std::sync::Arc;
use synapse_cache::*;
use synapse_common::config::SecurityConfig;
use synapse_common::metrics::MetricsCollector;
use synapse_common::validation::Validator;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::refresh_token::repository::RefreshTokenRepository;
use synapse_storage::*;

pub use r#trait::Auth;

// Re-export shared claims types from synapse-common for backward compatibility.
pub use guest::GuestAuthExt;
pub use password_policy::{PasswordPolicy, PasswordPolicyService, PasswordValidationResult};
pub use synapse_common::claims::{Claims, ClaimsBuilder};

const TOKEN_CACHE_TTL_SECS: u64 = 300; // 5 min - must be short to respect revocation
const USER_ACTIVE_CACHE_TTL_SECS: u64 = 60;
const ADMIN_CACHE_TTL_SECS: u64 = 60;
const DEFAULT_POWER_LEVEL: i64 = 50;

#[derive(Clone)]
pub struct AuthService {
    pub user_storage: Arc<dyn UserStore>,
    pub device_storage: Arc<dyn DeviceRepository>,
    pub token_storage: AccessTokenStorage,
    pub refresh_token_storage: Arc<dyn RefreshTokenRepository>,
    pub room_storage: RoomStorage,
    pub member_storage: Arc<dyn RoomMemberRepository>,
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
            user_storage: Arc::new(UserStorage::new(pool, cache.clone())),
            device_storage: Arc::new(DeviceStorage::new(pool)),
            token_storage: AccessTokenStorage::new(pool),
            refresh_token_storage: Arc::new(synapse_storage::refresh_token::RefreshTokenStorage::new(pool)),
            room_storage: RoomStorage::new(pool),
            member_storage: Arc::new(RoomMemberStorage::new(pool, &server_name_for_storage)),
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
    let mut rng = rand::rng();
    let mut token = String::with_capacity(length);
    for _ in 0..length {
        let idx = (rng.next_u32() as usize) % CHARSET.len();
        token.push(CHARSET[idx] as char);
    }
    token
}

// ── Auth trait delegation impl ────────────────────────────────────────

#[async_trait::async_trait]
impl Auth for AuthService {
    // ── Token / session ──────────────────────────────────────────────

    async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)> {
        self.validate_token(token).await
    }

    async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        self.login(username, password, device_id, initial_display_name).await
    }

    async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        self.register(username, password, admin, displayname).await
    }

    async fn register_with_device_name(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        self.register_with_device_name(username, password, admin, displayname, initial_device_display_name).await
    }

    async fn generate_access_token(&self, user_id: &str, device_id: &str, admin: bool) -> ApiResult<String> {
        self.generate_access_token(user_id, device_id, admin).await
    }

    async fn generate_refresh_token(&self, user_id: &str, device_id: &str) -> ApiResult<String> {
        self.generate_refresh_token(user_id, device_id).await
    }

    async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()> {
        self.logout(access_token, device_id).await
    }

    async fn logout_all(&self, user_id: &str) -> ApiResult<()> {
        self.logout_all(user_id).await
    }

    async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)> {
        self.refresh_token(refresh_token).await
    }

    // ── Account ──────────────────────────────────────────────────────

    async fn change_password(
        &self,
        user_id: &str,
        current_password: Option<&str>,
        new_password: &str,
        current_device_id: Option<&str>,
    ) -> ApiResult<()> {
        self.change_password(user_id, current_password, new_password, current_device_id).await
    }

    async fn deactivate_user(&self, user_id: &str) -> ApiResult<()> {
        self.deactivate_user(user_id).await
    }

    async fn verify_user_credentials(&self, user_id: &str, password: &str) -> ApiResult<()> {
        self.verify_user_credentials(user_id, password).await
    }

    async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64> {
        self.revoke_device(user_id, device_id).await
    }

    async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64> {
        self.revoke_devices(user_id, device_ids).await
    }

    async fn hash_password_for_storage(&self, password: &str) -> Result<String, ApiError> {
        self.hash_password_for_storage(password).await
    }

    fn generate_email_verification_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        self.generate_email_verification_token()
    }

    // ── Power levels ─────────────────────────────────────────────────

    async fn get_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        self.get_user_power_level(room_id, user_id).await
    }

    async fn get_required_state_event_power_level(&self, room_id: &str, event_type: &str) -> ApiResult<i64> {
        self.get_required_state_event_power_level(room_id, event_type).await
    }

    async fn get_required_message_event_power_level(&self, room_id: &str, event_type: &str) -> ApiResult<i64> {
        self.get_required_message_event_power_level(room_id, event_type).await
    }

    async fn verify_message_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()> {
        self.verify_message_event_write(room_id, user_id, event_type).await
    }

    async fn verify_state_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()> {
        self.verify_state_event_write(room_id, user_id, event_type).await
    }

    async fn verify_power_levels_change(
        &self,
        room_id: &str,
        user_id: &str,
        new_content: &serde_json::Value,
    ) -> ApiResult<()> {
        self.verify_power_levels_change(room_id, user_id, new_content).await
    }

    async fn verify_room_moderator(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.verify_room_moderator(room_id, user_id).await
    }

    async fn verify_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.verify_room_admin(room_id, user_id).await
    }

    async fn can_kick_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()> {
        self.can_kick_user(room_id, actor_user_id, target_user_id).await
    }

    async fn can_ban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()> {
        self.can_ban_user(room_id, actor_user_id, target_user_id).await
    }

    async fn can_unban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()> {
        self.can_unban_user(room_id, actor_user_id, target_user_id).await
    }

    async fn can_invite_user(&self, room_id: &str, actor_user_id: &str) -> ApiResult<()> {
        self.can_invite_user(room_id, actor_user_id).await
    }

    async fn can_redact_event(&self, room_id: &str, actor_user_id: &str, event_sender_id: &str) -> ApiResult<()> {
        self.can_redact_event(room_id, actor_user_id, event_sender_id).await
    }

    // ── Guest accounts ───────────────────────────────────────────────

    async fn register_guest_account(&self) -> ApiResult<(User, String, String)> {
        <Self as crate::auth::guest::GuestAuthExt>::register_guest_account(self).await
    }

    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User> {
        <Self as crate::auth::guest::GuestAuthExt>::require_guest_user(self, user_id).await
    }

    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String> {
        <Self as crate::auth::guest::GuestAuthExt>::upgrade_guest_account(self, user_id, device_id, username, password)
            .await
    }

    // ── Configuration accessors ──────────────────────────────────────

    fn token_expiry(&self) -> i64 {
        self.token_expiry
    }

    fn server_name(&self) -> &str {
        &self.server_name
    }

    fn validator(&self) -> &Arc<Validator> {
        &self.validator
    }
}
