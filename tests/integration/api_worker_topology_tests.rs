//! App-level coverage for the two worker topology endpoints (W-A):
//!
//! * `GET /_synapse/worker/v1/topology` — the static worker catalogue plus the
//!   registry's `live` list;
//! * `GET /_synapse/worker/v1/topology/validate` — the configuration validation
//!   with the runtime reconciliation attached.
//!
//! The decision logic (`evaluate_runtime_topology`, `validate_worker_config`,
//! `build_topology_validation_response`) is already unit-tested. What nothing
//! covered before this file is the wiring: that both handlers are mounted
//! unconditionally, that they sit behind `admin_auth_middleware`, and that the
//! registry rows written by the real register/heartbeat routes show up in the
//! responses. So these tests drive HTTP end to end rather than calling the
//! builders directly.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_common::config::worker::InstanceLocationConfig;
use tower::ServiceExt;

/// Shared secret the test app's `replication_http_auth_middleware` expects.
const WORKER_SECRET: &str = "test_worker_secret";

/// Every stream writer route this build can express. Kept as a literal here so
/// a stream silently disappearing from the response fails the test.
const STREAM_NAMES: [&str; 8] =
    ["events", "typing", "to_device", "account_data", "receipts", "presence", "push_rules", "device_lists"];

/// Build an isolated app whose container config has been tweaked before the
/// router is assembled.
///
/// `TestContext` (a fresh cloned schema per test) is used instead of
/// `setup_fresh_test_app_with_config`, which shares one schema across tests:
/// the `live` assertions below would then see workers registered by other
/// tests.
async fn worker_app(configure: impl FnOnce(&mut synapse_common::Config)) -> Option<axum::Router> {
    let ctx = super::TestContext::new().await?;
    let super::TestContext { app: _, mut state, pool: _ } = ctx;

    {
        let container = Arc::make_mut(&mut state.services);
        let config = super::config_mut(container);
        configure(config);
    }

    Some(synapse_web::create_router(state))
}

/// Enable the worker surface (the register/heartbeat routes are mounted only
/// when `worker.enabled && worker.replication.http.enabled`).
fn enable_worker_surface(config: &mut synapse_common::Config) {
    config.worker.enabled = true;
    config.worker.replication.http.enabled = true;
    config.worker.replication.http.secret = Some(WORKER_SECRET.to_string());
    config.worker.replication.http.secret_path = None;
}

async fn get(app: &axum::Router, token: Option<&str>, uri: &str) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    let request = builder.body(Body::empty()).unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap_or(Value::Null) };
    (status, json)
}

/// Register a worker through the real admin route and assert it was accepted.
async fn register_worker(app: &axum::Router, admin_token: &str, worker_id: &str, worker_name: &str, worker_type: &str) {
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/register")
        .header("Authorization", format!("Bearer {admin_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "worker_id": worker_id,
                "worker_name": worker_name,
                "worker_type": worker_type,
                "host": "127.0.0.1",
                "port": 8081,
                "config": null,
                "metadata": null,
                "version": "test"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "worker registration must succeed");
}

/// Send one heartbeat through the real secret-authenticated route.
async fn send_heartbeat(app: &axum::Router, worker_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/workers/{worker_id}/heartbeat"))
        .header("x-synapse-worker-secret", WORKER_SECRET)
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "status": "running" }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "heartbeat must be accepted");
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("expected a JSON array, got {value}"))
        .iter()
        .map(|entry| entry.as_str().unwrap_or_else(|| panic!("expected strings, got {entry}")).to_string())
        .collect()
}

/// Both endpoints are admin-only even though `create_worker_admin_router`
/// mounts them unconditionally.
#[tokio::test]
async fn topology_endpoints_require_admin() {
    let Some(app) = worker_app(|_| {}).await else {
        return;
    };
    let user_token = super::create_test_user(&app).await;

    for uri in ["/_synapse/worker/v1/topology", "/_synapse/worker/v1/topology/validate"] {
        let (status, _) = get(&app, None, uri).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} must reject an anonymous caller");

        let (status, _) = get(&app, Some(&user_token), uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri} must reject a non-admin token");
    }

    let (admin_token, _admin_user) = super::get_admin_token(&app).await;

    // A worker-disabled, empty-registry deployment still answers: this is the
    // route being reachable at all, not a topology claim.
    let (status, body) = get(&app, Some(&admin_token), "/_synapse/worker/v1/topology").await;
    assert_eq!(status, StatusCode::OK, "topology must answer the admin token: {body}");
    assert!(body["workers"].is_array(), "the flattened catalogue must expose `workers`: {body}");
    assert!(body["deployment_presets"].is_array(), "the flattened catalogue must expose `deployment_presets`");
    assert_eq!(strings(&body["live"]), Vec::<String>::new(), "the isolated schema must have no registered workers");
}

/// A registered + heartbeating worker appears in `live` with a fresh age.
#[tokio::test]
async fn topology_reports_the_live_registry() {
    let Some(app) = worker_app(enable_worker_surface).await else {
        return;
    };
    let (admin_token, _admin_user) = super::get_admin_token(&app).await;
    let worker_id = format!("topology-live-{}", rand::random::<u32>());
    register_worker(&app, &admin_token, &worker_id, "topology-test-worker", "frontend").await;
    send_heartbeat(&app, &worker_id).await;

    let (status, body) = get(&app, Some(&admin_token), "/_synapse/worker/v1/topology").await;
    assert_eq!(status, StatusCode::OK, "topology must answer: {body}");

    let live = body["live"].as_array().expect("`live` must be an array");
    let entry = live
        .iter()
        .find(|entry| entry["worker_id"].as_str() == Some(worker_id.as_str()))
        .unwrap_or_else(|| panic!("`live` must contain {worker_id}: {body}"));

    assert_eq!(entry["instance_name"].as_str(), Some("topology-test-worker"));
    assert_eq!(entry["worker_type"].as_str(), Some("frontend"));
    assert_eq!(entry["status"].as_str(), Some("running"));
    assert!(entry["last_heartbeat_ts"].as_i64().is_some(), "the heartbeat must have been recorded");
    let age = entry["last_heartbeat_age_secs"].as_i64().expect("age must be reported once a heartbeat exists");
    assert!((0..=5).contains(&age), "the fresh heartbeat must be seconds old, got {age}");
}

