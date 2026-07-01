//! Integration tests for `CapabilityGovernance` at
//! `synapse-services/src/capability_governance.rs`.
//!
//! `CapabilityGovernance` is a pure domain service: it derives Matrix client
//! capability / version / SSO-provider responses from a `Config` snapshot plus
//! a pre-collected set of `(method, path)` route-surface entries. It performs
//! no I/O and holds no mutable state, so these tests exercise it synchronously
//! without a database pool. The serial-execution guard, unique-id counter and
//! poisoning-tolerant lock pattern are kept for structural consistency with
//! the rest of the integration test suite.
//!
//! Covers all 7 public items on the public API surface:
//!   - `RouteCheck::new`
//!   - `CapabilityGovernance::new`
//!   - `CapabilityGovernance::declared_client_api_versions`
//!   - `CapabilityGovernance::build_client_versions`
//!   - `CapabilityGovernance::change_password_capability_enabled`
//!   - `CapabilityGovernance::sso_providers`
//!   - `CapabilityGovernance::build_capabilities_response`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;
use synapse_common::config::{BuiltinOidcUser, Config};
use synapse_services::capability_governance::{CapabilityGovernance, RouteCheck};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn capability_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Acquire the serial-execution guard, tolerating a poisoned mutex so a single
/// panicking test does not cascade into every subsequent test in this file.
fn guard() -> std::sync::MutexGuard<'static, ()> {
    capability_guard().lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

/// Build a `CapabilityGovernance` from `config` and the given `(method, path)`
/// route entries, mirroring how the HTTP layer converts its route manifests.
fn governance(config: &Config, routes: Vec<(&str, &'static str)>) -> CapabilityGovernance {
    CapabilityGovernance::new(
        config,
        routes.into_iter().map(|(method, path)| RouteCheck::new(method.to_string(), path)).collect(),
    )
}

/// Every route-surface route that the governance module inspects. Tests that
/// need "all capabilities enabled" reuse this fixture.
fn full_route_surface() -> Vec<(&'static str, &'static str)> {
    vec![
        ("GET", "/_matrix/client/v3/rooms/{room_id}/summary"),
        ("POST", "/_synapse/room_summary/v1/summaries/batch"),
        ("GET", "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device"),
        ("GET", "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports"),
        ("GET", "/_matrix/client/v1/rooms/{room_id}/hierarchy"),
        ("GET", "/_matrix/client/v3/voip/turnServer"),
        ("GET", "/_matrix/client/v1/threads"),
        ("POST", "/_matrix/client/v1/sync"),
        ("POST", "/_matrix/client/v3/account/password"),
        ("PUT", "/_matrix/client/v3/profile/{user_id}/displayname"),
        ("PUT", "/_matrix/client/v3/profile/{user_id}/avatar_url"),
        ("POST", "/_matrix/client/v3/account/3pid"),
        ("GET", "/_matrix/client/v3/friends"),
        ("GET", "/_matrix/client/v1/external_services/health"),
        ("GET", "/_matrix/client/v1/voice/config"),
        ("PUT", "/_matrix/client/v1/rooms/{room_id}/burn"),
    ]
}

fn governance_full(config: &Config) -> CapabilityGovernance {
    governance(config, full_route_surface())
}

/// Configure an external OIDC IdP that satisfies `OidcConfig::is_enabled()`.
fn enable_external_oidc(config: &mut Config) {
    config.oidc.enabled = true;
    config.oidc.issuer = "https://idp.example.com".to_string();
    config.oidc.client_id = "synapse-rust".to_string();
}

/// Configure a builtin OIDC provider that satisfies `BuiltinOidcConfig::is_enabled()`.
fn enable_builtin_oidc(config: &mut Config) {
    config.builtin_oidc.enabled = true;
    config.builtin_oidc.issuer = "https://hs.example.com".to_string();
    config.builtin_oidc.users.push(BuiltinOidcUser {
        id: format!("@alice_{}:example.com", unique_id()),
        username: "alice".to_string(),
        password: Some("password".to_string()),
        password_hash: None,
        email: "alice@example.com".to_string(),
        displayname: Some("Alice".to_string()),
    });
}

/// Borrow the `capabilities` object from a capabilities response body.
fn caps(body: &Value) -> &serde_json::Map<String, Value> {
    body["capabilities"].as_object().expect("capabilities object")
}

/// Borrow the `unstable_features` object from a response body.
fn unstable(body: &Value) -> &serde_json::Map<String, Value> {
    body["unstable_features"].as_object().expect("unstable_features object")
}

// =============================================================================
// RouteCheck::new
// =============================================================================

#[test]
fn test_route_check_new_stores_method_and_path() {
    let _g = guard();
    let entry = RouteCheck::new("POST".to_string(), "/_matrix/client/v1/sync");
    assert_eq!(entry.method, "POST");
    assert_eq!(entry.path, "/_matrix/client/v1/sync");
}

#[test]
fn test_route_check_equality_hash_and_set_dedup() {
    let _g = guard();
    let a = RouteCheck::new("GET".to_string(), "/_matrix/client/v1/threads");
    let b = RouteCheck::new("GET".to_string(), "/_matrix/client/v1/threads");
    let c = RouteCheck::new("POST".to_string(), "/_matrix/client/v1/threads");

    // PartialEq + Eq
    assert_eq!(a, b);
    assert_ne!(a, c);

    // Hash: identical entries collapse inside a HashSet.
    let mut set = HashSet::new();
    set.insert(a.clone());
    set.insert(b.clone());
    set.insert(c.clone());
    assert_eq!(set.len(), 2, "duplicate (method,path) entries must dedup in a HashSet");
}

// =============================================================================
// CapabilityGovernance::new
// =============================================================================

#[test]
fn test_new_with_empty_route_surface_yields_disabled_route_capabilities() {
    let _g = guard();
    let config = Config::default();
    let g = CapabilityGovernance::new(&config, vec![]);

    // Empty route surface -> every route-surface capability reports disabled.
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["m.change_password"]["enabled"], false);
    assert_eq!(caps["m.room.summary"]["enabled"], false);
    assert_eq!(caps["m.thread"]["enabled"], false);
    assert_eq!(caps["m.voice"]["enabled"], false);
}

