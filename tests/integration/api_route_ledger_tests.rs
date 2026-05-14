//! End-to-end probe of the [`declared_route_manifest_for`] against the assembled
//! [`Router`].
//!
//! The ledger is the substitute we ship for the route-walker API axum does
//! not expose (R4 / O2 in `docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md`).
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
//! [`declared_route_manifest_for`]: synapse_rust::web::routes::declared_route_manifest_for
//! [`Router`]: axum::Router

use axum::http::Method;
use axum::{body::Body, http::Request};
use futures::stream::{self, StreamExt};
use hyper::StatusCode;
use std::{env, fs, path::PathBuf};
use synapse_rust::web::routes::declared_route_manifest_for;
use synapse_rust::web::routes::route_ledger::RouteLedger;
use synapse_rust::web::routes::state::AppState;
use tokio::sync::OnceCell;
use tower::ServiceExt;

use super::{setup_test_app_with_config, setup_test_app_with_state, with_local_connect_info};

type TestFixture = Option<(axum::Router, AppState)>;

static DEFAULT_FIXTURE: OnceCell<TestFixture> = OnceCell::const_new();
static WORKER_ENABLED_FIXTURE: OnceCell<TestFixture> = OnceCell::const_new();
static OPENCLAW_ENABLED_FIXTURE: OnceCell<TestFixture> = OnceCell::const_new();
static DEFAULT_LEDGER: OnceCell<Option<RouteLedger>> = OnceCell::const_new();
static WORKER_ENABLED_LEDGER: OnceCell<Option<RouteLedger>> = OnceCell::const_new();
static OPENCLAW_ENABLED_LEDGER: OnceCell<Option<RouteLedger>> = OnceCell::const_new();

async fn default_fixture() -> TestFixture {
    DEFAULT_FIXTURE
        .get_or_init(|| async { setup_test_app_with_state().await })
        .await
        .clone()
}

async fn worker_enabled_fixture() -> TestFixture {
    WORKER_ENABLED_FIXTURE
        .get_or_init(|| async {
            setup_test_app_with_config(|container| {
                container.config.federation.allow_ingress = true;
                container.config.worker.enabled = true;
                container.config.worker.replication.http.enabled = true;
                container.config.worker.replication.http.secret =
                    Some("test_worker_secret".to_string());
                container.config.worker.replication.http.secret_path = None;
            })
            .await
        })
        .await
        .clone()
}

async fn openclaw_enabled_fixture() -> TestFixture {
    OPENCLAW_ENABLED_FIXTURE
        .get_or_init(|| async {
            setup_test_app_with_config(|container| {
                container.config.federation.allow_ingress = true;
                container.config.experimental.openclaw_routes_enabled = true;
            })
            .await
        })
        .await
        .clone()
}

async fn default_ledger() -> Option<RouteLedger> {
    DEFAULT_LEDGER
        .get_or_init(|| async {
            default_fixture()
                .await
                .as_ref()
                .map(|(_, state)| declared_route_manifest_for(state))
        })
        .await
        .clone()
}

async fn worker_enabled_ledger() -> Option<RouteLedger> {
    WORKER_ENABLED_LEDGER
        .get_or_init(|| async {
            worker_enabled_fixture()
                .await
                .as_ref()
                .map(|(_, state)| declared_route_manifest_for(state))
        })
        .await
        .clone()
}

async fn openclaw_enabled_ledger() -> Option<RouteLedger> {
    OPENCLAW_ENABLED_LEDGER
        .get_or_init(|| async {
            openclaw_enabled_fixture()
                .await
                .as_ref()
                .map(|(_, state)| declared_route_manifest_for(state))
        })
        .await
        .clone()
}

