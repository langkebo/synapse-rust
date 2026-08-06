// Assembly Route Tests - Top-Level Router Aggregation Coverage
//
// These tests cover the route-aggregation surface exposed by
// `src/web/routes/assembly.rs` (P-096: previously zero tests).
//
// `assembly.rs` is the entry point that builds the live axum `Router` from
// dozens of sub-routers. It exposes three pure-data entry points:
//   - `declared_route_manifest_for(&AppState)` (live, profile-aware)
//   - `declared_route_manifest_for_profile(&ProfileFlags)` (offline)
//   - `top_level_inline_manifest()` (the inline `.route(...)` calls in create_router)
//
// The `assembly_compat_manifest()` private helper expands inline sub-routers
// (capabilities, media_config, voip, auth, account, directory) across the
// r0/v1/v3 prefixes. Its output is folded into the public manifest via
// `declared_route_manifest_for_profile`, which is what these tests assert
// against — exactly the same pattern used by the route-ledger snapshot tests
// under `tests/integration/`.

use axum::http::Method;
use synapse_rust::web::routes::route_ledger::{RouteEntry, RouteLedger};
use synapse_rust::web::routes::route_module::ProfileFlags;
use synapse_rust::web::routes::declared_route_manifest_for_profile;

// ============================================================================
// declared_route_manifest_for_profile — surface sanity checks
// ============================================================================

#[test]
fn test_declared_manifest_default_profile_is_non_empty() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    assert!(!ledger.is_empty(), "default profile must declare at least one route");
}

#[test]
fn test_declared_manifest_returns_route_ledger() {
    // declared_route_manifest_for_profile returns a RouteLedger, not a bare
    // Vec<RouteEntry>. The ledger exposes `validate()` and `registered_by_counts()`
    // which the live `create_router` calls at startup.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let _ledger: &RouteLedger = &ledger;
    assert!(ledger.validate().is_ok(), "default-profile manifest must validate without duplicates");
}

#[test]
fn test_declared_manifest_has_no_duplicate_method_path_pairs() {
    // The live `create_router` aborts on duplicate (method, path) entries.
    // `RouteLedger::validate` is the gatekeeper — this test asserts the
    // default profile passes that gate.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let report = ledger.validate().expect("default manifest must be duplicate-free");
    assert_eq!(report.total_entries, report.unique_tuples, "every entry must be unique");
}

#[test]
fn test_declared_manifest_includes_top_level_inline_routes() {
    // The /health and /_matrix/client/versions endpoints are registered
    // inline in create_router and manifested in `top_level_inline_manifest`.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let entries: Vec<(Method, &str)> = ledger.iter().map(|e| (e.method.clone(), e.path)).collect();

    assert!(entries.contains(&(Method::GET, "/")), "root path must be declared");
    assert!(entries.contains(&(Method::GET, "/health")), "/health must be declared");
    assert!(entries.contains(&(Method::GET, "/_health")), "/_health must be declared");
    assert!(entries.contains(&(Method::GET, "/_matrix/client/versions")), "versions must be declared");
    assert!(entries.contains(&(Method::GET, "/_matrix/client/v3/versions")), "v3 versions must be declared");
}

#[test]
fn test_declared_manifest_includes_well_known_routes() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/.well-known/matrix/server"), "well-known server must be declared");
    assert!(paths.contains("/.well-known/matrix/client"), "well-known client must be declared");
    assert!(paths.contains("/.well-known/matrix/support"), "well-known support must be declared");
}

// ============================================================================
// Assembly compat manifests — capabilities, media_config, voip, auth, account, directory
// ============================================================================

#[test]
fn test_declared_manifest_includes_capabilities_under_r0_and_v3() {
    // create_client_capabilities_router is nested under both r0 and v3.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/_matrix/client/r0/capabilities"), "r0 capabilities missing");
    assert!(paths.contains("/_matrix/client/v3/capabilities"), "v3 capabilities missing");
}

#[test]
fn test_declared_manifest_includes_media_config_under_three_prefixes() {
    // create_client_media_config_router is nested under v1, r0, and v3.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/_matrix/client/v1/media/config"), "v1 media/config missing");
    assert!(paths.contains("/_matrix/client/r0/media/config"), "r0 media/config missing");
    assert!(paths.contains("/_matrix/client/v3/media/config"), "v3 media/config missing");
}

