//! Route-table presence tests for the Matrix `/room_keys/*` surface.
//!
//! The previous `e2ee_routes.rs` regression silently won the merge over
//! `key_backup.rs` because axum's `Router::merge` lets duplicate paths
//! overwrite each other without warning, and we had no test that asserted
//! the assembled router actually exposes every spec endpoint with the
//! right methods. This test exists so that a future re-introduction of
//! that bug fails CI instead of a live deployment.
//!
//! Strategy: for every spec path we send a request with an unsupported
//! method (`PATCH`). If the route is registered, axum's `MethodRouter`
//! responds `405 Method Not Allowed` with an `Allow` header listing the
//! registered methods. If the route is missing entirely we get `404`.
//! The presence of `405` (and the expected methods in `Allow`) is what
//! proves the route is wired through `create_key_backup_router`.
//!
//! See 已删除的 SPEC_ALIGNMENT_PLAN_2026-05-01 §1.2 (R4 / T2).

use axum::{body::Body, http::Request};
use hyper::StatusCode;
use tower::ServiceExt;

use super::{setup_fresh_test_app, with_local_connect_info};

fn expected_methods(allow_header: &str) -> std::collections::HashSet<String> {
    allow_header.split(',').map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()).collect()
}

async fn assert_route(app: &axum::Router, path: &str, expected: &[&str]) {
    let req = Request::builder().method("PATCH").uri(path).body(Body::empty()).unwrap();

    let response = app.clone().oneshot(with_local_connect_info(req)).await.expect("oneshot");

    let status = response.status();
    assert_ne!(status, StatusCode::NOT_FOUND, "route {} returned 404 — not registered in the assembled Router", path);
    assert_eq!(
        status,
        StatusCode::METHOD_NOT_ALLOWED,
        "route {} returned {} for PATCH; expected 405 Method Not Allowed",
        path,
        status
    );

    let allow = response
        .headers()
        .get("allow")
        .unwrap_or_else(|| panic!("route {} returned 405 without Allow header", path))
        .to_str()
        .unwrap()
        .to_string();
    let got = expected_methods(&allow);

    for method in expected {
        assert!(
            got.contains(&method.to_ascii_uppercase()),
            "route {} Allow header {:?} is missing expected method {}",
            path,
            allow,
            method
        );
    }
}

#[tokio::test]
async fn key_backup_routes_are_wired_under_v3() {
    let Some(app) = setup_fresh_test_app().await else {
        super::skip_or_fail_without_db();
        return;
    };

    // (path, expected methods). All three nested prefixes (v1/r0/v3) share
    // the same inner `Router`; verifying v3 is sufficient to prove
    // `create_key_backup_router` is participating in the merge.
    let cases: &[(&str, &[&str])] = &[
        // Spec endpoints (version as ?version= query param).
        ("/_matrix/client/v3/room_keys/version", &["GET", "POST"]),
        ("/_matrix/client/v3/room_keys/version/v123", &["GET", "PUT", "DELETE"]),
        ("/_matrix/client/v3/room_keys/keys", &["GET", "PUT", "DELETE"]),
        ("/_matrix/client/v3/room_keys/keys/!room:example.org", &["GET", "PUT", "DELETE"]),
        ("/_matrix/client/v3/room_keys/keys/!room:example.org/abcsession", &["GET", "PUT", "DELETE"]),
        // Legacy path-version variants (kept for MSC compatibility).
        ("/_matrix/client/v3/room_keys/v123/keys", &["GET", "PUT", "DELETE"]),
        ("/_matrix/client/v3/room_keys/v123/keys/!room:example.org", &["GET", "PUT", "DELETE"]),
        ("/_matrix/client/v3/room_keys/v123/keys/!room:example.org/abcsession", &["GET", "PUT", "DELETE"]),
    ];

    for (path, methods) in cases {
        assert_route(&app, path, methods).await;
    }
}

#[tokio::test]
async fn key_backup_routes_are_wired_under_v3_and_v1_is_retired() {
    let Some(app) = setup_fresh_test_app().await else {
        super::skip_or_fail_without_db();
        return;
    };

    // v3 已由 `key_backup_routes_are_wired_under_v3` 覆盖；这里只钉住
    // A3 的退役决定（`synapse-web/src/routes/key_backup.rs` 仅 nest `/_matrix/client/v3`，
    // 依据 `docs/audit/A3_v1_deprecation_strategy.md` 的 A 族历史双注册清零）。
    // 若未来有人把 `/v1` 双注册加回来，本用例会红——这正是退役决定需要的守卫。
    for path in
        ["/_matrix/client/v1/room_keys/version", "/_matrix/client/v1/room_keys/keys/!room:example.org/abcsession"]
    {
        let req = Request::builder().method("PATCH").uri(path).body(Body::empty()).unwrap();
        let response = app.clone().oneshot(with_local_connect_info(req)).await.expect("oneshot");
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "route {} is registered again — A3 retired the `/v1` duplicate mounting for key_backup",
            path
        );
    }
}

// 2026-10-23 删除 `key_backup_routes_are_wired_under_v1_and_r0`（重复 + 恒红）。
//
// 为什么是重复：上面 `key_backup_routes_are_wired_under_v3` 的注释已经写明
// 「All three nested prefixes (v1/r0/v3) share the same inner `Router`; verifying v3 is
// sufficient to prove `create_key_backup_router` is participating in the merge」。
// 被删的那条遍历 `["/_matrix/client/v1", "/_matrix/client/v3"]`：
//   · v3 的两条断言与上面**完全重叠**；
//   · v1 的两条断言**永远不可能通过** —— `create_key_backup_router` 结尾只有
//     `.nest("/_matrix/client/v3", router)`；全仓 0 处 `/_matrix/client/r0` 路由；
//     三个 v1 基线（`v1_surface_baseline` / `v1_no_twin_baseline.txt` /
//     `v1_spec_keep_baseline.txt`）里也都没有 `room_keys`。也就是说 `/room_keys/*`
//     从来没有在 v1/r0 面注册过，断言的期望值与代码事实相反。
//   · 函数名与内容也不符：名字说 "v1_and_r0"，代码测的是 v1 和 v3。
//
// 它从 2026-10-22 起就是 HEAD 既有红（见
// `docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md:1422` 与 `:1579` 的「契约/路由面 4 条」）。
// 删除不损失覆盖：v3 面由上面那条逐条覆盖（8 条路径 × 期望方法集）；
// v1 退役则由 `key_backup_routes_are_wired_under_v3_and_v1_is_retired` 钉住（期望 404）。
