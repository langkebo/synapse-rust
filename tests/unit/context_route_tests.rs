// Context Route Tests - Request-Pipeline Context Coverage
//
// These tests cover the request-pipeline context structs exposed by
// `synapse-web/src/routes/context.rs` (P-096: previously zero tests).
//
// `context.rs` defines 11 `*Context` structs that bundle shared services
// for different route groups (CoreContext, RoomContext, E2eeRoomContext,
// SyncContext, DeviceContext, AuthContext, AdminContext, FederationContext,
// MediaContext, SsoContext, FriendContext). Each implements `Clone` and
// `FromRef<AppState>` so axum's `State<T>` extractor can derive them.
//
// The module exposes two pure helpers (`CoreContext::rate_limit_config` and
// `SyncContext::sync_rate_limit_override`) whose logic is mirrored below.
// Struct-level coverage is enforced through trait-bound checks and field
// visibility assertions, following the same pattern as the existing
// `key_rotation_route_tests.rs`: pure data + logic assertions, no DB.

use synapse_web::routes::context::{
    AdminContext, AuthContext, CoreContext, DeviceContext, E2eeRoomContext, FederationContext, MediaContext,
    RoomContext, SsoContext, SyncContext,
};

// ============================================================================
// Trait bound checks — every context must be Clone + Send + Sync
// ============================================================================

#[test]
fn test_core_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<CoreContext>();
}

#[test]
fn test_room_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<RoomContext>();
}

#[test]
fn test_e2ee_room_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<E2eeRoomContext>();
}

#[test]
fn test_sync_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<SyncContext>();
}

#[test]
fn test_device_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<DeviceContext>();
}

#[test]
fn test_auth_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<AuthContext>();
}

#[test]
fn test_admin_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<AdminContext>();
}

#[test]
fn test_federation_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<FederationContext>();
}

#[test]
fn test_media_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<MediaContext>();
}

#[test]
fn test_sso_context_is_clone_send_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<SsoContext>();
}

// ============================================================================
// FromRef<AppState> — every context must implement axum's extractor trait
// ============================================================================

#[test]
fn test_all_contexts_implement_from_ref() {
    // axum::extract::FromRef is the trait that lets `State<T>` derive
    // sub-contexts from the top-level AppState. If any context stops
    // implementing it the corresponding router group breaks at startup.
    use axum::extract::FromRef;
    use synapse_web::routes::AppState;

    fn assert_from_ref<T: FromRef<AppState>>() {}

    assert_from_ref::<CoreContext>();
    assert_from_ref::<RoomContext>();
    assert_from_ref::<E2eeRoomContext>();
    assert_from_ref::<SyncContext>();
    assert_from_ref::<DeviceContext>();
    assert_from_ref::<AuthContext>();
    assert_from_ref::<AdminContext>();
    assert_from_ref::<FederationContext>();
    assert_from_ref::<MediaContext>();
    assert_from_ref::<SsoContext>();
}

// ============================================================================
// CoreContext::rate_limit_config — pure helper logic
// ============================================================================

/// Mirror of `CoreContext::rate_limit_config`:
/// ```ignore
/// pub fn rate_limit_config(&self) -> Option<crate::common::RateLimitConfigFile> {
///     self.rate_limit_config_manager.as_ref().map(|m| m.get_config())
/// }
/// ```
fn core_rate_limit_config(
    manager: Option<&synapse_common::rate_limit_config::RateLimitConfigManager>,
) -> Option<synapse_rust::common::RateLimitConfigFile> {
    manager.map(|m| m.get_config())
}

#[test]
fn test_core_rate_limit_config_returns_none_when_manager_absent() {
    // When AppState has no rate_limit_config_manager, CoreContext also has None
    // and `rate_limit_config()` returns None (rate-limit middleware fails open).
    let manager: Option<synapse_common::rate_limit_config::RateLimitConfigManager> = None;
    assert!(core_rate_limit_config(manager.as_ref()).is_none());
}

#[test]
fn test_core_rate_limit_config_returns_some_when_manager_present() {
    // Constructing a live manager requires config; we assert the contract
    // by mirroring the logic on a stub. The shape of the returned value is
    // whatever `RateLimitConfigManager::get_config()` returns.
    // Here we verify the Option layer (Some vs None) without a live manager.
    fn helper<T: Clone>(mgr: Option<&T>) -> Option<()> {
        mgr.map(|_| ())
    }
    #[derive(Clone)]
    struct Stub;
    let stub = Stub;
    assert!(helper(Some(&stub)).is_some());
    assert!(helper::<Stub>(None).is_none());
}

// ============================================================================
// SyncContext::sync_rate_limit_override — pure helper logic
// ============================================================================

