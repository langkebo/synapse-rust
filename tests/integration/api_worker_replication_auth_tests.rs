use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::ServiceContainer;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;

async fn setup_test_app_with_replication_secret() -> Option<(axum::Router, String, String)> {
    let pool = super::require_test_pool().await;
    let mut container = ServiceContainer::new_test_with_pool(pool).await;
    super::config_mut(&mut container).worker.enabled = true;
    super::config_mut(&mut container).worker.replication.http.enabled = true;
    super::config_mut(&mut container).worker.replication.http.secret = Some("test_worker_secret".to_string());
    super::config_mut(&mut container).worker.replication.http.secret_path = None;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    let app = synapse_web::create_router(state);

    let (admin_token, _admin_user) = super::get_admin_token(&app).await;
    let worker_id = format!("worker-{}", rand::random::<u32>());
    register_worker(&app, &admin_token, &worker_id).await;

    Some((app, admin_token, worker_id))
}

async fn register_worker(app: &axum::Router, admin_token: &str, worker_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/worker/v1/register")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "worker_id": worker_id,
                "worker_name": "test-worker",
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
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_worker_endpoints_require_replication_secret_when_enabled() {
    let Some((app, _admin_token, worker_id)) = setup_test_app_with_replication_secret().await else {
        return;
    };

    let heartbeat_body = json!({
        "status": "running",
        "load_stats": null
    })
    .to_string();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/workers/{}/heartbeat", worker_id))
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body.clone()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/workers/{}/heartbeat", worker_id))
        .header("x-synapse-worker-secret", "wrong_secret")
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body.clone()))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/workers/{}/heartbeat", worker_id))
        .header("x-synapse-worker-secret", "test_worker_secret")
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// When HTTP replication is disabled, the worker body surface must not exist.
///
/// This test previously asserted the OPPOSITE — it was named
/// `..._do_not_require_replication_secret_when_disabled` and expected **200** for
/// a heartbeat sent with no credential at all. That pinned an unauthenticated
/// write surface: with `worker.enabled: true` and `replication.http.enabled` at
/// its `false` default, anyone could post worker heartbeats/command completions,
/// write replication positions and read the event stream, because
/// `replication_http_auth_middleware` treated "replication disabled" as "no
/// authentication required" and the routes were mounted on `worker.enabled`
/// alone.
///
/// The expectation is now inverted (see S1 in docs/audit/DB_REVIEW_2026-09-17.md
/// §13.7): the surface is not mounted at all, so the request 404s. The
/// credential-required path (replication enabled) is covered by the tests above.
#[tokio::test]
async fn test_worker_body_endpoints_are_not_mounted_when_replication_http_disabled() {
    let pool = super::require_test_pool().await;
    let mut container = ServiceContainer::new_test_with_pool(pool).await;
    super::config_mut(&mut container).worker.enabled = true;
    super::config_mut(&mut container).worker.replication.http.enabled = false;
    super::config_mut(&mut container).worker.replication.http.secret = Some("test_worker_secret".to_string());
    super::config_mut(&mut container).worker.replication.http.secret_path = None;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    let app = synapse_web::create_router(state);

    let (admin_token, _admin_user) = super::get_admin_token(&app).await;
    let worker_id = format!("worker-{}", rand::random::<u32>());
    register_worker(&app, &admin_token, &worker_id).await;

    let heartbeat_body = json!({
        "status": "running",
        "load_stats": null
    })
    .to_string();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/workers/{}/heartbeat", worker_id))
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body))
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the worker body surface must not be mounted (and must never be a credential-free pass-through) \
         when worker.replication.http.enabled is false"
    );
}

#[tokio::test]
async fn test_admin_worker_endpoints_still_require_admin_jwt() {
    let Some((app, admin_token, _worker_id)) = setup_test_app_with_replication_secret().await else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/worker/v1/workers")
        .header("x-synapse-worker-secret", "test_worker_secret")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/worker/v1/workers")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