#[test]
fn test_new_clones_config_independent_of_caller_mutations() {
    let _g = guard();
    let mut config = Config::default();
    config.saml.enabled = true;
    let g = CapabilityGovernance::new(&config, vec![]);

    // Mutating the caller-side config after construction must not leak in.
    config.saml.enabled = false;
    let providers = g.sso_providers();
    assert!(
        providers.contains(&"saml"),
        "governance should hold its own config snapshot; later caller mutations must not affect it"
    );
}

#[test]
fn test_new_dedups_duplicate_route_entries() {
    let _g = guard();
    let config = Config::default();
    // Same (method, path) supplied twice — the internal HashSet collapses them.
    let routes = vec![
        RouteCheck::new("POST".to_string(), "/_matrix/client/v3/account/password"),
        RouteCheck::new("POST".to_string(), "/_matrix/client/v3/account/password"),
    ];
    let g = CapabilityGovernance::new(&config, routes);

    assert!(g.change_password_capability_enabled(), "duplicate route entries should still enable the capability");
}

// =============================================================================
// declared_client_api_versions
// =============================================================================

#[test]
fn test_declared_client_api_versions_is_non_empty() {
    let _g = guard();
    let versions = CapabilityGovernance::declared_client_api_versions();
    assert!(!versions.is_empty(), "server must declare at least one client API version");
}

#[test]
fn test_declared_client_api_versions_contains_legacy_r0_6_1() {
    let _g = guard();
    let versions = CapabilityGovernance::declared_client_api_versions();
    assert!(versions.contains(&"r0.6.1"), "legacy r0.6.1 must remain declared");
    assert!(versions.contains(&"r0.5.0"), "legacy r0.5.0 must remain declared");
}

#[test]
fn test_declared_client_api_versions_contains_stable_v1_14() {
    let _g = guard();
    let versions = CapabilityGovernance::declared_client_api_versions();
    assert!(versions.contains(&"v1.14"), "v1.14 must be declared (VERSION_GAP_ANALYSIS audit)");
    assert!(versions.contains(&"v1.13"), "v1.13 must be declared");
}

#[test]
fn test_declared_client_api_versions_are_unique() {
    let _g = guard();
    let versions = CapabilityGovernance::declared_client_api_versions();
    let mut deduped = versions.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(deduped.len(), versions.len(), "declared versions must not contain duplicates");
}

// =============================================================================
// build_client_versions
// =============================================================================

