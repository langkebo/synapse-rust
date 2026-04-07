use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;
use synapse_rust::cache::CacheManager;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

async fn setup_test_app_with_replication_secret() -> Option<(axum::Router, String, String)> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.worker.enabled = true;
    container.config.worker.replication.http.enabled = true;
    container.config.worker.replication.http.secret = Some("test_worker_secret".to_string());
    container.config.worker.replication.http.secret_path = None;

    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state);

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

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_worker_endpoints_require_replication_secret_when_enabled() {
    let Some((app, _admin_token, worker_id)) = setup_test_app_with_replication_secret().await
    else {
        return;
    };

    let heartbeat_body = json!({
        "status": "running",
        "load_stats": null
    })
    .to_string();

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/worker/v1/workers/{}/heartbeat",
            worker_id
        ))
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body.clone()))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/worker/v1/workers/{}/heartbeat",
            worker_id
        ))
        .header("x-synapse-worker-secret", "wrong_secret")
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body.clone()))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_synapse/worker/v1/workers/{}/heartbeat",
            worker_id
        ))
        .header("x-synapse-worker-secret", "test_worker_secret")
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_worker_endpoints_do_not_require_replication_secret_when_disabled() {
    let Some(pool) = super::get_test_pool().await else {
        return;
    };
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.worker.enabled = true;
    container.config.worker.replication.http.enabled = false;
    container.config.worker.replication.http.secret = Some("test_worker_secret".to_string());
    container.config.worker.replication.http.secret_path = None;

    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state);

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
        .uri(format!(
            "/_synapse/worker/v1/workers/{}/heartbeat",
            worker_id
        ))
        .header("Content-Type", "application/json")
        .body(Body::from(heartbeat_body))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_admin_worker_endpoints_still_require_admin_jwt() {
    let Some((app, admin_token, _worker_id)) = setup_test_app_with_replication_secret().await
    else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/worker/v1/workers")
        .header("x-synapse-worker-secret", "test_worker_secret")
        .body(Body::empty())
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/worker/v1/workers")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
