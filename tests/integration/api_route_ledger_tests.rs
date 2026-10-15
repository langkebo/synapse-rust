//! End-to-end probe of the [`declared_ledger_for`] against the assembled
//! [`Router`].
//!
//! The ledger is the substitute we ship for the route-walker API axum does
//! not expose (R4 / O2 in 已删除的 SPEC_ALIGNMENT_PLAN_2026-05-01).
//! Manifest-only validation catches *internal* duplicates, but a manifest
//! entry that no longer has a real route behind it (e.g. someone deleted a
//! `.route(...)` call without removing the manifest entry) would silently
//! pass — exactly the inverse of the original key_backup bug. This test
//! closes that loop by sending a `PATCH` to every declared `(method, path)`
//! tuple and asserting the response is a 405 with the declared method
//! present in the `Allow` header. A 404 here means the manifest is lying.
//!
//! Why `PATCH`? It is reserved by RFC 5789 but unused by every endpoint we
//! register, so axum's `MethodRouter` will always answer with 405 + `Allow`
//! when the route exists.
//!
//! [`declared_ledger_for`]: synapse_web::routes::declared_ledger_for
//! [`Router`]: axum::Router

use axum::http::Method;
use axum::{body::Body, http::Request};
use futures::stream::{self, StreamExt};
use hyper::StatusCode;
use regex::Regex;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};
use synapse_web::routes::declared_ledger_for;
use synapse_web::routes::route_ledger::RouteLedger;
use synapse_web::routes::state::AppState;
use tokio::sync::OnceCell;
use tower::ServiceExt;

use super::{setup_fresh_test_app_with_config, with_local_connect_info};

type TestFixture = Option<(axum::Router, AppState)>;

static DEFAULT_FIXTURE: OnceCell<TestFixture> = OnceCell::const_new();
static WORKER_ENABLED_FIXTURE: OnceCell<TestFixture> = OnceCell::const_new();
static WORKER_WITHOUT_REPLICATION_FIXTURE: OnceCell<TestFixture> = OnceCell::const_new();
static DEFAULT_LEDGER: OnceCell<Option<RouteLedger>> = OnceCell::const_new();
static WORKER_ENABLED_LEDGER: OnceCell<Option<RouteLedger>> = OnceCell::const_new();

async fn default_fixture() -> TestFixture {
    DEFAULT_FIXTURE
        .get_or_init(|| async {
            setup_fresh_test_app_with_config(|container| {
                super::config_mut(container).federation.allow_ingress = true;
            })
            .await
        })
        .await
        .clone()
}

/// Worker mode ON, HTTP replication left at its **default (false)**.
///
/// This is the configuration that used to expose the worker body surface with no
/// authentication at all: the routes were mounted on `worker.enabled` alone,
/// while `replication_http_auth_middleware` returned `next.run(request)` when
/// `replication.http.enabled` was false. See `S1` in
/// docs/audit/DB_REVIEW_2026-09-17.md §13.7.
///
/// `worker.replication.http.enabled` is deliberately NOT set here; that its
/// default is `false` is pinned by
/// `synapse_common::config::worker::tests::test_replication_config_default`.
async fn worker_without_replication_fixture() -> TestFixture {
    WORKER_WITHOUT_REPLICATION_FIXTURE
        .get_or_init(|| async {
            setup_fresh_test_app_with_config(|container| {
                super::config_mut(container).federation.allow_ingress = true;
                super::config_mut(container).worker.enabled = true;
            })
            .await
        })
        .await
        .clone()
}

async fn worker_enabled_fixture() -> TestFixture {
    WORKER_ENABLED_FIXTURE
        .get_or_init(|| async {
            setup_fresh_test_app_with_config(|container| {
                super::config_mut(container).federation.allow_ingress = true;
                super::config_mut(container).worker.enabled = true;
                super::config_mut(container).worker.replication.http.enabled = true;
                super::config_mut(container).worker.replication.http.secret = Some("test_worker_secret".to_string());
                super::config_mut(container).worker.replication.http.secret_path = None;
            })
            .await
        })
        .await
        .clone()
}

async fn default_ledger() -> Option<RouteLedger> {
    DEFAULT_LEDGER
        .get_or_init(|| async { default_fixture().await.as_ref().map(|(_, state)| declared_ledger_for(state)) })
        .await
        .clone()
}

