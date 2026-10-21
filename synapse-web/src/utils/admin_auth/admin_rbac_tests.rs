//! Guards for the admin RBAC allow-surface.
//!
//! `is_role_allowed("admin", …)` decides with a prefix OR-chain, and the
//! blacklist (`is_super_admin_only_endpoint`) only carves a handful of holes out
//! of it. A new **write** endpoint under `/_synapse/admin/v1/users` (or
//! `/rooms`, `/media`, `/saml`, `/_ … `) is therefore reachable by a plain admin
//! with nothing in the build noticing — the gap is silent by construction.
//!
//! These tests turn that into an explicit register:
//! `test-support/admin_rbac_write_allowlist.txt` must equal the set of write
//! routes a plain admin can reach, **in both directions**. Adding an endpoint is
//! not forbidden; adding one without deciding is.
//!
//! The comparison calls the production predicate directly rather than
//! re-implementing it, so the two cannot drift apart.

use std::collections::BTreeSet;

use axum::http::Method;

use super::is_role_allowed;

/// Methods that mutate. `is_role_allowed` treats GET/HEAD as reads.
const WRITE_METHODS: [&str; 4] = ["POST", "PUT", "PATCH", "DELETE"];

/// Write routes under `/_synapse/admin` that `is_role_allowed("admin", …)` allows.
fn plain_admin_write_routes() -> BTreeSet<(String, String)> {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/synapse-rust/ROUTE_HANDLERS.json"))
        .expect("ROUTE_HANDLERS.json must be readable — run scripts/contract/gen_route_handlers.py");
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("ROUTE_HANDLERS.json must be JSON");
    let routes = doc["routes"].as_array().expect("`routes` must be an array");

    let mut allowed = BTreeSet::new();
    for route in routes {
        let path = route["path"].as_str().unwrap_or_default();
        let method_name = route["method"].as_str().unwrap_or_default();
        if !path.contains("/_synapse/admin") || !WRITE_METHODS.contains(&method_name) {
            continue;
        }
        let Ok(method) = Method::from_bytes(method_name.as_bytes()) else {
            continue;
        };
        if is_role_allowed("admin", &method, path) {
            allowed.insert((method_name.to_string(), path.to_string()));
        }
    }
    allowed
}

fn declared_allowlist() -> BTreeSet<(String, String)> {
    let raw =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../test-support/admin_rbac_write_allowlist.txt"))
            .expect("the RBAC register must be readable");
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once(' ').map(|(method, path)| (method.to_string(), path.to_string())))
        .collect()
}

/// Every admin write route a plain admin can reach must be registered, and every
/// registered route must still be reachable.
#[test]
fn plain_admin_write_surface_matches_the_register() {
    let actual = plain_admin_write_routes();
    let declared = declared_allowlist();

    // A regression guard for the guard: if the route table failed to load (wrong
    // path, empty `routes`), both sets would be empty and the assertions below
    // would pass while measuring nothing.
    assert!(
        actual.len() > 50,
        "loaded only {} plain-admin write routes — ROUTE_HANDLERS.json is probably stale or \
         unreadable, so this test would be asserting nothing",
        actual.len()
    );

    let unregistered: Vec<_> = actual.difference(&declared).collect();
    assert!(
        unregistered.is_empty(),
        "these admin write routes are reachable by a plain admin but are **not** registered in \
         test-support/admin_rbac_write_allowlist.txt:\n{unregistered:#?}\n\
         Decide explicitly: register them (deliberate) or add them to \
         `is_super_admin_only_endpoint` (restrict)."
    );

    let stale: Vec<_> = declared.difference(&actual).collect();
    assert!(
        stale.is_empty(),
        "these routes are registered as plain-admin-writable but no longer are — the rule was \
         tightened or the route was removed:\n{stale:#?}\n\
         Drop them from test-support/admin_rbac_write_allowlist.txt."
    );
}