#[test]
fn test_build_client_versions_versions_array_matches_declared() {
    let _g = guard();
    let g = governance(&Config::default(), vec![]);
    let body = g.build_client_versions();
    let versions = body["versions"].as_array().expect("versions is an array");
    let declared = CapabilityGovernance::declared_client_api_versions();
    assert_eq!(versions.len(), declared.len());
    for value in versions {
        let s = value.as_str().expect("version entry is a string");
        assert!(declared.contains(&s), "versions response contains undeclared version {s}");
    }
}

#[test]
fn test_build_client_versions_declares_base_unstable_features() {
    let _g = guard();
    let g = governance(&Config::default(), vec![]);
    let body = g.build_client_versions();
    let unstable = unstable(&body);

    assert_eq!(unstable["m.lazy_load_members"], true);
    assert_eq!(unstable["m.require_identity_server"], false);
    assert_eq!(unstable["org.matrix.msc3882"], true);
    assert_eq!(unstable["uk.tcpip.msc4133"], true);
}

#[test]
fn test_build_client_versions_phone_login_unavailable() {
    let _g = guard();
    // Phone (MSISDN) verification is intentionally disabled because no SMS
    // service is wired in — clients must not be offered a login-via-phone path.
    let g = governance(&Config::default(), vec![]);
    let body = g.build_client_versions();
    let unstable = unstable(&body);
    assert_eq!(unstable["m.supports_login_via_phone_number"], false);
}

#[test]
fn test_build_client_versions_sliding_sync_off_without_route() {
    let _g = guard();
    let g = governance(&Config::default(), vec![]);
    let body = g.build_client_versions();
    let unstable = unstable(&body);
    assert_eq!(unstable["org.matrix.msc3886.sliding_sync"], false);
}

#[test]
fn test_build_client_versions_sliding_sync_on_with_route() {
    let _g = guard();
    let g = governance(&Config::default(), vec![("POST", "/_matrix/client/v1/sync")]);
    let body = g.build_client_versions();
    let unstable = unstable(&body);
    assert_eq!(unstable["org.matrix.msc3886.sliding_sync"], true);
}

#[test]
fn test_build_client_versions_route_driven_msc_features() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_client_versions();
    let unstable = unstable(&body);
    // All four route-surface-driven MSCs light up when their routes are present.
    assert_eq!(unstable["org.matrix.msc3266"], true);
    assert_eq!(unstable["org.matrix.msc3245"], true);
    assert_eq!(unstable["org.matrix.msc3983"], true);
    assert_eq!(unstable["org.matrix.msc3814"], true);
    assert_eq!(unstable["org.matrix.msc4143"], true);
}

#[test]
fn test_build_client_versions_never_leaks_private_hula_namespace() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_client_versions();
    let unstable = unstable(&body);
    // `/versions` is unauthenticated; private `io.hula.*` extensions must stay
    // on the authenticated `/capabilities` surface only.
    assert!(!unstable.contains_key("io.hula.friends"));
    assert!(!unstable.contains_key("io.hula.burn_after_read"));
    assert!(!unstable.contains_key("io.hula.sliding_sync"));
}

// =============================================================================
// change_password_capability_enabled
// =============================================================================

#[test]
fn test_change_password_capability_enabled_false_without_route() {
    let _g = guard();
    let g = governance(&Config::default(), vec![]);
    assert!(!g.change_password_capability_enabled());
}

#[test]
fn test_change_password_capability_enabled_true_with_route() {
    let _g = guard();
    let g = governance(
        &Config::default(),
        vec![("POST", "/_matrix/client/v3/account/password")],
    );
    assert!(g.change_password_capability_enabled());
}

#[test]
fn test_change_password_capability_method_must_match() {
    let _g = guard();
    // A GET on the same path must not enable the POST-gated capability.
    let g = governance(
        &Config::default(),
        vec![("GET", "/_matrix/client/v3/account/password")],
    );
    assert!(!g.change_password_capability_enabled());
}

// =============================================================================
// sso_providers
// =============================================================================

#[test]
fn test_sso_providers_empty_by_default() {
    let _g = guard();
    let providers = governance(&Config::default(), vec![]).sso_providers();
    assert!(providers.is_empty(), "default config declares no SSO providers");
}

#[test]
fn test_sso_providers_includes_saml_when_enabled() {
    let _g = guard();
    let mut config = Config::default();
    config.saml.enabled = true;
    let providers = governance(&config, vec![]).sso_providers();
    assert!(providers.contains(&"saml"));
    assert_eq!(providers.len(), 1);
}