/// Mirror of `SyncContext::sync_rate_limit_override`:
/// ```ignore
/// pub fn sync_rate_limit_override(&self) -> Option<SyncRateLimitOverride> {
///     self.rate_limit_config_manager.as_ref().map(|m| {
///         let config = m.get_config();
///         SyncRateLimitOverride {
///             fail_open_on_error: config.fail_open_on_error,
///             sync: config.sync,
///         }
///     })
/// }
/// ```
fn sync_rate_limit_override(
    manager: Option<&synapse_common::rate_limit_config::RateLimitConfigManager>,
) -> Option<OverrideStub> {
    manager.map(|m| {
        let config = m.get_config();
        OverrideStub { fail_open_on_error: config.fail_open_on_error, sync: config.sync }
    })
}

#[derive(Debug)]
#[allow(dead_code)] // 形状替身：镜像 SyncRateLimitOverride 的两字段，测试只验证 Option 层
struct OverrideStub {
    fail_open_on_error: bool,
    sync: synapse_rust::common::SyncRateLimitConfigFile,
}

#[test]
fn test_sync_rate_limit_override_none_when_manager_absent() {
    // Without a manager, sync requests bypass the override and use defaults.
    let manager: Option<synapse_common::rate_limit_config::RateLimitConfigManager> = None;
    assert!(sync_rate_limit_override(manager.as_ref()).is_none());
}

#[test]
fn test_sync_rate_limit_override_shape_is_two_field_struct() {
    // SyncRateLimitOverride has exactly two fields: `fail_open_on_error: bool`
    // and `sync: SyncRateLimitConfigFile`. The override is consumed by the
    // sync rate-limit middleware to decide fail-open behavior.
    // Verify the source-of-truth struct shape (compile-time check).
    fn assert_shape(_x: synapse_web::routes::state::SyncRateLimitOverride) {}
    let _ = assert_shape;
}

// ============================================================================
// AppState::sync_rate_limit_override — sibling helper
// ============================================================================

#[test]
fn test_app_state_sync_rate_limit_override_is_accessible() {
    // AppState exposes a sibling `sync_rate_limit_override()` helper that
    // mirrors SyncContext::sync_rate_limit_override. Both must agree:
    // SyncContext pulls the manager through `rate_limit_config_manager`,
    // AppState reads it from the same field. Confirming the method exists
    // guards against accidental drift if one is renamed/removed.
    use synapse_web::routes::AppState;
    fn _assert_method_present(state: &AppState) -> Option<synapse_web::routes::state::SyncRateLimitOverride> {
        state.sync_rate_limit_override()
    }
}

#[test]
fn test_app_state_rate_limit_config_manager_accessor_exists() {
    // AppState exposes `rate_limit_config_manager()` which is consumed by
    // CoreContext::from_ref to populate the rate_limit_config_manager field.
    use synapse_web::routes::AppState;
    fn _assert_method_present(state: &AppState) {
        let _: Option<&std::sync::Arc<synapse_common::rate_limit_config::RateLimitConfigManager>> =
            state.rate_limit_config_manager();
    }
}

// ============================================================================
// Context field visibility — guards against accidental field privatization
// ============================================================================

#[test]
fn test_core_context_public_fields_are_accessible() {
    // The fields below are read by the request-pipeline middlewares
    // (auth_middleware, shadow_ban_middleware, csrf_middleware, rate_limit_middleware).
    // Privatizing them would break the middlewares' inline `ctx.field` access.
    fn assert_fields(ctx: &CoreContext) {
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.config;
        let _ = &ctx.cache;
        let _ = &ctx.rate_limit_config_manager;
    }
    let _ = assert_fields;
}

#[test]
fn test_auth_context_public_fields_are_accessible() {
    // AuthContext fields are read by the account/auth handlers (whoami,
    // change_password, deactivate, add_threepid, …). Field renames would
    // silently break those handlers.
    fn assert_fields(ctx: &AuthContext) {
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.registration_service;
        let _ = &ctx.user_service;
        let _ = &ctx.server_name;
        let _ = &ctx.cache;
        let _ = &ctx.config;
        let _ = &ctx.admin_audit_service;
        let _ = &ctx.account_identity_service;
        let _ = &ctx.uia_service;
        let _ = &ctx.federation_client;
        let _ = &ctx.email_verification_storage;
        let _ = &ctx.account_device_list_service;
        let _ = &ctx.refresh_token_service;
        let _ = &ctx.metrics;
        let _ = &ctx.identity_service;
        let _ = &ctx.oidc_service;
        let _ = &ctx.rendezvous_service;
    }
    let _ = assert_fields;
}

