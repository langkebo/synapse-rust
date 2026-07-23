//! Pre-positioned Mock adapter for the services layer.
//!
//! This module aggregates in-memory test doubles for service-layer
//! dependencies. It re-exports the storage-layer fakes (so service
//! tests have a single import surface) and provides [`TestSyncContext`]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::type_complexity)]
//! for exercising sync-adjacent logic without a database.
//!
//! See `.claude/skills/tdd-rust/SKILL.md` §4 and
//! `.trae/documents/TDD落地执行清单.md` Phase 3 for the extension plan.

pub use synapse_storage::test_mocks::{
    seed_locked_users, shared_fake_user_store, FakeUserStore, InMemoryAccessTokenStore, InMemoryAdminMediaStore,
    InMemoryBackgroundUpdateStore, InMemoryDeviceListStore, InMemoryEventStore, InMemoryMemberStore,
    InMemoryOidcUserMappingStore, InMemoryRateLimitStore, InMemoryRefreshTokenStore, InMemoryRelationsStore,
    InMemoryRoomStore, InMemoryRoomTagStore, InMemoryThreepidStore, SharedFakeUserStore,
};

use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use std::sync::RwLock;

use async_trait::async_trait;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::event::{EventReader, EventWriter};
use synapse_storage::User;

// ── Focused Auth Fakes ───────────────────────────────────────────────
// These implement the three focused auth traits (TokenAuth, CredentialAuth,
// RoomAuth) for tests that only need a subset of auth methods.

/// Configurable fake for [`crate::auth::TokenAuth`].
///
/// Default: all methods return errors. Use [`Self::with_validate_token_ok`]
/// to configure token validation.
pub struct FakeTokenAuth {
    validate_token_response: RwLock<Option<ApiResult<(String, Option<String>, bool, bool, bool)>>>,
    token_expiry_value: RwLock<i64>,
}

impl FakeTokenAuth {
    pub fn new() -> Self {
        Self { validate_token_response: RwLock::new(None), token_expiry_value: RwLock::new(3_600_000) }
    }

    pub fn with_validate_token_ok(self, result: (String, Option<String>, bool, bool, bool)) -> Self {
        *self.validate_token_response.write().unwrap() = Some(Ok(result));
        self
    }

    pub fn with_token_expiry(self, expiry: i64) -> Self {
        *self.token_expiry_value.write().unwrap() = expiry;
        self
    }
}

impl Default for FakeTokenAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::auth::TokenAuth for FakeTokenAuth {
    async fn validate_token(&self, _token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)> {
        self.validate_token_response
            .read()
            .unwrap()
            .clone()
            .unwrap_or(Err(ApiError::unauthorized("mock token_auth: validate_token not configured")))
    }

    async fn generate_access_token(&self, _user_id: &str, _device_id: &str, _admin: bool) -> ApiResult<String> {
        Err(ApiError::unauthorized("mock token_auth: generate_access_token not configured"))
    }

    async fn generate_refresh_token(&self, _user_id: &str, _device_id: &str) -> ApiResult<String> {
        Err(ApiError::unauthorized("mock token_auth: generate_refresh_token not configured"))
    }

    async fn refresh_token(&self, _refresh_token: &str) -> ApiResult<(String, String, String)> {
        Err(ApiError::unauthorized("mock token_auth: refresh_token not configured"))
    }

    async fn logout(&self, _access_token: &str, _device_id: Option<&str>) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock token_auth: logout not configured"))
    }

    async fn logout_all(&self, _user_id: &str) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock token_auth: logout_all not configured"))
    }

    async fn revoke_device(&self, _user_id: &str, _device_id: &str) -> ApiResult<u64> {
        Err(ApiError::unauthorized("mock token_auth: revoke_device not configured"))
    }

    async fn revoke_devices(&self, _user_id: &str, _device_ids: &[String]) -> ApiResult<u64> {
        Err(ApiError::unauthorized("mock token_auth: revoke_devices not configured"))
    }

    fn token_expiry(&self) -> i64 {
        *self.token_expiry_value.read().unwrap()
    }
}

/// Stub fake for [`crate::auth::CredentialAuth`].
///
/// All methods return errors by default.
pub struct FakeCredentialAuth;

impl FakeCredentialAuth {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FakeCredentialAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::auth::CredentialAuth for FakeCredentialAuth {
    async fn login(
        &self,
        _username: &str,
        _password: &str,
        _device_id: Option<&str>,
        _initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        Err(ApiError::unauthorized("mock credential_auth: login not configured"))
    }