/// The validate endpoint reconciles the configured instance map against the
/// registry: declared instances that are running are neither `missing` nor
/// `unexpected`, and `sync_worker` makes the sync probe resolve to a
/// synchrotron.
#[tokio::test]
async fn topology_validate_reconciles_config_against_registry() {
    let Some(app) = worker_app(|config| {
        enable_worker_surface(config);
        // A non-empty `instance_map` requires replication, else validation
        // reports a hard error.
        config.worker.replication.enabled = true;
        config.worker.instance_map.insert(
            "event_persister".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8081, tls: false },
        );
        // `sync_worker` is the `instance_map` key `WorkerType::Synchrotron`
        // declares; naming it `synchrotron` would not infer the worker type.
        config.worker.instance_map.insert(
            "sync_worker".to_string(),
            InstanceLocationConfig { host: "127.0.0.1".to_string(), port: 8082, tls: false },
        );
    })
    .await
    else {
        return;
    };

    let (admin_token, _admin_user) = super::get_admin_token(&app).await;
    for (name, worker_type) in [("event_persister", "event_persister"), ("sync_worker", "synchrotron")] {
        let worker_id = format!("topology-{name}-{}", rand::random::<u32>());
        register_worker(&app, &admin_token, &worker_id, name, worker_type).await;
        send_heartbeat(&app, &worker_id).await;
    }

    let (status, body) = get(&app, Some(&admin_token), "/_synapse/worker/v1/topology/validate").await;
    assert_eq!(status, StatusCode::OK, "validate must answer: {body}");

    assert_eq!(body["worker_enabled"].as_bool(), Some(true));
    assert_eq!(body["instance_name"].as_str(), Some("master"));
    assert_eq!(body["replication_enabled"].as_bool(), Some(true));
    assert_eq!(body["replication_http_enabled"].as_bool(), Some(true));

    let mut known = strings(&body["known_instances"]);
    known.sort();
    assert_eq!(known, vec!["event_persister", "master", "sync_worker"]);

    // The configuration itself is consistent.
    assert_eq!(body["validation"]["valid"].as_bool(), Some(true), "validation: {body}");
    assert_eq!(strings(&body["validation"]["errors"]), Vec::<String>::new());
    assert_eq!(strings(&body["validation"]["warnings"]), Vec::<String>::new());

    // All eight streams keep the default owner, and the probe table reports
    // three entries.
    let mut streams = body["stream_writers"]
        .as_array()
        .expect("`stream_writers` must be an array")
        .iter()
        .map(|entry| {
            assert_eq!(
                strings(&entry["owners"]),
                vec!["master"],
                "stream {} must default to master",
                entry["stream_name"]
            );
            entry["stream_name"].as_str().expect("stream_name must be a string").to_string()
        })
        .collect::<Vec<_>>();
    streams.sort();
    let mut expected_streams = STREAM_NAMES.to_vec();
    expected_streams.sort();
    assert_eq!(streams, expected_streams);

    let probes = body["route_owner_expectations"].as_array().expect("`route_owner_expectations` must be an array");
    assert_eq!(probes.len(), 3);
    let sync_probe = probes.iter().find(|probe| probe["probe"] == "sync").expect("a sync probe must exist");
    assert_eq!(sync_probe["path"].as_str(), Some("/_matrix/client/v3/sync"));
    // `sync_worker` is declared, so sync is expected to be served by a
    // synchrotron rather than by master.
    assert_eq!(sync_probe["expected_owner"].as_str(), Some("synchrotron"));
    let media_probe = probes.iter().find(|probe| probe["probe"] == "media").expect("a media probe must exist");
    assert_eq!(media_probe["expected_owner"].as_str(), Some("master"));
    let federation_probe =
        probes.iter().find(|probe| probe["probe"] == "federation").expect("a federation probe must exist");
    assert_eq!(federation_probe["expected_owner"].as_str(), Some("master"));

    // Both declared instances registered and heartbeated, so the runtime view
    // agrees with the configuration.
    let runtime = &body["runtime"];
    let mut declared = strings(&runtime["declared_instances"]);
    declared.sort();
    assert_eq!(declared, vec!["event_persister", "master", "sync_worker"]);
    let mut live = strings(&runtime["live_instances"]);
    live.sort();
    assert_eq!(live, vec!["event_persister", "sync_worker"]);
    assert_eq!(strings(&runtime["missing"]), Vec::<String>::new(), "runtime: {runtime}");
    assert_eq!(strings(&runtime["stale"]), Vec::<String>::new(), "runtime: {runtime}");
    assert_eq!(strings(&runtime["unexpected"]), Vec::<String>::new(), "runtime: {runtime}");
    assert_eq!(strings(&runtime["unowned_streams"]), Vec::<String>::new(), "runtime: {runtime}");
    assert_eq!(runtime["valid"].as_bool(), Some(true), "runtime: {runtime}");

    // Write forwarding is off by default, so the report must still be present
    // and say so rather than being omitted.
    assert!(body["write_forwarding"].is_object(), "`write_forwarding` must be reported: {body}");
    assert_eq!(body["write_forwarding"]["enabled"].as_bool(), Some(false));
}
