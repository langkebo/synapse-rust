mod account;
/// The `credential_auth` module.
pub mod credential_auth;
mod login;
/// The `mas_rest_client` module.
pub mod mas_rest_client;
/// The `mas_validator` module.
pub mod mas_validator;
/// The `password_policy` module.
pub mod password_policy;
mod power_levels;
mod register;
/// The `room_auth` module.
pub mod room_auth;
mod session;
/// The `test_harness` module.
#[cfg(test)]
pub(crate) mod test_harness;
#[cfg(test)]
mod tests;
mod token;
/// The `token_auth` module.
pub mod token_auth;

use rand::RngCore;
use std::sync::Arc;
use synapse_cache::*;
use synapse_common::config::SecurityConfig;
use synapse_common::metrics::MetricsCollector;
use synapse_common::validation::Validator;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::*;

pub use credential_auth::CredentialAuth;
pub use mas_validator::{MasTokenClaims, MasTokenValidator, OidcMasTokenValidator};
pub use room_auth::RoomAuth;
pub use token_auth::TokenAuth;

pub use password_policy::{PasswordPolicy, PasswordPolicyService, PasswordValidationResult};
pub use synapse_common::claims::{Claims, ClaimsBuilder};

use crate::UserService;

const TOKEN_CACHE_TTL_SECS: u64 = 300; // 5 min - must be short to respect revocation
const USER_ACTIVE_CACHE_TTL_SECS: u64 = 60;
const ADMIN_CACHE_TTL_SECS: u64 = 60;
/// S4: 撤销/黑名单检查结果的缓存 TTL（秒）。所有撤销写入路径
/// （logout / logout_all / change_password / deactivate_user / revoke_device(s)）
/// 都会主动失效对应标记，该 TTL 仅作为异常路径的兜底上限。
const REVOCATION_CHECK_CACHE_TTL_SECS: u64 = 30;
const DEFAULT_POWER_LEVEL: i64 = 50;

/// The `AuthService` struct.
#[derive(Clone)]
pub struct AuthService {
    /// The `user_storage` field.
    pub user_storage: Arc<dyn UserStore>,
    /// The `user_service` field.
    pub user_service: Arc<UserService>,
    /// The `device_storage` field.
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    /// The `token_storage` field.
    pub token_storage: Arc<dyn AccessTokenStoreApi>,
    /// The `refresh_token_storage` field.
    pub refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>,
    /// The `room_storage` field.
    pub room_storage: RoomStorage,
    /// The `member_storage` field.
    pub member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    /// The `event_reader` field.
    pub event_reader: Arc<dyn synapse_storage::event::EventReader>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `metrics` field.
    pub metrics: Arc<MetricsCollector>,
    /// The `validator` field.
    pub validator: Arc<Validator>,
    /// The `jwt_secret` field.
    pub jwt_secret: Vec<u8>,
    /// The `token_expiry` field.
    pub token_expiry: i64,
    /// The `refresh_token_expiry` field.
    pub refresh_token_expiry: i64,
    /// The `server_name` field.
    pub server_name: String,
    /// The `argon2_m_cost` field.
    pub argon2_m_cost: u32,
    /// The `argon2_t_cost` field.
    pub argon2_t_cost: u32,
    /// The `argon2_p_cost` field.
    pub argon2_p_cost: u32,
    /// The `allow_legacy_hashes` field.
    pub allow_legacy_hashes: bool,
    /// The `login_failure_lockout_threshold` field.
    pub login_failure_lockout_threshold: u32,
    /// The `login_lockout_duration_seconds` field.
    pub login_lockout_duration_seconds: u64,
    /// MSC3861: Optional MAS token validator. When set (MAS deployed),
    /// `validate_token` first tries the MAS path for RS256/ES256/EdDSA
    /// JWTs before falling back to the local HS256 path. `None` preserves
    /// the legacy behavior (local tokens only).
    pub mas_validator: Option<Arc<dyn MasTokenValidator>>,
    /// C1: Tamper-evident audit storage. When set, security events
    /// (login success/failure, account lockout, password change, token
    /// revocation) are persisted to the `audit_events` table in addition
    /// to tracing logs. `None` preserves the legacy tracing-only path
    /// (tests / pre-wiring builds).
    pub audit_storage: Option<Arc<dyn synapse_storage::audit::AuditEventStoreApi>>,
}