async fn worker_enabled_ledger() -> Option<RouteLedger> {
    WORKER_ENABLED_LEDGER
        .get_or_init(|| async { worker_enabled_fixture().await.as_ref().map(|(_, state)| declared_ledger_for(state)) })
        .await
        .clone()
}

fn allow_methods(allow_header: &str) -> std::collections::HashSet<String> {
    allow_header.split(',').map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()).collect()
}

#[allow(clippy::needless_pass_by_value)]
fn has_declared_route(ledger: &synapse_web::routes::route_ledger::RouteLedger, method: Method, path: &str) -> bool {
    ledger.iter().any(|entry| entry.method == method && entry.path == path)
}

fn render_ledger_snapshot(snapshot_name: &str, ledger: &synapse_web::routes::route_ledger::RouteLedger) -> String {
    let mut lines: Vec<String> = ledger
        .iter()
        .map(|entry| format!("{} {} [{}]", entry.method.as_str(), entry.path, entry.registered_by))
        .collect();
    lines.sort();

    format!("# route-ledger snapshot: {snapshot_name}\ncount: {}\n\n{}\n", lines.len(), lines.join("\n"))
}

fn route_ledger_snapshot_path(snapshot_file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("integration").join("snapshots").join(snapshot_file)
}

fn assert_route_ledger_snapshot(snapshot_file: &str, actual: &str) {
    let path = route_ledger_snapshot_path(snapshot_file);
    if env::var_os("UPDATE_ROUTE_LEDGER_SNAPSHOTS").is_some() {
        let parent = path.parent().expect("snapshot path should have parent");
        fs::create_dir_all(parent).expect("create snapshot directory");
        fs::write(&path, actual).expect("write route-ledger snapshot");
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read snapshot {}: {}. Re-run with UPDATE_ROUTE_LEDGER_SNAPSHOTS=1 to generate it.",
            path.display(),
            error
        )
    });
    assert_eq!(
        actual, expected,
        "route-ledger snapshot mismatch for {}. Re-run with UPDATE_ROUTE_LEDGER_SNAPSHOTS=1 if the route-surface change is intentional.",
        snapshot_file
    );
}

#[tokio::test]
async fn declared_route_manifest_size_stays_under_probe_warning_threshold() {
    // Guard for SPEC_ALIGNMENT_PLAN_2026-05-01 §7.2: bumping this constant
    // silently is a regression path. Current ceiling = current manifest size
    // (1317 on 2026-08-06) + ~10% headroom. If you genuinely need to raise
    // it, refresh §7.2 with a fresh probe-time datapoint and decide whether
    // PROBE_CONCURRENCY needs raising or sampling needs to land first.
    const WARNING_ROUTE_COUNT: usize = 1450;

    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let report = ledger.validate().expect("manifest must validate");
    assert!(
        report.unique_tuples <= WARNING_ROUTE_COUNT,
        "ledger has grown to {} routes (>{} warning threshold). Update \
         SPEC_ALIGNMENT_PLAN §7.2 with a fresh probe-time datapoint and \
         decide whether to raise PROBE_CONCURRENCY or move to sampling \
         before bumping this constant.",
        report.unique_tuples,
        WARNING_ROUTE_COUNT,
    );
}

#[tokio::test]
async fn declared_route_manifest_validates_with_no_duplicates() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let report = ledger.validate().expect("declared_ledger_for must be free of duplicate (method, path) tuples");
    assert!(report.unique_tuples >= 1, "ledger should declare at least one route, got {}", report.unique_tuples);
    assert_eq!(report.total_entries, report.unique_tuples);
}

