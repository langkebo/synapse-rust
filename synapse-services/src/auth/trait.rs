use async_trait::async_trait;
use std::sync::Arc;
use synapse_common::validation::Validator;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::User;

/// Abstract interface for authentication, token management, power-level
/// authorization, and guest-account lifecycle. The trait captures every public
/// method on [`AuthService`](crate::auth::AuthService) so that callers (routes,
/// other services, tests) can depend on `Arc<dyn Auth>` instead of the
/// concrete struct.
#[async_trait]
pub trait Auth: Send + Sync {
    // ── Token / session ──────────────────────────────────────────────────

    async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)>;

    async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    async fn register_with_device_name(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    async fn generate_access_token(&self, user_id: &str, device_id: &str, admin: bool) -> ApiResult<String>;

    async fn generate_refresh_token(&self, user_id: &str, device_id: &str) -> ApiResult<String>;

    async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()>;

    async fn logout_all(&self, user_id: &str) -> ApiResult<()>;

    async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)>;

    // ── Account ──────────────────────────────────────────────────────────

    async fn change_password(
        &self,
        user_id: &str,
        current_password: Option<&str>,
        new_password: &str,
        current_device_id: Option<&str>,
    ) -> ApiResult<()>;

    async fn deactivate_user(&self, user_id: &str) -> ApiResult<()>;

    async fn verify_user_credentials(&self, user_id: &str, password: &str) -> ApiResult<()>;

    async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64>;

    async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64>;

    async fn hash_password_for_storage(&self, password: &str) -> Result<String, ApiError>;

    /// Generate a cryptographically-random email-verification token.
    /// This is a **synchronous** method because it only calls a local RNG.
    fn generate_email_verification_token(&self) -> Result<String, Box<dyn std::error::Error>>;

    // ── Power levels ─────────────────────────────────────────────────────

    async fn get_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64>;

    async fn get_required_state_event_power_level(&self, room_id: &str, event_type: &str) -> ApiResult<i64>;

    async fn get_required_message_event_power_level(&self, room_id: &str, event_type: &str) -> ApiResult<i64>;

    async fn verify_message_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()>;

    async fn verify_state_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()>;

    async fn verify_power_levels_change(
        &self,
        room_id: &str,
        user_id: &str,
        new_content: &serde_json::Value,
    ) -> ApiResult<()>;

    async fn verify_room_moderator(&self, room_id: &str, user_id: &str) -> ApiResult<()>;

    async fn verify_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()>;

    async fn can_kick_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()>;

    async fn can_ban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()>;

    async fn can_unban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()>;

    async fn can_invite_user(&self, room_id: &str, actor_user_id: &str) -> ApiResult<()>;

    async fn can_redact_event(&self, room_id: &str, actor_user_id: &str, event_sender_id: &str) -> ApiResult<()>;

    // ── Guest accounts ───────────────────────────────────────────────────

    async fn register_guest_account(&self) -> ApiResult<(User, String, String)>;

    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User>;

    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String>;

    // ── Configuration accessors ──────────────────────────────────────────

    fn token_expiry(&self) -> i64;

    fn server_name(&self) -> &str;

    fn validator(&self) -> &Arc<Validator>;
}
