//! Guards for the admin RBAC allow-surface.
//!
//! `is_role_allowed("admin", …)` decides with a prefix OR-chain, and the
//! blacklist (`is_super_admin_only_endpoint`) only carves a handful of holes out
//! of it. A new **write** endpoint under `/_synapse/admin/v1/users` (or
//! `/rooms`, `/media`, `/saml`, `/_ … `) is therefore reachable by a plain admin
//! with nothing in the build noticing — the gap is silent by construction.
//!
//! Two layers of protection:
//!
//! 1. **按角色的登记表**（`admin_rbac_write_allowlist.txt` /
//!    `admin_rbac_read_allowlist.txt`）把「普通 admin 能碰什么」变成显式清单，
//!    **双向**断言：能到却没登记 ⇒ 红；登记了却到不了 ⇒ 红。
//! 2. **判定矩阵**（`admin_rbac_decision_matrix.txt`）：`role × 每条 admin 路由`
//!    的 allow/deny 全量快照。登记表只覆盖 `admin` 一个角色，矩阵把
//!    `auditor` / `security_admin` / `user_admin` / `media_admin` / 无角色
//!    这些**没有细粒度测试**的分支一并钉住，也是把判定改成数据表时的 golden。
//!
//! 两层都调用生产判定 `is_role_allowed`，不重新实现逻辑，所以不可能各自漂移。

use std::collections::BTreeSet;

use axum::http::Method;

use super::{is_admin_request_allowed, is_role_allowed, NO_ADMIN_ROLE};

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

// ── 判定矩阵（golden）────────────────────────────────────────────────────────
//
// 登记表只覆盖 `admin`。`auditor` / `security_admin` / `user_admin` /
// `media_admin` / 无角色这几支此前**没有任何细粒度断言**，改一行 `||` 就可能
// 悄悄放开或堵死它们。矩阵把「角色 × 每条 admin 路由」的判定全量钉住：
// 任何无意的行为变化都会在 diff 里显示成具体某几行，而不是一句"说不清"。

/// 全部 admin 路由（任意方法），按 `(path, method)` 排序。
fn all_admin_routes() -> BTreeSet<(String, String)> {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/synapse-rust/ROUTE_HANDLERS.json"))
        .expect("ROUTE_HANDLERS.json must be readable");
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("ROUTE_HANDLERS.json must be JSON");
    doc["routes"]
        .as_array()
        .expect("`routes` must be an array")
        .iter()
        .filter_map(|route| {
            let path = route["path"].as_str()?;
            let method = route["method"].as_str()?;
            path.contains("/_synapse/admin").then(|| (path.to_string(), method.to_string()))
        })
        .collect()
}

/// 矩阵覆盖的角色。含 `none`（`user_type` 未设置 ⇒ 最小权限）与一个不存在的角色。
const MATRIX_ROLES: [&str; 7] =
    ["super_admin", "admin", "auditor", "security_admin", "user_admin", "media_admin", "none"];

fn decision_matrix() -> String {
    // 矩阵里写的是字面量 `none`：`MATRIX_ROLES` 是 `const`，不能用 `NO_ADMIN_ROLE`。
    // 这里把两者钉在一起，常量改名/改值会让本测试直接红。
    assert!(MATRIX_ROLES.contains(&NO_ADMIN_ROLE), "MATRIX_ROLES must cover NO_ADMIN_ROLE ({NO_ADMIN_ROLE})");

    let routes = all_admin_routes();
    let mut out = String::from(
        "# role\tmethod\tpath\tdecision\n\
         # 由 `cargo nextest run -E 'test(regenerate_role_decision_matrix)' --run-ignored` 生成；\n\
         # 手工改动前先确认自己是在**有意**改变某个角色的可及面。\n",
    );
    for role in MATRIX_ROLES {
        for (path, method) in &routes {
            let Ok(method) = Method::from_bytes(method.as_bytes()) else {
                continue;
            };
            let decision = if is_role_allowed(role, &method, path) { "allow" } else { "deny" };
            out.push_str(&format!("{role}\t{method}\t{path}\t{decision}\n"));
        }
    }
    out
}

fn matrix_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-support/admin_rbac_decision_matrix.txt")
}

/// 判定矩阵必须与 golden 逐行相同。
///
/// 报错只列**差异行**：2093 行的矩阵整体 `assert_eq!` 会把终端刷满，实际要看的是
/// "哪几个 `角色/方法/路径` 的判定变了"。用行集合差，新增路由导致的行位移也不会来干扰。
#[test]
fn role_decision_matrix_matches_the_golden() {
    let actual = decision_matrix();
    assert!(
        actual.lines().count() > 1000,
        "the matrix is suspiciously small ({} lines) — ROUTE_HANDLERS.json probably failed to load",
        actual.lines().count()
    );
    let golden = std::fs::read_to_string(matrix_path()).expect("the decision matrix golden must be readable");

    let actual_lines: BTreeSet<&str> = actual.lines().filter(|line| !line.starts_with('#')).collect();
    let golden_lines: BTreeSet<&str> = golden.lines().filter(|line| !line.starts_with('#')).collect();
    let now_allowed_or_denied: Vec<&&str> = actual_lines.difference(&golden_lines).collect();
    let no_longer_true: Vec<&&str> = golden_lines.difference(&actual_lines).collect();

    let mut problems: Vec<String> = Vec::new();
    if !now_allowed_or_denied.is_empty() {
        problems.push(format!(
            "判定结果里有 {} 条在 golden 中不存在（角色/方法/路径 → 新判定）：\n{now_allowed_or_denied:#?}",
            now_allowed_or_denied.len()
        ));
    }
    if !no_longer_true.is_empty() {
        problems.push(format!(
            "golden 里有 {} 条已不再成立（说明该角色的可及面**被收窄**，或路由被删）：\n{no_longer_true:#?}",
            no_longer_true.len()
        ));
    }
    assert!(
        problems.is_empty(),
        "{}\n\n若这是**有意**的权限变化，重新生成 golden 并把它当作一次 permission change 审阅：\n  \
         cargo nextest run -E 'test(regenerate_role_decision_matrix)' --run-ignored",
        problems.join("\n\n")
    );
}

