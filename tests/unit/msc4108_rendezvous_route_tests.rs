// MSC4108 rendezvous —— **路由台账面**（derived ledger）与 `ApiError` 映射。
//
// ⚠️ 本文件**不覆盖 handler 的 HTTP 行为**，只锁两件真东西：
//   1. `declared_ledger_all()` 里 `msc4108_rendezvous` 这个 slice 的路由台账（方法/路径/注册者/
//      无重复元组）—— 这是生产路由表的派生结果，删/改一个路由就会红；
//   2. `delete_success_response()`（DELETE 成功时**原样返回**的那个响应）与 `ApiError` 的
//      状态码/errcode 映射 —— 都是真函数调用。
//
// 这个文件曾经有 20 个用例，其中 12 个是「自证」：它把 `build_rendezvous_url`、
// `http_date_from_millis`、ETag 拼装各自**抄了一份**放进测试，再断言自己抄的那份
// （`let status = StatusCode::OK; assert_eq!(status, StatusCode::OK)`），
// 于是改坏生产 handler 它仍然全绿，而文件名 `..._route_tests` 让人以为路由被覆盖了
// （审计 NEW-05 的标本）。
//
// 现在 handler 的**真实 HTTP 往返**（真 router + 真库：200/400/404/412/413/304/204、
// ETag 变化、If-None-Match、If-Match 冲突、4KB 上限）在
// `tests/integration/msc4108_rendezvous_route_tests.rs`。要改 handler 契约，改那个文件；
// 要改路由表，改这个文件。

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use synapse_common::ApiError;
use synapse_web::routes::declared_ledger_all;
use synapse_web::routes::route_ledger::{RouteEntry, RouteLedger};

const BASE_PATH: &str = "/_matrix/client/unstable/org.matrix.msc4108/rendezvous";

/// The `msc4108_rendezvous` slice of the derived route table.
///
/// `msc4108_route_manifest()` was one of ~120 hand-copied projections deleted
/// by B2-2; route metadata now has a single source (`derived_routes`), and a
/// test that needs one module's surface filters it by `registered_by`.
fn msc4108_route_manifest() -> Vec<RouteEntry> {
    declared_ledger_all().iter().filter(|e| e.registered_by == "msc4108_rendezvous").cloned().collect()
}

// ── Route manifest ──────────────────────────────────────────────────────────

#[test]
fn manifest_declares_exactly_four_endpoints() {
    let entries = msc4108_route_manifest();
    assert_eq!(entries.len(), 4, "MSC4108 rendezvous must expose exactly 4 endpoints");
}

#[test]
fn manifest_declares_create_endpoint() {
    let entries = msc4108_route_manifest();
    assert!(entries.iter().any(|e| e.method == Method::POST && e.path == BASE_PATH));
}

#[test]
fn manifest_declares_get_endpoint() {
    let entries = msc4108_route_manifest();
    let session_path = format!("{BASE_PATH}/{{session_id}}");
    assert!(entries.iter().any(|e| e.method == Method::GET && e.path == session_path));
}

#[test]
fn manifest_declares_update_endpoint() {
    let entries = msc4108_route_manifest();
    let session_path = format!("{BASE_PATH}/{{session_id}}");
    assert!(entries.iter().any(|e| e.method == Method::PUT && e.path == session_path));
}

#[test]
fn manifest_declares_delete_endpoint() {
    let entries = msc4108_route_manifest();
    let session_path = format!("{BASE_PATH}/{{session_id}}");
    assert!(entries.iter().any(|e| e.method == Method::DELETE && e.path == session_path));
}

#[test]
fn manifest_entries_are_registered_by_msc4108_namespace() {
    let entries = msc4108_route_manifest();
    assert!(entries.iter().all(|e| e.registered_by == "msc4108_rendezvous"));
}

#[test]
fn manifest_has_no_duplicate_method_path_tuples() {
    let mut ledger = RouteLedger::new();
    ledger.extend(msc4108_route_manifest());
    let report = ledger.validate().expect("MSC4108 manifest must be duplicate-free");
    assert_eq!(report.unique_tuples, 4);
    assert_eq!(report.total_entries, 4);
}

#[test]
fn manifest_can_be_collected_into_route_ledger() {
    let ledger: Vec<RouteEntry> = msc4108_route_manifest();
    let methods: Vec<&str> = ledger.iter().map(|e| e.method.as_str()).collect();
    assert!(methods.contains(&"POST"));
    assert!(methods.contains(&"GET"));
    assert!(methods.contains(&"PUT"));
    assert!(methods.contains(&"DELETE"));
}

// ── delete_success_response：真实生产函数 ────────────────────────────────────

#[test]
fn delete_success_response_is_204_with_the_mandated_caching_headers() {
    // 这个函数就是 DELETE handler 成功时**逐字返回**的响应（S-15 的修法：把响应构造抽成
    // 自由函数，让头契约可以在不构造 AppState 的前提下被测）。删掉里面任意一个头元组 ⇒ 红。
    let response = synapse_web::routes::msc4108_rendezvous::delete_success_response();
    assert_eq!(response.status(), StatusCode::NO_CONTENT, "MSC4108 DELETE 成功是 204");
    let headers = response.headers();
    assert_eq!(headers.get("cache-control").and_then(|v| v.to_str().ok()), Some("no-store"));
    assert_eq!(headers.get("pragma").and_then(|v| v.to_str().ok()), Some("no-cache"));
    let last_modified = headers.get("last-modified").and_then(|v| v.to_str().ok()).unwrap_or_default();
    assert!(
        last_modified.ends_with("GMT") && last_modified.len() > 20,
        "Last-Modified 必须是 IMF-fixdate，实际 {last_modified:?}"
    );
}

// ── ApiError 状态码 / errcode 映射（真构造器） ──────────────────────────────

#[test]
fn not_found_error_maps_to_404() {
    let err = ApiError::not_found("Rendezvous session not found or expired".to_string());
    assert_eq!(err.http_status(), StatusCode::NOT_FOUND);
    assert!(err.is_not_found());
}

#[test]
fn bad_request_error_maps_to_400() {
    let err = ApiError::bad_request("ETag mismatch or session expired".to_string());
    assert_eq!(err.http_status(), StatusCode::BAD_REQUEST);
    assert!(err.is_bad_request());
}

#[test]
fn internal_error_maps_to_500_and_redacts_the_detail() {
    let err = ApiError::internal_with_context("Failed to create MSC4108 session", &"db down");
    assert_eq!(err.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(err.is_internal());
    // `message()` 对 Internal 只回通用文案，细节只在 `internal_message()`（日志用）。
    assert_eq!(err.message(), "An internal error occurred");
    assert!(err.internal_message().contains("Failed to create MSC4108 session"));
}