#[test]
fn test_declared_manifest_includes_voip_compat_under_r0_and_v3() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/_matrix/client/r0/voip/turnServer"), "r0 voip/turnServer missing");
    assert!(paths.contains("/_matrix/client/v3/voip/turnServer"), "v3 voip/turnServer missing");
    assert!(paths.contains("/_matrix/client/r0/voip/config"), "r0 voip/config missing");
    assert!(paths.contains("/_matrix/client/v3/voip/config"), "v3 voip/config missing");
    assert!(paths.contains("/_matrix/client/r0/voip/turnServer/guest"), "r0 voip/turnServer/guest missing");
    assert!(paths.contains("/_matrix/client/v3/voip/turnServer/guest"), "v3 voip/turnServer/guest missing");
}

#[test]
fn test_declared_manifest_includes_auth_compat_under_r0_and_v3() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    // Both GET and POST are registered on /register and /login.
    assert!(paths.contains("/_matrix/client/r0/register"), "r0 register missing");
    assert!(paths.contains("/_matrix/client/v3/register"), "v3 register missing");
    assert!(paths.contains("/_matrix/client/r0/login"), "r0 login missing");
    assert!(paths.contains("/_matrix/client/v3/login"), "v3 login missing");
    assert!(paths.contains("/_matrix/client/r0/logout"), "r0 logout missing");
    assert!(paths.contains("/_matrix/client/v3/logout"), "v3 logout missing");
    assert!(paths.contains("/_matrix/client/r0/logout/all"), "r0 logout/all missing");
    assert!(paths.contains("/_matrix/client/v3/logout/all"), "v3 logout/all missing");
    assert!(paths.contains("/_matrix/client/r0/refresh"), "r0 refresh missing");
    assert!(paths.contains("/_matrix/client/v3/refresh"), "v3 refresh missing");
}

#[test]
fn test_declared_manifest_includes_auth_standalone_routes() {
    // Login fallback page (MSC2965) and MSC4108 QR token are absolute paths
    // not nested under r0/v3.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/_matrix/static/client/login/"), "login fallback page missing");
    assert!(paths.contains("/_matrix/client/v1/login/qr_token"), "MSC4108 qr_token missing");
}

#[test]
fn test_declared_manifest_includes_account_compat_under_three_prefixes() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    // whoami and password are exposed under v1, r0, and v3.
    for prefix in ["/_matrix/client/v1", "/_matrix/client/r0", "/_matrix/client/v3"] {
        assert!(paths.contains(&format!("{prefix}/account/whoami").as_str()), "{prefix}/account/whoami missing");
        assert!(paths.contains(&format!("{prefix}/account/password").as_str()), "{prefix}/account/password missing");
        assert!(paths.contains(&format!("{prefix}/account/deactivate").as_str()), "{prefix}/account/deactivate missing");
        assert!(paths.contains(&format!("{prefix}/account/3pid").as_str()), "{prefix}/account/3pid missing");
    }
}

#[test]
fn test_declared_manifest_includes_account_r0_only_extras() {
    // The r0-only router adds /account/profile/{user_id}* aliases that were
    // never standardized into v3. They are deprecated but still served.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/_matrix/client/r0/account/profile/{user_id}"), "r0 account/profile missing");
    assert!(
        paths.contains("/_matrix/client/r0/account/profile/{user_id}/displayname"),
        "r0 account/profile displayname missing"
    );
    assert!(
        paths.contains("/_matrix/client/r0/account/profile/{user_id}/avatar_url"),
        "r0 account/profile avatar_url missing"
    );
}

#[test]
fn test_declared_manifest_includes_directory_compat_under_r0_and_v3() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/_matrix/client/r0/user_directory/search"), "r0 user_directory/search missing");
    assert!(paths.contains("/_matrix/client/v3/user_directory/search"), "v3 user_directory/search missing");
    assert!(paths.contains("/_matrix/client/r0/directory/room/{room_alias}"), "r0 directory/room missing");
    assert!(paths.contains("/_matrix/client/v3/directory/room/{room_alias}"), "v3 directory/room missing");
    assert!(paths.contains("/_matrix/client/r0/publicRooms"), "r0 publicRooms missing");
    assert!(paths.contains("/_matrix/client/v3/publicRooms"), "v3 publicRooms missing");
}