#[test]
fn test_sso_providers_includes_oidc_for_external_idp() {
    let _g = guard();
    let mut config = Config::default();
    enable_external_oidc(&mut config);
    let providers = governance(&config, vec![]).sso_providers();
    assert!(providers.contains(&"oidc"), "enabled external OIDC should appear as an sso provider");
}

#[test]
fn test_sso_providers_includes_oidc_for_builtin_provider() {
    let _g = guard();
    let mut config = Config::default();
    enable_builtin_oidc(&mut config);
    let providers = governance(&config, vec![]).sso_providers();
    assert!(providers.contains(&"oidc"), "enabled builtin OIDC should appear as an sso provider");
}

#[test]
fn test_sso_providers_excludes_oidc_when_incomplete() {
    let _g = guard();
    // `enabled = true` but missing issuer/client_id -> is_enabled() is false.
    let mut config = Config::default();
    config.oidc.enabled = true;
    let providers = governance(&config, vec![]).sso_providers();
    assert!(!providers.contains(&"oidc"), "incomplete OIDC config must not declare the oidc provider");

    // Builtin enabled but no users -> is_enabled() is false.
    let mut config2 = Config::default();
    config2.builtin_oidc.enabled = true;
    config2.builtin_oidc.issuer = "https://hs.example.com".to_string();
    let providers2 = governance(&config2, vec![]).sso_providers();
    assert!(!providers2.contains(&"oidc"), "builtin OIDC without users must not declare the oidc provider");
}

#[test]
fn test_sso_providers_lists_multiple_providers() {
    let _g = guard();
    let mut config = Config::default();
    config.saml.enabled = true;
    enable_external_oidc(&mut config);
    let providers = governance(&config, vec![]).sso_providers();
    assert!(providers.contains(&"saml"));
    assert!(providers.contains(&"oidc"));
    // Exactly saml + oidc without cas-sso feature.
    #[cfg(not(feature = "cas-sso"))]
    assert_eq!(providers.len(), 2);
}

// =============================================================================
// build_capabilities_response — public (unauthenticated) surface
// =============================================================================

#[test]
fn test_capabilities_response_unauthenticated_has_room_versions() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert!(caps.contains_key("m.room_versions"), "m.room_versions must always be present");
    assert!(caps["m.room_versions"].is_object());
}

#[test]
fn test_capabilities_response_unauthenticated_excludes_private_surface() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    for key in &["m.sso", "io.hula.friends", "io.hula.burn_after_read", "io.hula.sliding_sync"] {
        assert!(!caps.contains_key(*key), "private capability {key} leaked to unauthenticated surface");
    }
}

#[test]
fn test_capabilities_response_unauthenticated_public_capability_keys() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    for key in &[
        "m.change_password",
        "m.room_versions",
        "m.set_displayname",
        "m.set_avatar_url",
        "m.3pid_changes",
        "m.room.summary",
        "m.room.suggested",
        "m.voice",
        "m.thread",
    ] {
        assert!(caps.contains_key(*key), "missing public capability {key}");
    }
}

#[test]
fn test_capabilities_response_route_surface_drives_public_capabilities() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["m.change_password"]["enabled"], true);
    assert_eq!(caps["m.set_displayname"]["enabled"], true);
    assert_eq!(caps["m.set_avatar_url"]["enabled"], true);
    assert_eq!(caps["m.3pid_changes"]["enabled"], true);
    assert_eq!(caps["m.room.summary"]["enabled"], true);
    assert_eq!(caps["m.room.suggested"]["enabled"], true);
    assert_eq!(caps["m.voice"]["enabled"], true);
    assert_eq!(caps["m.thread"]["enabled"], true);
}

// =============================================================================
// build_capabilities_response — authenticated surface
// =============================================================================

#[test]
fn test_capabilities_response_authenticated_includes_sso_with_providers() {
    let _g = guard();
    let mut config = Config::default();
    config.saml.enabled = true;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let sso = body["capabilities"]["m.sso"].as_object().expect("m.sso object");
    assert_eq!(sso["enabled"], true);
    assert_eq!(sso["providers"][0], "saml");
}

#[test]
fn test_capabilities_response_authenticated_sso_disabled_when_no_providers() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(true);
    let sso = body["capabilities"]["m.sso"].as_object().expect("m.sso object");
    assert_eq!(sso["enabled"], false, "m.sso must be disabled when no providers are configured");
    assert!(sso["providers"].as_array().expect("providers array").is_empty());
}