impl AuthService {
    /// See [`new`].
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        metrics: Arc<MetricsCollector>,
        security: &SecurityConfig,
        server_name: &str,
    ) -> Self {
        // S23: Test convenience — creates a local UserService for test isolation.
        // Production code must use new_with_lifetime() with a shared UserService.
        let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, cache.clone()));
        let user_service = Arc::new(UserService::new(user_storage.clone()));
        let device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi> = Arc::new(DeviceStorage::new(pool));
        let token_storage: Arc<dyn AccessTokenStoreApi> = Arc::new(AccessTokenStorage::new(pool));
        let refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi> =
            Arc::new(synapse_storage::refresh_token::RefreshTokenStorage::new(pool));
        Self::new_with_lifetime(
            pool,
            cache,
            metrics,
            security,
            server_name,
            security.expiry_time,
            user_service,
            user_storage,
            device_storage,
            token_storage,
            refresh_token_storage,
        )
    }

    /// See [`new_with_lifetime`].
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_lifetime(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        metrics: Arc<MetricsCollector>,
        security: &SecurityConfig,
        server_name: &str,
        access_token_lifetime: i64,
        user_service: Arc<UserService>,
        user_storage: Arc<dyn UserStore>,
        device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
        token_storage: Arc<dyn AccessTokenStoreApi>,
        refresh_token_storage: Arc<dyn synapse_storage::refresh_token::RefreshTokenStoreApi>,
    ) -> Self {
        let server_name_for_storage = server_name.to_string();
        Self {
            user_service,
            user_storage,
            device_storage,
            token_storage,
            refresh_token_storage,
            // These remain internal — they are read-only in auth context.
            // Injecting them would require changes to 4+ wiring files and
            // is tracked as a follow-up task.
            room_storage: RoomStorage::new(pool),
            member_storage: Arc::new(RoomMemberStorage::new(pool, &server_name_for_storage)),
            event_reader: Arc::new(EventStorage::new(pool, server_name_for_storage.clone())),
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
            mas_validator: None,
            audit_storage: None,
        }
    }

    /// MSC3861: Attach a MAS token validator. When set, `validate_token`
    /// will first attempt MAS (RS256/ES256/EdDSA JWT) validation before
    /// falling back to the local HS256 path. Call this when the server
    /// is configured to use an external MAS / OIDC provider for auth.
    pub fn with_mas_validator(mut self, validator: Arc<dyn MasTokenValidator>) -> Self {
        self.mas_validator = Some(validator);
        self
    }

    /// C1: Attach tamper-evident audit storage. When set, auth security events
    /// are persisted to the `audit_events` table. Production wiring calls this.
    /// Tests keep `None` (tracing-only path).
    pub fn with_audit_storage(self, storage: Arc<dyn synapse_storage::audit::AuditEventStoreApi>) -> Self {
        Self { audit_storage: Some(storage), ..self }
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

// ── Guest account inherent methods ──────────────────────────────────
// Extracted as inherent methods so both Auth and CredentialAuth trait
// impls can delegate without ambiguity.

impl AuthService {
    async fn register_guest_account(&self) -> ApiResult<(User, String, String)> {
        let guest_num = rand::random::<u64>();
        let username = format!("guest_{guest_num}");
        let user_id = format!("@{}:{}", username, self.server_name);
        let device_id = format!("guest_device_{guest_num}");

        let user = self.user_storage.create_user(&user_id, &username, None, false).await.map_err(|e| {
            if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
                ApiError::user_in_use("Username already exists".to_string())
            } else {
                ApiError::internal_with_cause("Failed to create guest user", e)
            }
        })?;

        self.user_storage
            .set_guest_status(&user.user_id, true)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to mark guest user", e))?;

        self.device_storage
            .create_device(&device_id, &user.user_id, Some("Guest Device"))
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create device", e))?;

        let access_token = self
            .generate_access_token(&user.user_id, &device_id, false)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate guest token", e))?;

        Ok((user, device_id, access_token))
    }

    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User> {
        let user = self.user_service.get_user_or_not_found(user_id).await?;

        if !user.is_guest {
            return Err(ApiError::forbidden("User is not a guest".to_string()));
        }

        Ok(user)
    }

    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String> {
        self.validator.validate_username(username)?;
        self.validator.validate_password(password)?;

        let guest_user = self.require_guest_user(user_id).await?;
        let existing = self.user_service.get_user_by_username(username).await?;

        if existing.as_ref().is_some_and(|user| user.user_id != user_id) {
            return Err(ApiError::conflict("Username already exists".to_string()));
        }

        let password_hash = self.hash_password_for_storage(password).await?;
        self.user_storage
            .upgrade_guest_account(&guest_user.user_id, username, &password_hash)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to upgrade account", e))?;

        self.generate_access_token(&guest_user.user_id, device_id.unwrap_or(""), false)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate token", e))
    }
}