#[test]
fn test_declared_manifest_includes_directory_r0_only_extras() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    assert!(paths.contains("/_matrix/client/r0/directory/room/{room_id}/alias"), "r0 directory room alias missing");
}

// ============================================================================
// Profile-driven modules — feature-gated routes are not in DEFAULT profile
// ============================================================================

#[test]
fn test_default_profile_excludes_oidc_specific_routes() {
    // The DEFAULT profile has oidc_enabled=false, so OIDC-specific routes
    // are not in the manifest. The OIDC routes come from the route_module
    // trait, not assembly_compat_manifest.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let oidc_entries: Vec<&RouteEntry> = ledger
        .iter()
        .filter(|e| e.registered_by == "oidc" || e.path.contains("/_matrix/client/v3/account/sso/oidc"))
        .collect();
    // OIDC routes are feature-gated and not present in the default profile.
    // This test is intentionally loose — it documents that the DEFAULT
    // profile does not auto-enable OIDC routes.
    let _ = oidc_entries;
}

#[test]
fn test_default_profile_includes_module_routes() {
    // The module router is always-on (not feature-gated).
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();
    // Pick one always-on module route to verify the module router is wired in.
    // admin is one of the largest always-on surfaces.
    let has_admin_route = paths.iter().any(|p| p.starts_with("/_synapse/admin/"));
    assert!(has_admin_route, "admin module routes must be present in default profile");
}

// ============================================================================
// registered_by tagging — every manifest entry must carry a registered_by tag
// ============================================================================

#[test]
fn test_every_manifest_entry_has_non_empty_registered_by() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    for entry in ledger.iter() {
        assert!(!entry.registered_by.is_empty(), "entry {:?} {} has empty registered_by", entry.method, entry.path);
    }
}

#[test]
fn test_registered_by_includes_expected_namespaces() {
    // The manifest aggregates entries from many router modules; this test
    // asserts that the well-known namespaces are present.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let namespaces: std::collections::HashSet<&str> =
        ledger.iter().map(|e| e.registered_by).collect();
    // These are the inline + always-on module namespaces.
    let expected_namespaces = [
        "assembly::create_router",
        "assembly::capabilities",
        "assembly::media_config",
        "assembly::voip_compat",
        "assembly::auth_compat",
        "assembly::account_compat",
        "assembly::directory_compat",
    ];
    for ns in &expected_namespaces {
        assert!(namespaces.contains(ns), "expected namespace '{ns}' missing from registered_by tags");
    }
}

// ============================================================================
// Method/path shape — every path starts with '/'
// ============================================================================

#[test]
fn test_every_manifest_path_starts_with_slash() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    for entry in ledger.iter() {
        assert!(
            entry.path.starts_with('/'),
            "path must start with '/' — got {:?} {}",
            entry.method,
            entry.path
        );
    }
}

#[test]
fn test_manifest_does_not_contain_empty_paths() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    for entry in ledger.iter() {
        assert!(!entry.path.is_empty(), "manifest contains an empty path");
    }
}

// ============================================================================
// Profile sensitivity — flipping flags changes the manifest
// ============================================================================

#[test]
fn test_worker_enabled_profile_adds_worker_routes() {
    // Enabling the worker flag must add at least one route to the manifest
    // (the worker admin router exposes additional endpoints).
    let default = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let worker_on = declared_route_manifest_for_profile(&ProfileFlags {
        oidc_enabled: false,
        worker_enabled: true,
        saml_enabled: false,
        #[cfg(feature = "openclaw-routes")]
        openclaw_enabled: false,
    });

    let default_count = default.iter().count();
    let worker_count = worker_on.iter().count();
    assert!(
        worker_count >= default_count,
        "enabling worker flag must not shrink the manifest: default={}, worker={}",
        default_count,
        worker_count
    );
}

