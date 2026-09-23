//! S4 end-to-end coverage: the worker heartbeat route must persist `load_stats`
//! and the statistics API must return the values.
//!
//! S1–S3 added the schema column, the `upsert_statistics` storage call and the
//! reader, but nothing in the repo exercised the whole chain over HTTP. These
//! tests do: register a worker, POST a heartbeat with `load_stats`, then assert
//! both the `worker_statistics` row and the `/_synapse/worker/v1/statistics`
//! response. The final test drives the payload through
//! `synapse_rust::worker::heartbeat::collect_load_stats`, the function the worker
//! binary actually calls.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use synapse_rust::worker::heartbeat::collect_load_stats;
use tower::ServiceExt;

/// Shared secret the test app's `replication_http_auth_middleware` expects.
const WORKER_SECRET: &str = "test_worker_secret";

/// Columns asserted on both the raw row and the statistics API.
const LOAD_STATS_COLUMNS: &str = "cpu_usage, memory_usage, active_connections, \
                                  requests_per_second, average_latency_ms, queue_depth";

/// Build a fresh, isolated app with the worker body surface mounted.
///
/// `setup_fresh_test_app_with_pool` cannot be used directly here: the worker
/// body surface is mounted only when `worker.enabled && worker.replication.http.enabled`,
/// which must be set **before** `create_router` assembles it. `TestContext` is
/// the helper underneath `setup_fresh_test_app_with_pool`, so this reuses the
/// same fresh cloned schema while allowing the container config to be tweaked.
async fn setup_worker_app() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let ctx = super::TestContext::new().await?;
    let super::TestContext { app: _, mut state, pool } = ctx;

    {
        let container = Arc::make_mut(&mut state.services);
        let config = super::config_mut(container);
        config.worker.enabled = true;
        config.worker.replication.http.enabled = true;
        config.worker.replication.http.secret = Some(WORKER_SECRET.to_string());
        config.worker.replication.http.secret_path = None;
    }

    let app = synapse_web::create_router(state);
    Some((app, pool))
}

async fn register_worker(app: &axum::Router, admin_token: &str, worker_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/register")
        .header("Authorization", format!("Bearer {admin_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "worker_id": worker_id,
                "worker_name": "heartbeat-test-worker",
                "worker_type": "frontend",
                "host": "127.0.0.1",
                "port": 8080,
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

/// POST one heartbeat through the real route (secret-authenticated, not the
/// storage layer) and return the response.
async fn send_heartbeat(app: &axum::Router, worker_id: &str, load_stats: Value) -> axum::response::Response {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/workers/{worker_id}/heartbeat"))
        .header("x-synapse-worker-secret", WORKER_SECRET)
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "status": "running", "load_stats": load_stats }).to_string()))
        .unwrap();

    app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap()
}

/// Fetch the statistics entry for `worker_id` through the admin route.
async fn statistics_for(app: &axum::Router, admin_token: &str, worker_id: &str) -> Value {
    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/worker/v1/statistics?limit=100")
        .header("Authorization", format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "statistics route must answer the admin token");

    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let stats: Value = serde_json::from_slice(&body).expect("statistics response must be JSON");

    stats
        .as_array()
        .expect("statistics must be a JSON array")
        .iter()
        .find(|entry| entry["worker_id"].as_str() == Some(worker_id))
        .cloned()
        .unwrap_or_else(|| panic!("statistics must contain worker {worker_id}: {stats}"))
}

async fn load_stats_row(pool: &sqlx::PgPool, worker_id: &str) -> sqlx::postgres::PgRow {
    let sql = format!("SELECT {LOAD_STATS_COLUMNS} FROM worker_statistics WHERE worker_id = $1");
    sqlx::query(&sql)
        .bind(worker_id)
        .fetch_one(pool)
        .await
        .expect("worker_statistics row must exist after the heartbeat upsert")
}

#[tokio::test]
async fn worker_heartbeat_load_stats_are_persisted_and_returned_non_null() {
    let Some((app, pool)) = setup_worker_app().await else {
        return;
    };
    let (admin_token, _admin_user) = super::get_admin_token(&app).await;
    let worker_id = format!("hb-full-{}", rand::random::<u32>());
    register_worker(&app, &admin_token, &worker_id).await;

    let response = send_heartbeat(
        &app,
        &worker_id,
        json!({
            "cpu_usage": 0.5,
            "memory_usage": 2048,
            "active_connections": 7,
            "requests_per_second": 2.0,
            "average_latency_ms": 12.5,
            "queue_depth": 42
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "heartbeat route must accept the load_stats payload");

    // (i) the UPSERT really wrote the reported values into `worker_statistics`.
    let row = load_stats_row(&pool, &worker_id).await;
    assert_eq!(row.get::<Option<f32>, _>("cpu_usage"), Some(0.5));
    assert_eq!(row.get::<Option<i64>, _>("memory_usage"), Some(2048));
    assert_eq!(row.get::<Option<i32>, _>("active_connections"), Some(7));
    assert_eq!(row.get::<Option<f32>, _>("requests_per_second"), Some(2.0));
    assert_eq!(row.get::<Option<f32>, _>("average_latency_ms"), Some(12.5));
    assert_eq!(row.get::<Option<i32>, _>("queue_depth"), Some(42));

    // (ii) the statistics API surfaces them as non-NULL, not just the raw table.
    let entry = statistics_for(&app, &admin_token, &worker_id).await;
    assert_eq!(entry["cpu_usage"].as_f64(), Some(0.5));
    assert_eq!(entry["memory_usage"].as_i64(), Some(2048));
    assert_eq!(entry["active_connections"].as_i64(), Some(7));
    assert_eq!(entry["requests_per_second"].as_f64(), Some(2.0));
    assert_eq!(entry["average_latency_ms"].as_f64(), Some(12.5));
    assert_eq!(entry["queue_depth"].as_i64(), Some(42));
}

#[tokio::test]
async fn worker_heartbeat_from_collector_persists_only_queue_depth() {
    let Some((app, pool)) = setup_worker_app().await else {
        return;
    };
    let (admin_token, _admin_user) = super::get_admin_token(&app).await;
    let worker_id = format!("hb-collector-{}", rand::random::<u32>());
    register_worker(&app, &admin_token, &worker_id).await;

    // Exactly what the worker binary sends: `collect_load_stats` fills
    // queue_depth from the task queue and leaves CPU/memory NULL (a real system
    // collector is a separate step, not faked here).
    let stats = collect_load_stats(Some(23));
    let response = send_heartbeat(&app, &worker_id, serde_json::to_value(&stats).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK, "heartbeat route must accept the collector payload");

    let row = load_stats_row(&pool, &worker_id).await;
    assert_eq!(row.get::<Option<i32>, _>("queue_depth"), Some(23));
    assert_eq!(row.get::<Option<f32>, _>("cpu_usage"), None, "S4 must not fake CPU usage");
    assert_eq!(row.get::<Option<i64>, _>("memory_usage"), None, "S4 must not fake memory usage");

    let entry = statistics_for(&app, &admin_token, &worker_id).await;
    assert_eq!(entry["queue_depth"].as_i64(), Some(23));
    assert!(entry["cpu_usage"].is_null());
    assert!(entry["memory_usage"].is_null());
}