#[tokio::test]
async fn declared_route_manifest_entries_are_actually_wired() {
    let Some((app, state)) = default_fixture().await else {
        super::skip_or_fail_without_db();
        return;
    };

    let ledger = declared_ledger_for(&state);
    assert!(!ledger.is_empty(), "ledger is empty — nothing to probe");

    // Concurrency cap chosen against the integration test PG pool. Bumping
    // this needs to be matched against `database.max_connections` in the
    // test config, otherwise the probe will deadlock on pool acquisition.
    // See SPEC_ALIGNMENT_PLAN_2026-05-01 §7.2.
    const PROBE_CONCURRENCY: usize = 16;

    enum Outcome {
        Ok,
        Missing(String),
        MethodMismatch(String),
    }

    let outcomes: Vec<Outcome> = stream::iter(ledger.iter())
        .map(|entry| {
            let app = app.clone();
            async move {
                let req = Request::builder().method("PATCH").uri(entry.path).body(Body::empty()).unwrap();

                let response = app.oneshot(with_local_connect_info(req)).await.expect("oneshot");
                let status = response.status();

                if status == StatusCode::NOT_FOUND {
                    // A wired route can legitimately answer 404: e.g. a handler
                    // that looks up a resource by a probe id that does not exist
                    // (the MSC4512 proxy accepts PATCH via `any()`, so the probe
                    // reaches `proxy_to_as`, which returns `M_NOT_FOUND` for an
                    // unknown AS). The router's catch-all fallback is the *only*
                    // signal that a route is genuinely absent, and it answers
                    // `M_UNRECOGNIZED` (see `create_router`). Distinguish them
                    // so `any()`-style routes are not misreported as missing.
                    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap_or_default();
                    let is_router_fallback = String::from_utf8_lossy(&body).contains("M_UNRECOGNIZED");
                    if is_router_fallback {
                        return Outcome::Missing(format!(
                            "{} {} (declared by {})",
                            entry.method, entry.path, entry.registered_by
                        ));
                    }
                    // A handler produced this 404, so the route is wired.
                    return Outcome::Ok;
                }
                if status != StatusCode::METHOD_NOT_ALLOWED {
                    // Any other status means the route exists and is doing
                    // something (e.g. a permissive handler or a middleware
                    // short-circuit). Fine for this test — we only care
                    // that the route is wired at all.
                    return Outcome::Ok;
                }

                let Some(allow) = response.headers().get("allow") else {
                    return Outcome::MethodMismatch(format!(
                        "{} {}: 405 without Allow header",
                        entry.method, entry.path
                    ));
                };
                let allow = allow.to_str().unwrap_or_default();
                let methods = allow_methods(allow);
                if !methods.contains(entry.method.as_str()) {
                    Outcome::MethodMismatch(format!(
                        "{} {} (declared by {}): Allow={:?}",
                        entry.method, entry.path, entry.registered_by, allow
                    ))
                } else {
                    Outcome::Ok
                }
            }
        })
        .buffer_unordered(PROBE_CONCURRENCY)
        .collect()
        .await;

    let mut missing = Vec::new();
    let mut method_mismatch = Vec::new();
    for outcome in outcomes {
        match outcome {
            Outcome::Ok => {}
            Outcome::Missing(msg) => missing.push(msg),
            Outcome::MethodMismatch(msg) => method_mismatch.push(msg),
        }
    }
    missing.sort();
    method_mismatch.sort();

    assert!(
        missing.is_empty(),
        "manifest declares routes that the live router does not expose:\n  - {}",
        missing.join("\n  - ")
    );
    assert!(
        method_mismatch.is_empty(),
        "manifest declares methods the live router does not honour:\n  - {}",
        method_mismatch.join("\n  - ")
    );
}

#[tokio::test]
async fn declared_route_manifest_full_snapshot_matches_default_state() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let actual = render_ledger_snapshot("default", &ledger);
    assert_route_ledger_snapshot("route_ledger_default.snapshot", &actual);
}