#[test]
fn test_capabilities_response_authenticated_includes_private_extensions() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.declare_private_extensions = true;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    for key in &[
        "io.hula.friends",
        "m.sso",
        "ai_connection",
        "openclaw",
        "external_services",
        "io.hula.voice_extended",
        "io.hula.burn_after_read",
    ] {
        assert!(caps.contains_key(*key), "missing authenticated capability {key}");
    }
}

// =============================================================================
// build_capabilities_response — config-driven capability toggles
// =============================================================================

#[test]
fn test_capabilities_response_msc4452_default_false() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["io.element.msc4452.preview_url"]["enabled"], false);
}

#[test]
fn test_capabilities_response_msc4452_enabled_via_config() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.msc4452_enabled = true;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["io.element.msc4452.preview_url"]["enabled"], true);
}

#[test]
fn test_capabilities_response_voice_independent_of_room_summary() {
    let _g = guard();
    // Only the TURN route is registered; the room-summary route is absent.
    let g = governance(
        &Config::default(),
        vec![("GET", "/_matrix/client/v3/voip/turnServer")],
    );
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["m.voice"]["enabled"], true, "m.voice follows the TURN route, not room summary");
    assert_eq!(caps["m.room.summary"]["enabled"], false, "room summary must stay disabled");
}

#[test]
fn test_capabilities_response_room_suggested_uses_hierarchy_route() {
    let _g = guard();
    // m.room.suggested follows the hierarchy endpoint, distinct from m.room.summary.
    let g = governance(
        &Config::default(),
        vec![("GET", "/_matrix/client/v1/rooms/{room_id}/hierarchy")],
    );
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["m.room.suggested"]["enabled"], true);
    assert_eq!(caps["m.room.summary"]["enabled"], false, "summary uses a different endpoint");
}

#[test]
fn test_capabilities_response_burn_after_read_suppressed_without_private_extensions() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.declare_private_extensions = false;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    assert_eq!(caps["io.hula.burn_after_read"]["enabled"], false);
}

#[test]
fn test_capabilities_response_burn_after_read_enabled_with_route_and_private_extensions() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.declare_private_extensions = true;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    assert_eq!(caps["io.hula.burn_after_read"]["enabled"], true);
}

#[test]
fn test_capabilities_response_friends_suppressed_without_private_extensions() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.declare_private_extensions = false;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    assert_eq!(body["capabilities"]["io.hula.friends"], false);
    // unstable_features mirrors the suppression.
    assert_eq!(body["unstable_features"]["io.hula.friends"], false);
}

#[test]
fn test_capabilities_response_voice_extended_suppressed_without_private_extensions() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.declare_private_extensions = false;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    assert_eq!(caps["io.hula.voice_extended"]["enabled"], false);
}

#[test]
fn test_capabilities_response_openclaw_config_driven() {
    let _g = guard();
    // Without the `openclaw-routes` feature, openclaw_routes_enabled() is false
    // and the capability reports disabled regardless of config.
    let mut config = Config::default();
    #[cfg(feature = "openclaw-routes")]
    {
        config.experimental.openclaw_routes_enabled = false;
    }
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    assert_eq!(caps["openclaw"]["enabled"], false);
    assert_eq!(caps["ai_connection"]["enabled"], false);
}

#[cfg(feature = "openclaw-routes")]
#[test]
fn test_capabilities_response_openclaw_enabled_when_feature_and_config_set() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.openclaw_routes_enabled = true;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    assert_eq!(caps["openclaw"]["enabled"], true);
    assert_eq!(caps["ai_connection"]["enabled"], true);
}

#[test]
fn test_capabilities_response_external_services_via_route() {
    let _g = guard();
    let g = governance(
        &Config::default(),
        vec![("GET", "/_matrix/client/v1/external_services/health")],
    );
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    assert_eq!(caps["external_services"]["enabled"], true);
}

// =============================================================================
// build_capabilities_response — unstable_features consistency
// =============================================================================

#[test]
fn test_capabilities_unstable_features_consistent_with_versions_surface() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let versions_body = g.build_client_versions();
    let versions_unstable = unstable(&versions_body).clone();
    let caps_body = g.build_capabilities_response(true);
    let caps_unstable = unstable(&caps_body);

    // The shared route-surface-driven MSC flags should be consistent when
    // present on both surfaces. Some MSCs may only appear on one surface
    // depending on config, so we only assert consistency for keys present
    // on both.
    for key in &[
        "org.matrix.msc3886.sliding_sync",
        "org.matrix.msc3266",
        "org.matrix.msc3245",
        "org.matrix.msc3983",
        "org.matrix.msc3814",
        "org.matrix.msc4143",
    ] {
        let v = versions_unstable.get(*key);
        let c = caps_unstable.get(*key);
        if v.is_some() && c.is_some() {
            assert_eq!(
                v, c,
                "unstable feature {key} disagrees between /versions and /capabilities"
            );
        }
    }
}