/// 重新生成 golden。只在**有意**修改判定时手工运行。
#[test]
#[ignore = "writes the golden file; run explicitly when a permission change is intended"]
fn regenerate_role_decision_matrix() {
    std::fs::write(matrix_path(), decision_matrix()).expect("the decision matrix golden must be writable");
}

// ── SEC-01：`admin_rbac_enabled=false` 不得重开超管专属端点 ─────────────────────
//
// 开关的语义是「关闭角色细分」。修复前的实现是 `!rbac_enabled || allowed`——
// 短路掉了 `allowed`（含黑名单判定）⇒ 关闭 RBAC 后，任何 `is_admin = true` 的
// 账号都能调用 `…/make_admin`、`DELETE /rooms/{id}`、`…/purge` 这类提权端点。
//
// 下面两条测试是一对：
// + 第一条钉住「黑名单在开关之外」，覆盖**全部**超管专属路由（不手抄，直接从
//   `ROUTE_HANDLERS.json` × 生产黑名单判定里取）；
// + 第二条钉住「开关仍然有效」——否则把判定改成恒 `false` 也能让第一条通过，
//   测试就空转了。

/// `ROUTE_HANDLERS.json` 中被生产黑名单判为「仅 super_admin」的 `(path, method)`。
fn super_admin_only_routes() -> Vec<(String, Method)> {
    let mut routes: Vec<(String, Method)> = all_admin_routes()
        .into_iter()
        .filter_map(|(path, method)| {
            let method = Method::from_bytes(method.as_bytes()).ok()?;
            super::privileged::is_super_admin_only_endpoint(&method, &path).then_some((path, method))
        })
        .collect();
    routes.sort_by(|a, b| (a.0.as_str(), a.1.as_str()).cmp(&(b.0.as_str(), b.1.as_str())));
    routes.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    routes
}

#[test]
fn rbac_switch_does_not_reopen_super_admin_only_endpoints() {
    let routes = super_admin_only_routes();
    // 读空/读少 ⇒ 断言变成空转：路由表路径写错、`routes` 为空、黑名单被整体删掉。
    assert!(
        routes.len() >= 20,
        "只识别出 {} 条超管专属路由 — ROUTE_HANDLERS.json 或黑名单判定失效，这条测试会空转通过",
        routes.len()
    );

    let mut leaked: Vec<String> = Vec::new();
    for (path, method) in &routes {
        for role in MATRIX_ROLES {
            if role == "super_admin" {
                // 正控：开关不改变 super_admin 的可及面。
                assert!(
                    is_admin_request_allowed(role, method, path, false),
                    "{method} {path} 在 admin_rbac_enabled=false 下也必须仍然允许 super_admin"
                );
                continue;
            }
            if is_admin_request_allowed(role, method, path, false) {
                leaked.push(format!("{role:<14} {method:<6} {path}"));
            }
        }
    }

    assert!(
        leaked.is_empty(),
        "admin_rbac_enabled=false 下仍有 {} 个「非超管角色 × 超管专属端点」组合被放行（应为 403）：\n{}",
        leaked.len(),
        leaked.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn rbac_switch_still_disables_role_splitting() {
    // 找一个「普通 admin 能到、无角色（none）到不了」且不在黑名单里的端点：
    // 它证明 allowlist 仍然起作用，从而证明开关确实做了事。
    let (path, method) = all_admin_routes()
        .into_iter()
        .filter_map(|(path, method)| {
            let method = Method::from_bytes(method.as_bytes()).ok()?;
            (!super::privileged::is_super_admin_only_endpoint(&method, &path)
                && is_role_allowed("admin", &method, &path)
                && !is_role_allowed(NO_ADMIN_ROLE, &method, &path))
            .then_some((path, method))
        })
        .next()
        .expect("必须存在一个仅靠 allowlist 收紧的端点，否则 allowlist 等于没生效");

    assert!(
        !is_admin_request_allowed(NO_ADMIN_ROLE, &method, &path, true),
        "RBAC 开启时 {method} {path} 必须拒绝无角色"
    );
    assert!(
        is_admin_request_allowed(NO_ADMIN_ROLE, &method, &path, false),
        "RBAC 关闭时 {method} {path} 必须放行（开关只关闭角色细分）——\
         否则上一条测试可能因为「整片拒绝」而空转通过"
    );
}