#[tokio::test]
async fn worker_body_routes_follow_runtime_flag_in_ledger() {
    let Some(disabled_ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    assert!(!has_declared_route(&disabled_ledger, Method::POST, "/_synapse/worker/v1/workers/{worker_id}/heartbeat"));
    assert!(!has_declared_route(&disabled_ledger, Method::GET, "/_synapse/worker/v1/events"));

    let Some(enabled_ledger) = worker_enabled_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    assert!(has_declared_route(&enabled_ledger, Method::POST, "/_synapse/worker/v1/workers/{worker_id}/heartbeat"));
    assert!(has_declared_route(&enabled_ledger, Method::GET, "/_synapse/worker/v1/events"));
}

#[tokio::test]
async fn worker_body_routes_are_live_when_worker_mode_enabled() {
    let heartbeat_uri = "/_synapse/worker/v1/workers/probe-worker/heartbeat";
    let events_uri = "/_synapse/worker/v1/events";

    let Some((disabled_app, _state)) = default_fixture().await else {
        super::skip_or_fail_without_db();
        return;
    };

    let heartbeat_disabled = disabled_app
        .clone()
        .oneshot(with_local_connect_info(
            Request::builder()
                .method(Method::POST)
                .uri(heartbeat_uri)
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"running","load_stats":null}"#))
                .unwrap(),
        ))
        .await
        .expect("oneshot");
    assert_eq!(heartbeat_disabled.status(), StatusCode::NOT_FOUND);

    let events_disabled = disabled_app
        .clone()
        .oneshot(with_local_connect_info(
            Request::builder().method(Method::GET).uri(events_uri).body(Body::empty()).unwrap(),
        ))
        .await
        .expect("oneshot");
    assert_eq!(events_disabled.status(), StatusCode::NOT_FOUND);

    let Some((enabled_app, _state)) = worker_enabled_fixture().await else {
        super::skip_or_fail_without_db();
        return;
    };

    let heartbeat_enabled = enabled_app
        .clone()
        .oneshot(with_local_connect_info(
            Request::builder()
                .method(Method::POST)
                .uri(heartbeat_uri)
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"running","load_stats":null}"#))
                .unwrap(),
        ))
        .await
        .expect("oneshot");
    assert_eq!(heartbeat_enabled.status(), StatusCode::UNAUTHORIZED);

    let events_enabled = enabled_app
        .clone()
        .oneshot(with_local_connect_info(
            Request::builder().method(Method::GET).uri(events_uri).body(Body::empty()).unwrap(),
        ))
        .await
        .expect("oneshot");
    assert_eq!(events_enabled.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn declared_route_manifest_full_snapshot_matches_worker_enabled_state() {
    let Some(ledger) = worker_enabled_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let actual = render_ledger_snapshot("worker-enabled", &ledger);
    assert_route_ledger_snapshot("route_ledger_worker_enabled.snapshot", &actual);
}

/// REGRESSION (S1): worker mode ON but HTTP replication at its default (false)
/// must NOT expose the worker body surface.
///
/// The routes were mounted on `worker.enabled` alone while
/// `replication_http_auth_middleware` treated "replication disabled" as "no
/// authentication required" and forwarded the request. With `worker.enabled:
/// true` and the default `replication.http.enabled: false`, these endpoints were
/// reachable anonymously — they write replication positions, read the event
/// stream, and mutate worker/task state. They must now be absent (404);
/// `worker_body_routes_are_live_when_worker_mode_enabled` covers the enabled
/// case, which must still be mounted and require the shared secret (401).
#[tokio::test]
async fn worker_body_routes_are_absent_without_http_replication() {
    let Some((app, _state)) = worker_without_replication_fixture().await else {
        super::skip_or_fail_without_db();
        return;
    };

    for (method, uri) in [
        (Method::POST, "/_synapse/worker/v1/workers/probe-worker/heartbeat"),
        (Method::POST, "/_synapse/worker/v1/workers/probe-worker/connect"),
        (Method::GET, "/_synapse/worker/v1/events"),
        (Method::PUT, "/_synapse/worker/v1/replication/probe-worker/events"),
        (Method::POST, "/_synapse/worker/v1/tasks/probe-task/complete"),
    ] {
        let response = app
            .clone()
            .oneshot(with_local_connect_info(
                Request::builder().method(method.clone()).uri(uri).body(Body::empty()).unwrap(),
            ))
            .await
            .expect("oneshot");
        // 404, not 401/403/200: the surface must not be mounted at all. An
        // unauthenticated pass-through returned 200/400 here before the fix.
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{method} {uri} must not be routed when worker.replication.http.enabled is false"
        );
    }
}

#[cfg(feature = "friends")]
#[tokio::test]
async fn friend_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    assert!(has_declared_route(&ledger, Method::GET, "/_matrix/vendor/v1/friends"));
}

#[cfg(feature = "voice-extended")]
#[tokio::test]
async fn voice_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    // ISSUE-13: the two former client-prefix twins (`/_matrix/client/v3/voice/config`
    // and `/_matrix/client/v1/voice/config`) were dead aliases of this single
    // canonical vendor route and were deleted.
    assert!(has_declared_route(&ledger, Method::GET, "/_matrix/vendor/v1/voice/config"));
    assert!(has_declared_route(&ledger, Method::POST, "/_matrix/vendor/v1/voice/upload"));
}

