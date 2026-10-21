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

/// Methods that only read.
const READ_METHODS: [&str; 2] = ["GET", "HEAD"];

/// Routes under `/_synapse/admin` of the given methods that
/// `is_role_allowed("admin", …)` allows.
fn plain_admin_routes(methods: &[&str]) -> BTreeSet<(String, String)> {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/synapse-rust/ROUTE_HANDLERS.json"))
        .expect("ROUTE_HANDLERS.json must be readable — run scripts/contract/gen_route_handlers.py");
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("ROUTE_HANDLERS.json must be JSON");
    let routes = doc["routes"].as_array().expect("`routes` must be an array");

    let mut allowed = BTreeSet::new();
    for route in routes {
        let path = route["path"].as_str().unwrap_or_default();
        let method_name = route["method"].as_str().unwrap_or_default();
        if !path.contains("/_synapse/admin") || !methods.contains(&method_name) {
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

fn declared_allowlist(file: &str) -> BTreeSet<(String, String)> {
    let raw = std::fs::read_to_string(format!("{}/../test-support/{file}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|error| panic!("the RBAC register `{file}` must be readable: {error}"));
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once(' ').map(|(method, path)| (method.to_string(), path.to_string())))
        .collect()
}

/// 双向比对 + 「守卫的守卫」：登记表或路由表任一侧读空都会让断言变成空转。
fn assert_register_matches(actual: &BTreeSet<(String, String)>, file: &str, min_expected: usize, kind: &str) {
    let declared = declared_allowlist(file);

    // 路由表读空（路径写错、`routes` 为空）时两个集合都空，下面的断言会"通过"却什么都没测。
    assert!(
        actual.len() > min_expected,
        "loaded only {} plain-admin {kind} routes — ROUTE_HANDLERS.json is probably stale or \
         unreadable, so this test would be asserting nothing",
        actual.len()
    );

    // 两类差异一起报：先报 unregistered 再报 stale 的话，每次修完一个方向才能看到另一个，
    // 而一次收紧几十条规则时这会把反馈循环拖成三四轮。
    let unregistered: Vec<_> = actual.difference(&declared).collect();
    let stale: Vec<_> = declared.difference(actual).collect();
    let mut problems: Vec<String> = Vec::new();
    if !unregistered.is_empty() {
        problems.push(format!(
            "reachable by a plain admin but **not** registered in \
             test-support/{file}:\n{unregistered:#?}\n\
             Decide explicitly: register them (deliberate) or add them to \
             `is_super_admin_only_endpoint` (restrict)."
        ));
    }
    if !stale.is_empty() {
        problems.push(format!(
            "registered as plain-admin-reachable but no longer reachable (rule tightened or route \
             removed):\n{stale:#?}\n\
             Drop them from test-support/{file}."
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

/// Every admin write route a plain admin can reach must be registered, and every
/// registered route must still be reachable.
#[test]
fn plain_admin_write_surface_matches_the_register() {
    assert_register_matches(&plain_admin_routes(&WRITE_METHODS), "admin_rbac_write_allowlist.txt", 50, "write");
}

/// 读面同一套登记：新增读端点不会"提权"，但会在无人察觉的情况下开始输出数据
/// （房间消息、用户 IP、举报内容、审计日志）。这张表把"开始输出"变成一个显式决定。
#[test]
fn plain_admin_read_surface_matches_the_register() {
    assert_register_matches(&plain_admin_routes(&READ_METHODS), "admin_rbac_read_allowlist.txt", 100, "read");
}