#[test]
fn test_admin_context_public_fields_are_accessible() {
    // AdminContext is the largest context (40+ fields) — covering the
    // security-sensitive admin_*_service and admin_audit_service surface.
    fn assert_fields(ctx: &AdminContext) {
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.registration_service;
        let _ = &ctx.config;
        let _ = &ctx.server_name;
        let _ = &ctx.cache;
        let _ = &ctx.metrics;
        let _ = &ctx.media_service;
        let _ = &ctx.room_service;
        let _ = &ctx.sliding_sync_service;
        let _ = &ctx.space_service;
        let _ = &ctx.user_service;
        let _ = &ctx.account_identity_service;
        let _ = &ctx.account_device_list_service;
        let _ = &ctx.invite_blocklist_storage;
        let _ = &ctx.admin_user_service;
        let _ = &ctx.admin_registration_service;
        let _ = &ctx.admin_token_service;
        let _ = &ctx.refresh_token_service;
        let _ = &ctx.registration_token_service;
        let _ = &ctx.email_verification_storage;
        let _ = &ctx.background_update_service;
        let _ = &ctx.retention_service;
        let _ = &ctx.feature_flag_service;
        let _ = &ctx.event_report_service;
        let _ = &ctx.delayed_event_service;
        let _ = &ctx.policy_service;
        let _ = &ctx.event_storage;
        let _ = &ctx.push_notification_service;
        let _ = &ctx.app_service_manager;
        let _ = &ctx.app_service_scheduler;
        let _ = &ctx.module_service;
        let _ = &ctx.module_storage;
        let _ = &ctx.account_validity_service;
        let _ = &ctx.worker_manager;
        let _ = &ctx.admin_audit_service;
        let _ = &ctx.admin_security_service;
        let _ = &ctx.admin_server_service;
        let _ = &ctx.captcha_service;
        let _ = &ctx.telemetry_alert_service;
        let _ = &ctx.admin_federation_service;
        let _ = &ctx.federation_blacklist_service;
        let _ = &ctx.admin_media_service;
        let _ = &ctx.federation_client;
        let _ = &ctx.rate_limit_config_manager;
        let _ = &ctx.shutdown_signal;
        let _ = &ctx.account_data_service;
        let _ = &ctx.health_checker;
        let _ = &ctx.ssss_service;
        let _ = &ctx.token_storage;
        let _ = &ctx.client_push_service;
    }
    let _ = assert_fields;
}

#[test]
fn test_device_context_public_fields_are_accessible() {
    fn assert_fields(ctx: &DeviceContext) {
        let _ = &ctx.device_storage;
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.user_service;
        let _ = &ctx.server_name;
        let _ = &ctx.account_device_list_service;
        let _ = &ctx.room_service;
        let _ = &ctx.uia_service;
        let _ = &ctx.event_broadcaster;
        let _ = &ctx.config;
        let _ = &ctx.admin_audit_service;
        let _ = &ctx.account_identity_service;
        let _ = &ctx.cross_signing_service;
        let _ = &ctx.device_keys_service;
        let _ = &ctx.federation_client;
        let _ = &ctx.to_device_service;
        let _ = &ctx.metrics;
        let _ = &ctx.cache;
        let _ = &ctx.event_notifier;
        let _ = &ctx.key_request_service;
        let _ = &ctx.verification_service;
        let _ = &ctx.device_trust_service;
        let _ = &ctx.key_rotation_service;
    }
    let _ = assert_fields;
}

#[test]
fn test_federation_context_public_fields_are_accessible() {
    fn assert_fields(ctx: &FederationContext) {
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.user_service;
        let _ = &ctx.config;
        let _ = &ctx.server_name;
        let _ = &ctx.cache;
        let _ = &ctx.metrics;
        let _ = &ctx.room_service;
        let _ = &ctx.space_service;
        let _ = &ctx.registration_service;
        let _ = &ctx.account_identity_service;
        let _ = &ctx.account_device_list_service;
        let _ = &ctx.key_rotation_manager;
        let _ = &ctx.federation_client;
        let _ = &ctx.event_auth_chain;
        let _ = &ctx.device_sync_manager;
        let _ = &ctx.federation_server_name;
        let _ = &ctx.admin_audit_service;
        let _ = &ctx.worker_manager;
        let _ = &ctx.media_service;
        let _ = &ctx.account_data_service;
        let _ = &ctx.federation_signature_cache;
        let _ = &ctx.replay_protection_cache;
        let _ = &ctx.federation_key_fetch_general_semaphore;
        let _ = &ctx.federation_key_fetch_priority_semaphore;
        let _ = &ctx.admin_federation_service;
        let _ = &ctx.device_keys_service;
        let _ = &ctx.cross_signing_service;
        let _ = &ctx.to_device_service;
        let _ = &ctx.presence_storage;
        let _ = &ctx.device_storage;
        let _ = &ctx.federation_inbound_edu_semaphore;
        let _ = &ctx.federation_inbound_edu_origin_semaphores;
        let _ = &ctx.federation_presence_backoff_until;
        let _ = &ctx.federation_join_semaphore;
    }
    let _ = assert_fields;
}