#[cfg(feature = "external-services")]
#[tokio::test]
async fn external_service_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    assert!(has_declared_route(&ledger, Method::GET, "/_synapse/admin/v1/external_services"));
    assert!(has_declared_route(&ledger, Method::POST, "/_synapse/external/webhook/{service_id}"));
}

#[cfg(feature = "widgets")]
#[tokio::test]
async fn widget_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    assert!(has_declared_route(&ledger, Method::POST, "/_matrix/vendor/v1/widgets"));
}

#[cfg(feature = "burn-after-read")]
#[tokio::test]
async fn burn_after_read_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    assert!(has_declared_route(&ledger, Method::PUT, "/_matrix/vendor/v1/rooms/{room_id}/burn"));
}

#[cfg(feature = "cas-sso")]
#[tokio::test]
async fn cas_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    // ⚠️ 2026-09-30 C89：断言从根级 `/login` 改为 **`/_synapse/cas/login`** —— 与
    // `synapse-web/src/routes/route_module.rs` 里 C76 修的是**同一处陈旧预期**（那次只改了 route_module
    // 侧，这条集成断言漏了，而集成批次此前一直因前置作业红被 skip ⇒ 长期不可见）。
    // CAS 模块**有意**挂在 `/_synapse/cas` 前缀下（不占用根级路径、不与 Matrix 标准端点冲突）；
    // 清单里的真实路径见 `derived_route_table_always.inc.rs` 的
    // `RouteEntry::new(Method::GET, "/_synapse/cas/login", "cas")`。
    assert!(has_declared_route(&ledger, Method::GET, "/_synapse/cas/login"));
    assert!(has_declared_route(&ledger, Method::GET, "/_synapse/admin/v1/cas/services"));
}

#[cfg(feature = "voip-tracking")]
#[tokio::test]
async fn voip_tracking_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        super::skip_or_fail_without_db();
        return;
    };
    assert!(has_declared_route(&ledger, Method::PUT, "/_matrix/client/v3/rooms/{room_id}/send/m.call.invite/{txn_id}"));
    assert!(has_declared_route(&ledger, Method::GET, "/_matrix/client/v3/rooms/{room_id}/call/{call_id}"));
}

// ---------------------------------------------------------------------------
// 反向兜底（handoff 2026-xx-15 §5.3 / 建议 B）
// ---------------------------------------------------------------------------
//
// 上面那条 `declared_route_manifest_entries_are_actually_wired` 只覆盖单向
// 「声明 → 接线」。本测试补上**反方向**：「router 源码里实际注册的 `.route(...)`
// 站点 ⊆ 已提交的 ledger 镜像」。缺了它，新增一条 `.route(...)` 却忘了同步
// `derived_route_table_*.inc.rs` 时没有任何测试会红，缺口会顺着
// ledger → SDK 镜像 → codegen 一路"隐形"下去 —— 本次 MSC3814 的
// `POST .../dehydrated_device/{device_id}/events` 一度就只在 derived 表里以 GET
// 出现，SDK 的 `claimDevice()`（POST）在 ledger 里"无对应条目"。
//
// 它**不是**语义级解析器，只是"新路径必须被声明"的粗粒度兜底；权威仍是
// `python3 scripts/contract/gen_derived_routes.py --check`（双向 + 双 lane fixture
// 硬门禁）与 `EXTRACT_STRICT=1 extract_registered.py` 的 S-14 `undeclared` 检查。
// 之所以要这层冗余：那条 Python 门禁是 CI 步骤，而本断言随
// `cargo test --test integration` 一起被本地/CI 直接执行。
//
// 镜像对象刻意选 `tests/unit/fixtures/ledger_export_sdk/all.json`
// （SDK lane × ALL profile，entry_count = 1032）：它是 SDK codegen 真正消费的那一份，
// 且与本次编译启用的 feature 集合无关。不用编译期内联的 `derived_route_manifest`
// 是因为后者受 `#[cfg]` 裁剪，拿它当基线在默认 feature 下会把 feature-gated 路由
// 误报成"未声明"。
//
// 匹配规则（与验证过的原型一致）：路径字面量以 `/_` 开头 → 精确匹配 `(METHOD, path)`；
// 否则（相对路径，例如 `"/capabilities"`）→ 按 `path.ends_with(literal)` 匹配。
// 解析不出方法名的站点直接跳过（宁可少覆盖，不误报）。