fn allow_methods(allow_header: &str) -> std::collections::HashSet<String> {
    allow_header
        .split(',')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn has_declared_route(
    ledger: &synapse_rust::web::routes::route_ledger::RouteLedger,
    method: Method,
    path: &str,
) -> bool {
    ledger
        .iter()
        .any(|entry| entry.method == method && entry.path == path)
}

fn render_ledger_snapshot(
    snapshot_name: &str,
    ledger: &synapse_rust::web::routes::route_ledger::RouteLedger,
) -> String {
    let mut lines: Vec<String> = ledger
        .iter()
        .map(|entry| {
            format!(
                "{} {} [{}]",
                entry.method.as_str(),
                entry.path,
                entry.registered_by
            )
        })
        .collect();
    lines.sort();

    format!(
        "# route-ledger snapshot: {snapshot_name}\ncount: {}\n\n{}\n",
        lines.len(),
        lines.join("\n")
    )
}

fn route_ledger_snapshot_path(snapshot_file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("integration")
        .join("snapshots")
        .join(snapshot_file)
}

fn assert_route_ledger_snapshot(snapshot_file: &str, actual: String) {
    let path = route_ledger_snapshot_path(snapshot_file);
    if env::var_os("UPDATE_ROUTE_LEDGER_SNAPSHOTS").is_some() {
        let parent = path.parent().expect("snapshot path should have parent");
        fs::create_dir_all(parent).expect("create snapshot directory");
        fs::write(&path, &actual).expect("write route-ledger snapshot");
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
    // (1190 on 2026-05-02) + ~10% headroom. If you genuinely need to raise
    // it, refresh §7.2 with a fresh probe-time datapoint and decide whether
    // PROBE_CONCURRENCY needs raising or sampling needs to land first.
    const WARNING_ROUTE_COUNT: usize = 1300;

    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
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
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    let report = ledger
        .validate()
        .expect("declared_route_manifest_for must be free of duplicate (method, path) tuples");
    assert!(
        report.unique_tuples >= 1,
        "ledger should declare at least one route, got {}",
        report.unique_tuples
    );
    assert_eq!(report.total_entries, report.unique_tuples);
}

#[tokio::test]
async fn declared_route_manifest_entries_are_actually_wired() {
    let Some((app, state)) = default_fixture().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };

    let ledger = declared_route_manifest_for(&state);
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
                let req = Request::builder()
                    .method("PATCH")
                    .uri(entry.path)
                    .body(Body::empty())
                    .unwrap();

                let response = app
                    .oneshot(with_local_connect_info(req))
                    .await
                    .expect("oneshot");
                let status = response.status();

                if status == StatusCode::NOT_FOUND {
                    return Outcome::Missing(format!(
                        "{} {} (declared by {})",
                        entry.method, entry.path, entry.registered_by
                    ));
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
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    let actual = render_ledger_snapshot("default", &ledger);
    assert_route_ledger_snapshot("route_ledger_default.snapshot", actual);
}

#[tokio::test]
async fn worker_body_routes_follow_runtime_flag_in_ledger() {
    let Some(disabled_ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(!has_declared_route(
        &disabled_ledger,
        Method::POST,
        "/_synapse/worker/v1/workers/{worker_id}/heartbeat"
    ));
    assert!(!has_declared_route(
        &disabled_ledger,
        Method::GET,
        "/_synapse/worker/v1/events"
    ));

    let Some(enabled_ledger) = worker_enabled_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &enabled_ledger,
        Method::POST,
        "/_synapse/worker/v1/workers/{worker_id}/heartbeat"
    ));
    assert!(has_declared_route(
        &enabled_ledger,
        Method::GET,
        "/_synapse/worker/v1/events"
    ));
}

#[tokio::test]
async fn worker_body_routes_are_live_when_worker_mode_enabled() {
    let heartbeat_uri = "/_synapse/worker/v1/workers/probe-worker/heartbeat";
    let events_uri = "/_synapse/worker/v1/events";

    let Some((disabled_app, _state)) = default_fixture().await else {
        eprintln!("Skipping: integration test database is not available");
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
            Request::builder()
                .method(Method::GET)
                .uri(events_uri)
                .body(Body::empty())
                .unwrap(),
        ))
        .await
        .expect("oneshot");
    assert_eq!(events_disabled.status(), StatusCode::NOT_FOUND);

    let Some((enabled_app, _state)) = worker_enabled_fixture().await else {
        eprintln!("Skipping: integration test database is not available");
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
            Request::builder()
                .method(Method::GET)
                .uri(events_uri)
                .body(Body::empty())
                .unwrap(),
        ))
        .await
        .expect("oneshot");
    assert_eq!(events_enabled.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn declared_route_manifest_full_snapshot_matches_worker_enabled_state() {
    let Some(ledger) = worker_enabled_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    let actual = render_ledger_snapshot("worker-enabled", &ledger);
    assert_route_ledger_snapshot("route_ledger_worker_enabled.snapshot", actual);
}

#[cfg(feature = "friends")]
#[tokio::test]
async fn friend_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &ledger,
        Method::GET,
        "/_matrix/client/v3/friends"
    ));
}