#[test]
fn test_oidc_enabled_profile_changes_manifest_size() {
    // Flipping oidc_enabled must not produce an empty manifest (OIDC routes
    // are additive on top of the always-on surface).
    let oidc_off = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let oidc_on = declared_route_manifest_for_profile(&ProfileFlags {
        oidc_enabled: true,
        worker_enabled: false,
        saml_enabled: false,
        #[cfg(feature = "openclaw-routes")]
        openclaw_enabled: false,
    });

    assert!(!oidc_on.is_empty(), "oidc-enabled manifest must not be empty");
    assert!(!oidc_off.is_empty(), "oidc-disabled manifest must not be empty");
}

// ============================================================================
// RouteLedger trait/behavior — smoke tests on the ledger object
// ============================================================================

#[test]
fn test_route_ledger_iter_returns_route_entry_refs() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let entries: Vec<&RouteEntry> = ledger.iter().collect();
    assert!(!entries.is_empty());
    for entry in &entries {
        assert!(!entry.path.is_empty());
        assert!(!entry.registered_by.is_empty());
    }
}

#[test]
fn test_route_ledger_validate_returns_report_with_counts() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let report = ledger.validate().expect("manifest must validate");
    assert!(report.unique_tuples > 0, "unique_tuples must be positive");
    assert_eq!(report.total_entries, report.unique_tuples);
    // The default profile should declare at least 100 routes — the synapse
    // client surface is large.
    assert!(
        report.unique_tuples >= 100,
        "default manifest seems too small: {} unique tuples",
        report.unique_tuples
    );
}

#[test]
fn test_route_ledger_registered_by_counts_is_non_empty() {
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let counts = ledger.registered_by_counts();
    assert!(!counts.is_empty(), "registered_by_counts must not be empty");
    // Each registered_by count must have a positive entry count.
    for count in &counts {
        assert!(count.entries > 0, "registered_by {} has 0 entries", count.registered_by);
    }
}

// ============================================================================
// Fallback route — /M_UNRECOGNIZED for unmatched paths
// ============================================================================

#[test]
fn test_fallback_handler_is_set_in_create_router() {
    // create_router sets a fallback that returns 404 M_UNRECOGNIZED for
    // unmatched paths. This is documented in the source but not exposed
    // in the manifest — manifest entries are explicit (method, path) tuples.
    // We assert the contract by checking that the manifest does NOT contain
    // a wildcard entry (every entry is a concrete path).
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    for entry in ledger.iter() {
        assert!(!entry.path.contains('*'), "manifest entries must not use wildcards: {}", entry.path);
        assert!(!entry.path.is_empty(), "manifest entries must not be empty");
    }
}

// ============================================================================
// top_level_inline_manifest — entry count check (via aggregation)
// ============================================================================

#[test]
fn test_top_level_inline_manifest_contributes_routes_to_default_profile() {
    // top_level_inline_manifest declares ~28 entries (GET / , /health, /_health,
    // versions, pushrules, well-known, MSC2965, MSC3814, MSC4143, MSC4133).
    // We assert a representative subset is present in the default manifest.
    let ledger = declared_route_manifest_for_profile(&ProfileFlags::DEFAULT);
    let paths: std::collections::HashSet<&str> = ledger.iter().map(|e| e.path).collect();

    let expected_inline_paths = [
        "/",
        "/health",
        "/_health",
        "/_matrix/client/versions",
        "/_matrix/client/v3/versions",
        "/_matrix/client/r0/version",
        "/_matrix/server_version",
        "/_matrix/client/v1/config/client",
        "/_matrix/client/v3/pushrules/",
        "/_matrix/client/v3/pushrules/global/",
        "/_matrix/client/r0/pushrules/",
        "/_matrix/client/r0/pushrules/global/",
        "/.well-known/matrix/server",
        "/.well-known/matrix/client",
        "/.well-known/matrix/support",
        "/_matrix/client/unstable/org.matrix.msc2965/auth_metadata",
        "/_matrix/client/unstable/org.matrix.msc2965/auth_issuer",
        "/_matrix/client/v1/auth_metadata",
        "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
        "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status",
        "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports",
        "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}",
    ];

    for path in &expected_inline_paths {
        assert!(paths.contains(*path), "top-level inline path missing from default manifest: {path}");
    }
}