/// 被比较的 ledger 镜像：SDK lane × ALL profile 的 fixture。
const LEDGER_MIRROR_FIXTURE: &str = "tests/unit/fixtures/ledger_export_sdk/all.json";
/// 需要扫描的 router 源码根（相对仓库根）。
const ROUTE_SOURCE_ROOT: &str = "synapse-web/src/routes";
/// `any()` 展开后的全部方法。
const ALL_HTTP_METHODS: [&str; 7] = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"];

/// 扫描器用到的正则集合。构造函数集中放置，避免每次调用重新编译。
struct RouteScanRegexes {
    /// `.route(` （转义点号，避免误命中 `aroute(` 之类）。
    route_call: Regex,
    /// `"<字符串字面量>"`，捕获组 1 = 去掉引号后的内容。
    string_lit: Regex,
    /// 方法调用：允许 `axum::routing::` 之类的全限定前缀，捕获组 1 = 方法名。
    /// `\b` 替代 lookbehind（Rust `regex` 不支持后者），足以排除 `output(`/`budget(`。
    method_call: Regex,
    /// `#[cfg(test)]`（容忍空白）。
    cfg_test: Regex,
    /// 行首的 `mod NAME {`（容忍 `pub(crate)` 前缀）。
    gated_mod: Regex,
}

impl RouteScanRegexes {
    fn new() -> Self {
        Self {
            route_call: Regex::new(r"\.route\s*\(").expect("route_call 正则必须可编译"),
            string_lit: Regex::new(r#""((?:[^"\\]|\\.)*)""#).expect("string_lit 正则必须可编译"),
            method_call: Regex::new(
                r"\b(?:[A-Za-z_][A-Za-z0-9_]*::)*(get|post|put|delete|patch|head|options|any)\s*\(",
            )
            .expect("method_call 正则必须可编译"),
            cfg_test: Regex::new(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]").expect("cfg_test 正则必须可编译"),
            gated_mod: Regex::new(r"^(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{")
                .expect("gated_mod 正则必须可编译"),
        }
    }
}

/// 剥离 `//` 行注释与 `/* */` 块注释，**保留**字符串字面量内容（后续要靠它取路径）。
///
/// 逐字节扫描：多字节 UTF-8 的首字节都 ≥ 0x80，永不等于待识别的 ASCII 分隔符，
/// 因此按字节比较是安全的；被丢弃的都是整段 ASCII 定界区间，结果仍是合法 UTF-8。
fn strip_rust_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            out.push(c);
            i += 1;
            while i < bytes.len() {
                let d = bytes[i];
                out.push(d);
                if d == b'\\' {
                    if i + 1 < bytes.len() {
                        out.push(bytes[i + 1]);
                    }
                    i += 2;
                    continue;
                }
                i += 1;
                if d == b'"' {
                    break;
                }
            }
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8(out).expect("剥离注释后仍应是合法 UTF-8")
}