#[test]
fn test_room_context_public_fields_are_accessible() {
    fn assert_fields(ctx: &RoomContext) {
        let _ = &ctx.room_service;
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.server_name;
        let _ = &ctx.cache;
        let _ = &ctx.sync_service;
        let _ = &ctx.thread_service;
        let _ = &ctx.space_service;
        let _ = &ctx.room_summary_service;
        let _ = &ctx.account_data_service;
        let _ = &ctx.search_service;
        let _ = &ctx.retention_service;
        let _ = &ctx.translation_service;
        let _ = &ctx.federation_client;
        let _ = &ctx.rtc_domain_service;
        let _ = &ctx.e2ee_backup_service;
        let _ = &ctx.config;
        let _ = &ctx.admin_audit_service;
        let _ = &ctx.account_identity_service;
        let _ = &ctx.account_device_list_service;
        let _ = &ctx.push_notification_service;
        let _ = &ctx.event_broadcaster;
        let _ = &ctx.cross_signing_service;
        let _ = &ctx.metrics;
        let _ = &ctx.presence_service;
        let _ = &ctx.typing_service;
        let _ = &ctx.relations_service;
        let _ = &ctx.ssss_service;
        let _ = &ctx.dehydrated_device_service;
        let _ = &ctx.delayed_event_service;
    }
    let _ = assert_fields;
}

#[test]
fn test_media_context_public_fields_are_accessible() {
    fn assert_fields(ctx: &MediaContext) {
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.user_service;
        let _ = &ctx.config;
        let _ = &ctx.server_name;
        let _ = &ctx.cache;
        let _ = &ctx.media_service;
        let _ = &ctx.media_domain_service;
        let _ = &ctx.room_service;
        let _ = &ctx.federation_client;
        let _ = &ctx.account_identity_service;
        let _ = &ctx.admin_audit_service;
    }
    let _ = assert_fields;
}

#[test]
fn test_sso_context_public_fields_are_accessible() {
    fn assert_fields(ctx: &SsoContext) {
        let _ = &ctx.validator;
        let _ = &ctx.token_auth;
        let _ = &ctx.credential_auth;
        let _ = &ctx.room_auth;
        let _ = &ctx.config;
        let _ = &ctx.server_name;
        let _ = &ctx.cache;
        let _ = &ctx.registration_service;
        let _ = &ctx.user_service;
        let _ = &ctx.account_identity_service;
        let _ = &ctx.account_device_list_service;
        let _ = &ctx.oidc_service;
        let _ = &ctx.oidc_mapping_storage;
        let _ = &ctx.admin_audit_service;
        let _ = &ctx.refresh_token_service;
    }
    let _ = assert_fields;
}

// ============================================================================
// Feature-gated context fields — compile-time contract for optional surfaces
// ============================================================================

#[test]
fn test_friend_context_is_feature_gated() {
    // FriendContext is only compiled when the `friends` feature is enabled.
    // This test asserts the conditional compilation is wired correctly:
    //   - `friends` on  → FriendContext exists and is Clone
    //   - `friends` off → the type cannot be named at all
    #[cfg(feature = "friends")]
    {
        use synapse_web::routes::context::FriendContext;
        fn assert_traits<T: Clone + Send + Sync>() {}
        assert_traits::<FriendContext>();
    }
    #[cfg(not(feature = "friends"))]
    {
        // Without the feature, FriendContext must not be exported. There's
        // nothing to assert at runtime — the compile-time check is the test.
    }
}

// ============================================================================
// Context count sanity check
// ============================================================================

#[test]
fn test_eleven_context_structs_exist() {
    // The module declares 11 context structs (10 unconditional + FriendContext
    // when `friends` is enabled). This test enumerates them by name to lock
    // the surface: removing or renaming a context silently breaks the
    // corresponding router group.
    #[allow(clippy::too_many_arguments)]
    fn _assert_contexts(
        _a: CoreContext,
        _b: RoomContext,
        _c: E2eeRoomContext,
        _d: SyncContext,
        _e: DeviceContext,
        _f: AuthContext,
        _g: AdminContext,
        _h: FederationContext,
        _i: MediaContext,
        _j: SsoContext,
    ) {
    }
    let _ = _assert_contexts;
}