    async fn register(
        &self,
        _username: &str,
        _password: &str,
        _admin: bool,
        _displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        Err(ApiError::unauthorized("mock credential_auth: register not configured"))
    }

    async fn register_with_device_name(
        &self,
        _username: &str,
        _password: &str,
        _admin: bool,
        _displayname: Option<&str>,
        _initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        Err(ApiError::unauthorized("mock credential_auth: register_with_device_name not configured"))
    }

    async fn change_password(
        &self,
        _user_id: &str,
        _current_password: Option<&str>,
        _new_password: &str,
        _current_device_id: Option<&str>,
    ) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock credential_auth: change_password not configured"))
    }

    async fn deactivate_user(&self, _user_id: &str) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock credential_auth: deactivate_user not configured"))
    }

    async fn verify_user_credentials(&self, _user_id: &str, _password: &str) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock credential_auth: verify_user_credentials not configured"))
    }

    async fn register_guest_account(&self) -> ApiResult<(User, String, String)> {
        Err(ApiError::unauthorized("mock credential_auth: register_guest_account not configured"))
    }

    async fn require_guest_user(&self, _user_id: &str) -> ApiResult<User> {
        Err(ApiError::unauthorized("mock credential_auth: require_guest_user not configured"))
    }

    async fn upgrade_guest_account(
        &self,
        _user_id: &str,
        _device_id: Option<&str>,
        _username: &str,
        _password: &str,
    ) -> ApiResult<String> {
        Err(ApiError::unauthorized("mock credential_auth: upgrade_guest_account not configured"))
    }

    fn generate_email_verification_token(&self) -> ApiResult<String> {
        Ok("mock-email-token".to_string())
    }
}

/// Stub fake for [`crate::auth::RoomAuth`].
///
/// Permissive by default — verification methods return `Ok(())`.
pub struct FakeRoomAuth;

impl FakeRoomAuth {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FakeRoomAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::auth::RoomAuth for FakeRoomAuth {
    async fn verify_message_event_write(&self, _room_id: &str, _user_id: &str, _event_type: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn verify_state_event_write(&self, _room_id: &str, _user_id: &str, _event_type: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn verify_power_levels_change(
        &self,
        _room_id: &str,
        _user_id: &str,
        _new_content: &serde_json::Value,
    ) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock room_auth: verify_power_levels_change not configured"))
    }

    async fn verify_room_moderator(&self, _room_id: &str, _user_id: &str) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock room_auth: verify_room_moderator not configured"))
    }

    async fn verify_room_admin(&self, _room_id: &str, _user_id: &str) -> ApiResult<()> {
        Err(ApiError::unauthorized("mock room_auth: verify_room_admin not configured"))
    }