/// 从 `open`（指向 `(` 或 `{`）出发，返回配平的 `)`/`}` 的字节下标；字符串字面量感知。
/// 未配平时返回 `None`（调用方据此跳过，不误报）。
fn match_delim(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let (open_b, close_b) = (bytes.get(open).copied()?, if bytes[open] == b'(' { b')' } else { b'}' });
    let mut depth = 0i32;
    let mut i = open;
    let mut in_str = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == open_b {
            depth += 1;
        } else if c == close_b {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// 从 `#[cfg(test)]` 之后跳过属性/文档注释/空白，若紧接着是 `mod NAME {`，
/// 返回该 `{` 的字节下标；否则返回 `None`（说明这个 `#[cfg(test)]` 挂在非 `mod`
/// 条目上，例如 `fn` —— 只跳过属性本身即可）。
fn gated_mod_open_brace(src: &str, mut i: usize, re: &RouteScanRegexes) -> Option<usize> {
    loop {
        let rest = &src[i..];
        let trimmed = rest.trim_start();
        i += rest.len() - trimmed.len();
        if i >= src.len() {
            return None;
        }
        if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("//") {
            i += trimmed.find('\n').unwrap_or(trimmed.len());
            continue;
        }
        if trimmed.starts_with("#[") {
            i += trimmed.find(']')? + 1;
            continue;
        }
        break;
    }
    let cap = re.gated_mod.captures(&src[i..])?;
    let whole = cap.get(0)?;
    if whole.start() != 0 {
        return None;
    }
    Some(i + whole.end() - 1)
}

/// 剔除所有 `#[cfg(test)] mod NAME { ... }` 块 —— 测试模块里也会构造 router，
/// 其路径不在生产 ledger 里，不剔除必然误报。
///
/// 采用"逐个 `#[cfg(test)]` + 属性跳过 + `mod NAME {` + 括号配平"的写法，
/// 而非"截断到第一个 `#[cfg(test)]`"：`space/lifecycle_query.rs` 里 test mod
/// 在前、router builder 在后，截断式会连生产代码一起丢掉。
fn strip_test_mods(src: &str, re: &RouteScanRegexes) -> String {
    let mut out = String::with_capacity(src.len());
    let mut pos = 0usize;
    while let Some(m) = re.cfg_test.find_at(src, pos) {
        let Some(brace) = gated_mod_open_brace(src, m.end(), re) else {
            out.push_str(&src[pos..m.end()]);
            pos = m.end();
            continue;
        };
        let Some(close) = match_delim(src, brace) else {
            out.push_str(&src[pos..]);
            return out;
        };
        out.push_str(&src[pos..m.start()]);
        pos = close + 1;
    }
    out.push_str(&src[pos..]);
    out
}

/// 递归收集 `*.rs` 文件，跳过 `tests/` 子目录与 `derived_route_table_*.inc.rs`
/// （后者是生成物，不是 router 源）。
fn collect_router_source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            collect_router_source_files(&path, out);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let is_derived = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("derived_route_table_") && n.ends_with(".inc.rs"));
        if is_derived {
            continue;
        }
        out.push(path);
    }
}

/// 从一段（未剥注释的）Rust 源码里抽出所有 `.route("<lit>", <chain>)` 站点，
/// 返回 `(METHOD, literal)` 对（`any()` → 展开成 7 个方法；解析不出方法则跳过）。
fn route_sites_in_source(raw: &str, re: &RouteScanRegexes) -> Vec<(String, String)> {
    let src = strip_test_mods(&strip_rust_comments(raw), re);
    let mut out = Vec::new();
    for m in re.route_call.find_iter(&src) {
        let Some(close) = match_delim(&src, m.end() - 1) else {
            continue;
        };
        let chain = &src[m.end() - 1..=close];
        let Some(lit_cap) = re.string_lit.captures(chain) else {
            continue;
        };
        let literal = lit_cap.get(1).expect("string_lit 必须含捕获组 1").as_str();
        if !literal.starts_with('/') {
            continue;
        }
        let rest = &chain[lit_cap.get(0).expect("string_lit 必须整段命中").end()..];
        let mut methods: Vec<String> = Vec::new();
        for mm in re.method_call.captures_iter(rest) {
            let name = mm.get(1).expect("method_call 必须含捕获组 1").as_str();
            if name == "any" {
                methods.extend(ALL_HTTP_METHODS.iter().map(|s| (*s).to_string()));
            } else {
                methods.push(name.to_ascii_uppercase());
            }
        }
        for method in methods {
            out.push((method, literal.to_string()));
        }
    }
    out
}

/// 读取 ledger 镜像 fixture，返回其 `(METHOD, path)` 集合。
fn ledger_mirror_tuples(fixture: &Path) -> HashSet<(String, String)> {
    let raw = fs::read_to_string(fixture).unwrap_or_else(|e| panic!("无法读取 ledger 镜像 {}：{e}", fixture.display()));
    let json: serde_json::Value = serde_json::from_str(&raw).expect("ledger 镜像必须是合法 JSON");
    let entries = json.get("entries").and_then(|v| v.as_array()).expect("ledger 镜像缺少 entries 数组");
    let mut out = HashSet::with_capacity(entries.len());
    for entry in entries {
        let method = entry.get("method").and_then(|v| v.as_str()).expect("entry.method 必须是字符串");
        let path = entry.get("path").and_then(|v| v.as_str()).expect("entry.path 必须是字符串");
        out.insert((method.to_string(), path.to_string()));
    }
    assert!(!out.is_empty(), "ledger 镜像 {} 为空", fixture.display());
    out
}