#[test]
fn test_capabilities_unstable_features_includes_private_hula_extensions() {
    let _g = guard();
    let mut config = Config::default();
    config.experimental.declare_private_extensions = true;
    let g = governance_full(&config);
    let body = g.build_capabilities_response(true);
    let unstable = unstable(&body);
    // The authenticated /capabilities surface keeps the private io.hula.*
    // extensions that /versions intentionally omits.
    assert!(unstable.contains_key("io.hula.friends"));
    assert!(unstable.contains_key("io.hula.burn_after_read"));
    assert_eq!(unstable["io.hula.friends"], true);
    assert_eq!(unstable["io.hula.burn_after_read"], true);
}

#[test]
fn test_capabilities_unstable_features_voice_and_thread_track_routes() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(false);
    let unstable = unstable(&body);
    assert_eq!(unstable["org.matrix.msc3245.voice"], true);
    assert_eq!(unstable["org.matrix.msc3983.thread"], true);
    assert_eq!(unstable["org.matrix.msc3886.sliding_sync"], true);
}

#[test]
fn test_capabilities_response_no_unknown_capability_keys() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(true);
    let caps = caps(&body);
    let known: &[&str] = &[
        "m.change_password",
        "m.room_versions",
        "m.set_displayname",
        "m.set_avatar_url",
        "m.3pid_changes",
        "m.room.summary",
        "m.room.suggested",
        "m.voice",
        "m.thread",
        "io.element.msc4452.preview_url",
        "io.hula.friends",
        "m.sso",
        "ai_connection",
        "openclaw",
        "external_services",
        "io.hula.voice_extended",
        "io.hula.burn_after_read",
    ];
    for key in caps.keys() {
        assert!(
            known.contains(&key.as_str()),
            "unexpected capability key in authenticated response: {key}"
        );
    }
}

// =============================================================================
// Edge cases: empty / partial route surfaces
// =============================================================================

#[test]
fn test_capabilities_response_all_route_caps_disabled_with_empty_surface() {
    let _g = guard();
    let g = governance(&Config::default(), vec![]);
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    // Every route-surface capability must be disabled when no routes are present.
    for key in &[
        "m.change_password",
        "m.set_displayname",
        "m.set_avatar_url",
        "m.3pid_changes",
        "m.room.summary",
        "m.room.suggested",
        "m.voice",
        "m.thread",
    ] {
        assert_eq!(caps[*key]["enabled"], false, "{key} should be disabled with empty route surface");
    }
}

#[test]
fn test_capabilities_response_partial_route_surface_enables_only_matching_caps() {
    let _g = guard();
    // Only the threads route is registered.
    let g = governance(&Config::default(), vec![("GET", "/_matrix/client/v1/threads")]);
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["m.thread"]["enabled"], true);
    assert_eq!(caps["m.voice"]["enabled"], false);
    assert_eq!(caps["m.room.summary"]["enabled"], false);
    assert_eq!(caps["m.change_password"]["enabled"], false);
}

#[test]
fn test_capabilities_response_method_mismatch_does_not_enable() {
    let _g = guard();
    // Wrong HTTP method on the threads path must not enable m.thread.
    let g = governance(&Config::default(), vec![("POST", "/_matrix/client/v1/threads")]);
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["m.thread"]["enabled"], false);
}

#[test]
fn test_capabilities_response_path_mismatch_does_not_enable() {
    let _g = guard();
    // A different path must not satisfy the route check.
    let g = governance(
        &Config::default(),
        vec![("GET", "/_matrix/client/v2/threads")],
    );
    let body = g.build_capabilities_response(false);
    let caps = caps(&body);
    assert_eq!(caps["m.thread"]["enabled"], false);
}

#[test]
fn test_sso_capability_disabled_when_no_providers_configured() {
    let _g = guard();
    let g = governance_full(&Config::default());
    let body = g.build_capabilities_response(true);
    let sso = body["capabilities"]["m.sso"].as_object().expect("m.sso object");
    assert_eq!(sso["enabled"], false);
    assert!(sso["providers"].as_array().expect("providers array").is_empty());
}