    async fn can_kick_user(&self, _room_id: &str, _actor_user_id: &str, _target_user_id: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn can_ban_user(&self, _room_id: &str, _actor_user_id: &str, _target_user_id: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn can_unban_user(&self, _room_id: &str, _actor_user_id: &str, _target_user_id: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn can_invite_user(&self, _room_id: &str, _actor_user_id: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn can_redact_event(&self, _room_id: &str, _actor_user_id: &str, _event_sender_id: &str) -> ApiResult<()> {
        Ok(())
    }
}

// ── TestSyncContext ──────────────────────────────────────────────────

/// Bundles all in-memory storage backends for sync-adjacent unit tests.
///
/// Construct via [`TestSyncContext::new()`] and seed data through the
/// individual store accessors. Each `InMemory*` store is `Clone`, so
/// individual stores can be passed to helper functions independently.
///
/// # Example
///
/// ```no_run
/// use synapse_services::test_mocks::TestSyncContext;
///
/// async fn example() {
///     let ctx = TestSyncContext::new();
///     ctx.room_store.create_room("!r:example.com", "@alice:example.com", "invite", "1", false).await.unwrap();
///     ctx.member_store.add_member("!r:example.com", "@alice:example.com", "join", None).await.unwrap();
///     assert!(ctx.member_store.is_member("!r:example.com", "@alice:example.com").await.unwrap());
/// }
/// ```
#[derive(Clone, Default)]
pub struct TestSyncContext {
    pub room_store: InMemoryRoomStore,
    pub event_store: InMemoryEventStore,
    pub member_store: InMemoryMemberStore,
    pub user_store: SharedFakeUserStore,
}

impl TestSyncContext {
    pub fn new() -> Self {
        Self {
            room_store: InMemoryRoomStore::new(),
            event_store: InMemoryEventStore::new(),
            member_store: InMemoryMemberStore::new(),
            user_store: shared_fake_user_store(),
        }
    }
}

// ── MockSyncServiceDepsBuilder ───────────────────────────────────────

/// Builder for [`crate::sync_service::SyncServiceDeps`] with in-memory
/// storage backends via `Arc<dyn EventReader>` and `Arc<dyn EventWriter>`
/// trait objects.
///
/// Use [`Self::with_event_reader`] and [`Self::with_event_writer`] to inject
/// an [`InMemoryEventStore`] (which implements both traits).
/// For full sync integration tests, use the test DB pool helpers in
/// [`crate::test_utils`].
#[derive(Default)]
pub struct MockSyncServiceDepsBuilder {
    event_reader: Option<Arc<dyn EventReader>>,
    event_writer: Option<Arc<dyn EventWriter>>,
}

impl MockSyncServiceDepsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts an `Arc<dyn EventReader>` for injection into the builder.
    pub fn with_event_reader(mut self, reader: Arc<dyn EventReader>) -> Self {
        self.event_reader = Some(reader);
        self
    }

    /// Accepts an `Arc<dyn EventWriter>` for injection into the builder.
    pub fn with_event_writer(mut self, writer: Arc<dyn EventWriter>) -> Self {
        self.event_writer = Some(writer);
        self
    }

    /// Constructs a [`TestSyncContext`] with all in-memory stores.
    /// This is the recommended path for unit tests that need storage
    /// backends without a real database.
    pub fn build_context(&self) -> TestSyncContext {
        TestSyncContext::new()
    }

    /// Returns a [`SharedFakeUserStore`] for injection into services
    /// that accept `Arc<dyn UserStore>`.
    pub fn with_fake_user_store(&self) -> SharedFakeUserStore {
        shared_fake_user_store()
    }
}

// =============================================================================
// InMemoryRegistrationTokenService
// =============================================================================

use crate::registration_token_service::RegistrationTokenApi;
use synapse_storage::registration_token::{
    CreateRegistrationTokenRequest, RegistrationToken, UpdateRegistrationTokenRequest,
};

/// In-memory test double for [`RegistrationTokenApi`].
#[derive(Clone, Default)]
pub struct InMemoryRegistrationTokenService {
    tokens: Arc<tokio::sync::RwLock<HashMap<String, RegistrationToken>>>,
    next_id: Arc<tokio::sync::RwLock<i64>>,
}

impl InMemoryRegistrationTokenService {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl RegistrationTokenApi for InMemoryRegistrationTokenService {
    async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, ApiError> {
        let id = {
            let mut n = self.next_id.write().await;
            *n += 1;
            *n
        };
        let token_str = request.token.unwrap_or_else(|| format!("auto_token_{id}"));
        let now = current_timestamp_millis();
        let token = RegistrationToken {
            id,
            token: token_str.clone(),
            token_type: request.token_type.unwrap_or_else(|| "single_use".to_string()),
            description: request.description,
            max_uses: request.max_uses.unwrap_or(0),
            uses_count: 0,
            is_used: false,
            is_enabled: true,
            expires_at: request.expires_at,
            created_by: request.created_by,
            created_ts: now,
            updated_ts: Some(now),
            last_used_ts: None,
            allowed_email_domains: request.allowed_email_domains,
            allowed_user_ids: request.allowed_user_ids,
            auto_join_rooms: request.auto_join_rooms,
            display_name: request.display_name,
            email: request.email,
        };
        self.tokens.write().await.insert(token_str, token.clone());
        Ok(token)
    }

    async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, ApiError> {
        Ok(self.tokens.read().await.get(token).cloned())
    }

    async fn delete_token(&self, id: i64) -> Result<(), ApiError> {
        self.tokens.write().await.retain(|_, t| t.id != id);
        Ok(())
    }

    async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, ApiError> {
        let mut tokens = self.tokens.write().await;
        for t in tokens.values_mut() {
            if t.id == id {
                if let Some(max_uses) = request.max_uses {
                    t.max_uses = max_uses;
                }
                if request.expires_at.is_some() {
                    t.expires_at = request.expires_at;
                }
                if let Some(is_enabled) = request.is_enabled {
                    t.is_enabled = is_enabled;
                }
                return Ok(t.clone());
            }
        }
        Err(ApiError::not_found("Token not found".to_string()))
    }
}

// =============================================================================
// Extension TODOs (tracked in 执行清单 Phase 3)
// =============================================================================

// SYNC-4 (DONE): SyncServiceDeps fields → Arc<dyn Trait>.
// SYNC-5 (DONE): FakeTokenAuth with configurable validate_token for auth-gated tests.

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::user::UserStore;

    #[tokio::test]
    async fn fake_user_store_reexported_for_service_injection() {
        let store = MockSyncServiceDepsBuilder::new().with_fake_user_store();
        let _trait_ref: Arc<dyn UserStore> = store.clone();
        assert!(!store.is_user_locked("@nobody:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn build_context_returns_all_stores() {
        let ctx = MockSyncServiceDepsBuilder::new().build_context();
        // All stores are usable
        ctx.room_store.create_room("!r:example.com", "@alice:example.com", "invite", "1", true).await.unwrap();
        ctx.member_store.add_member("!r:example.com", "@alice:example.com", "join", None).await.unwrap();
        let params = synapse_storage::event::CreateEventParams {
            event_id: "$ev1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.message".into(),
            content: serde_json::json!({"body": "hello"}),
            state_key: None,
            origin_server_ts: 1_700_000_000_000,
            redacts: None,
        };
        ctx.event_store.create_event(params).await.unwrap();

        assert!(ctx.room_store.room_exists("!r:example.com").await.unwrap());
        assert!(ctx.member_store.is_member("!r:example.com", "@alice:example.com").await.unwrap());
        assert_eq!(ctx.event_store.count_room_events("!r:example.com").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_sync_context_default_is_empty() {
        let ctx = TestSyncContext::default();
        assert!(ctx.room_store.get_room("!nonexistent:example.com").await.unwrap().is_none());
        assert!(ctx.member_store.get_member("!r:example.com", "@alice:example.com").await.unwrap().is_none());
        assert!(ctx.event_store.get_event("$nonexistent").await.unwrap().is_none());
    }

    /// RED → GREEN tracer bullet for SYNC-5: FakeTokenAuth should be
    /// injectable as `Arc<dyn TokenAuth>` and return pre-configured
    /// responses for validate_token.
    #[tokio::test]
    async fn fake_auth_validate_token_returns_configured_response() {
        use crate::auth::TokenAuth;

        let auth = FakeTokenAuth::new().with_validate_token_ok((
            "@alice:example.com".into(),
            Some("DEV1".into()),
            false,
            false,
            true,
        ));

        let trait_obj: Arc<dyn TokenAuth> = Arc::new(auth);
        let result = trait_obj.validate_token("fake-token").await.unwrap();
        assert_eq!(result.0, "@alice:example.com");
        assert_eq!(result.1, Some("DEV1".to_string()));
        assert!(!result.2); // not guest
        assert!(!result.3); // not admin
        assert!(result.4); // is valid
    }

    #[tokio::test]
    async fn fake_auth_returns_error_when_not_configured() {
        use crate::auth::TokenAuth;

        let auth: Arc<dyn TokenAuth> = Arc::new(FakeTokenAuth::new());
        let result = auth.validate_token("any-token").await;
        assert!(result.is_err());
    }

    #[test]
    fn builder_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MockSyncServiceDepsBuilder>();
        assert_send_sync::<TestSyncContext>();
    }

    /// RED — tracer bullet for SYNC-4: the builder should accept an
    /// InMemoryEventStore cast to `Arc<dyn EventReader>` and let tests
    /// retrieve events through the trait object.
    #[tokio::test]
    async fn builder_stores_and_returns_trait_object_event_store() {
        use synapse_storage::event::CreateEventParams;
        use synapse_storage::event::EventWriter;

        let store = Arc::new(synapse_storage::test_mocks::InMemoryEventStore::new());
        let params = CreateEventParams {
            event_id: "$tracer:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.message".into(),
            content: serde_json::json!({"body": "tracer"}),
            state_key: None,
            origin_server_ts: 1_700_000_000_000,
            redacts: None,
        };
        <synapse_storage::test_mocks::InMemoryEventStore as EventWriter>::create_event(&store, params, None)
            .await
            .unwrap();

        let _builder = MockSyncServiceDepsBuilder::new()
            .with_event_reader(store.clone() as Arc<dyn EventReader>)
            .with_event_writer(store.clone() as Arc<dyn EventWriter>);
    }
}