/// 扫描器自证（铁律 8：不能失败的门禁不是门禁）：合成源码上必须命中普通站点、
/// 展开 `any()`、识别全限定方法调用，并且**忽略** `#[cfg(test)] mod` 内的站点。
#[test]
fn route_source_scanner_is_well_formed() {
    let re = RouteScanRegexes::new();
    let synthetic = r#"
        pub fn create_demo_router() -> Router<AppState> {
            Router::new()
                .route("/demo", get(handler))
                .route("/demo/{id}", axum::routing::delete(remove).post(update))
                .route("/demo/any", any(proxy))
        }
        // `// .route("/commented_out", get(x))` 必须被注释剥离忽略
        #[cfg(test)]
        mod tests {
            fn build() -> Router<AppState> {
                Router::new().route("/__test_only", get(t))
            }
        }
    "#;

    let sites: HashSet<(String, String)> = route_sites_in_source(synthetic, &re).into_iter().collect();

    let expect = |m: &str, p: &str| (m.to_string(), p.to_string());
    assert!(sites.contains(&expect("GET", "/demo")), "普通站点未命中：{sites:?}");
    assert!(sites.contains(&expect("DELETE", "/demo/{id}")), "全限定 `axum::routing::delete` 未命中：{sites:?}");
    assert!(sites.contains(&expect("POST", "/demo/{id}")), "链式 `.post()` 未命中：{sites:?}");
    for m in ALL_HTTP_METHODS {
        assert!(sites.contains(&expect(m, "/demo/any")), "`any()` 未展开出 {m}：{sites:?}");
    }
    assert!(!sites.iter().any(|(_, p)| p == "/__test_only"), "`#[cfg(test)] mod` 内的路由必须被忽略：{sites:?}");
    assert!(!sites.iter().any(|(_, p)| p == "/commented_out"), "注释里的 `.route()` 必须被忽略：{sites:?}");
    assert_eq!(sites.len(), 1 + 2 + ALL_HTTP_METHODS.len(), "不应有额外命中：{sites:?}");
}

/// 反向兜底：`synapse-web/src/routes/**` 里每个 `.route("<lit>", <chain>)` 站点
/// 都必须能在 ledger 镜像里找到对应条目。
#[test]
fn source_route_sites_are_declared_in_ledger_mirror() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let re = RouteScanRegexes::new();

    let mirror = ledger_mirror_tuples(&root.join(LEDGER_MIRROR_FIXTURE));

    let mut files = Vec::new();
    collect_router_source_files(&root.join(ROUTE_SOURCE_ROOT), &mut files);
    assert!(
        !files.is_empty(),
        "必须至少扫描到一个 router 源文件（CARGO_MANIFEST_DIR 推导错了？root={})",
        root.display()
    );

    let mut scanned = 0usize;
    let mut missing = Vec::new();
    for path in &files {
        let raw = fs::read_to_string(path).expect("router 源码必须可按 UTF-8 读取");
        for (method, literal) in route_sites_in_source(&raw, &re) {
            scanned += 1;
            let covered = if literal.starts_with("/_") {
                mirror.contains(&(method.clone(), literal.clone()))
            } else {
                // 相对路径字面量（例如 `"/capabilities"`）：ledger 里记的是拼接后的
                // 绝对路径，按后缀匹配。
                mirror.iter().any(|(m, p)| m == &method && p.ends_with(&literal))
            };
            if !covered {
                let rel = path.strip_prefix(&root).unwrap_or(path);
                missing.push(format!("{}: {method} {literal}", rel.display()));
            }
        }
    }

    assert!(scanned > 0, "扫描到 0 个 `.route()` 站点 —— 扫描器可能被改坏了");
    assert!(
        missing.is_empty(),
        "以下 `.route()` 站点未在 ledger 镜像（{LEDGER_MIRROR_FIXTURE}）里声明 —— 新增路由后必须重新生成派生产物：\n  \
         python3 scripts/contract/gen_derived_routes.py --bootstrap\n  \
         bash scripts/generate_sdk_ledger_fixtures.sh\n共 {} 处：\n{}",
        missing.len(),
        missing.join("\n")
    );
}