#[cfg(feature = "voice-extended")]
#[tokio::test]
async fn voice_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &ledger,
        Method::GET,
        "/_matrix/client/r0/voice/config"
    ));
    assert!(has_declared_route(
        &ledger,
        Method::GET,
        "/_matrix/client/v1/voice/config"
    ));
    assert!(has_declared_route(
        &ledger,
        Method::POST,
        "/_matrix/client/r0/voice/upload"
    ));
}

#[cfg(feature = "external-services")]
#[tokio::test]
async fn external_service_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &ledger,
        Method::GET,
        "/_synapse/admin/v1/external_services"
    ));
    assert!(has_declared_route(
        &ledger,
        Method::POST,
        "/_synapse/external/webhook/{service_id}"
    ));
}

#[cfg(feature = "widgets")]
#[tokio::test]
async fn widget_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &ledger,
        Method::POST,
        "/_matrix/client/v1/widgets"
    ));
}

#[cfg(feature = "burn-after-read")]
#[tokio::test]
async fn burn_after_read_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &ledger,
        Method::PUT,
        "/_matrix/client/v1/rooms/{room_id}/burn"
    ));
}

#[cfg(feature = "cas-sso")]
#[tokio::test]
async fn cas_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(&ledger, Method::GET, "/login"));
    assert!(has_declared_route(
        &ledger,
        Method::GET,
        "/_synapse/admin/v1/cas/services"
    ));
}

#[cfg(feature = "openclaw-routes")]
#[tokio::test]
async fn openclaw_routes_follow_runtime_flag_in_ledger() {
    let Some(disabled_ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(!has_declared_route(
        &disabled_ledger,
        Method::GET,
        "/_matrix/client/unstable/org.synapse_rust.openclaw/connections"
    ));
    assert!(!has_declared_route(
        &disabled_ledger,
        Method::GET,
        "/connections"
    ));

    let Some(enabled_ledger) = openclaw_enabled_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &enabled_ledger,
        Method::GET,
        "/_matrix/client/unstable/org.synapse_rust.openclaw/connections"
    ));
    assert!(has_declared_route(
        &enabled_ledger,
        Method::GET,
        "/connections"
    ));
}

#[cfg(feature = "voip-tracking")]
#[tokio::test]
async fn voip_tracking_routes_are_declared_when_feature_enabled() {
    let Some(ledger) = default_ledger().await else {
        eprintln!("Skipping: integration test database is not available");
        return;
    };
    assert!(has_declared_route(
        &ledger,
        Method::PUT,
        "/_matrix/client/r0/rooms/{room_id}/send/m.call.invite/{txn_id}"
    ));
    assert!(has_declared_route(
        &ledger,
        Method::GET,
        "/_matrix/client/v3/rooms/{room_id}/call/{call_id}"
    ));
}