// ── TokenAuth trait delegation ────────────────────────────────────────

#[async_trait::async_trait]
impl crate::auth::TokenAuth for AuthService {
    async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)> {
        self.validate_token(token).await
    }

    async fn generate_access_token(&self, user_id: &str, device_id: &str, admin: bool) -> ApiResult<String> {
        self.generate_access_token(user_id, device_id, admin).await
    }

    async fn generate_refresh_token(&self, user_id: &str, device_id: &str, access_token: &str) -> ApiResult<String> {
        self.generate_refresh_token(user_id, device_id, access_token).await
    }

    async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)> {
        self.refresh_token(refresh_token).await
    }

    async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()> {
        self.logout(access_token, device_id).await
    }

    async fn logout_all(&self, user_id: &str) -> ApiResult<()> {
        self.logout_all(user_id).await
    }

    async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64> {
        self.revoke_device(user_id, device_id).await
    }

    async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64> {
        self.revoke_devices(user_id, device_ids).await
    }

    fn token_expiry(&self) -> i64 {
        self.token_expiry
    }
}

// ── CredentialAuth trait delegation ───────────────────────────────────

#[async_trait::async_trait]
impl crate::auth::CredentialAuth for AuthService {
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

    async fn change_password(
        &self,
        user_id: &str,
        current_password: Option<&str>,
        new_password: &str,
        current_device_id: Option<&str>,
        logout_devices: bool,
    ) -> ApiResult<()> {
        AuthService::change_password(self, user_id, current_password, new_password, current_device_id, logout_devices)
            .await
    }

    async fn deactivate_user(&self, user_id: &str) -> ApiResult<()> {
        self.deactivate_user(user_id).await
    }

    async fn verify_user_credentials(&self, user_id: &str, password: &str) -> ApiResult<()> {
        self.verify_user_credentials(user_id, password).await
    }

    async fn register_guest_account(&self) -> ApiResult<(User, String, String)> {
        self.register_guest_account().await
    }

    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User> {
        self.require_guest_user(user_id).await
    }

    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String> {
        self.upgrade_guest_account(user_id, device_id, username, password).await
    }

    fn generate_email_verification_token(&self) -> ApiResult<String> {
        self.generate_email_verification_token()
    }
}

// ── RoomAuth trait delegation ─────────────────────────────────────────

#[async_trait::async_trait]
impl crate::auth::RoomAuth for AuthService {
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
}
