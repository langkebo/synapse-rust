use axum::{
    body::Body,
    extract::Path,
    http::{Request, StatusCode},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use synapse_rust::cache::CacheManager;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

struct KeyServerMetrics {
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
    total_requests: AtomicUsize,
    delay_ms: u64,
    fail_status: Option<StatusCode>,
}

impl KeyServerMetrics {
    fn new(delay_ms: u64, fail_status: Option<StatusCode>) -> Self {
        Self {
            inflight: AtomicUsize::new(0),
            max_inflight: AtomicUsize::new(0),
            total_requests: AtomicUsize::new(0),
            delay_ms,
            fail_status,
        }
    }

    fn on_start(&self) {
        self.total_requests.fetch_add(1, Ordering::SeqCst);
        let now = self.inflight.fetch_add(1, Ordering::SeqCst) + 1;
        loop {
            let max = self.max_inflight.load(Ordering::SeqCst);
            if now <= max {
                break;
            }
            if self
                .max_inflight
                .compare_exchange(max, now, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break;
            }
        }
    }

    fn on_end(&self) {
        self.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn handle_server_keys(
    metrics: axum::extract::State<Arc<KeyServerMetrics>>,
) -> (StatusCode, Json<serde_json::Value>) {
    metrics.on_start();
    tokio::time::sleep(std::time::Duration::from_millis(metrics.delay_ms)).await;
    metrics.on_end();

    if let Some(status) = metrics.fail_status {
        return (status, Json(json!({})));
    }

    (
        StatusCode::OK,
        Json(json!({
            "server_name": "mock.test",
            "verify_keys": {
                "ed25519:fixed": { "key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }
            },
            "old_verify_keys": {},
            "valid_until_ts": 9999999999999i64
        })),
    )
}

async fn handle_key_query(
    metrics: axum::extract::State<Arc<KeyServerMetrics>>,
    Path((_server_name, key_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    metrics.on_start();
    tokio::time::sleep(std::time::Duration::from_millis(metrics.delay_ms)).await;
    metrics.on_end();

    if let Some(status) = metrics.fail_status {
        return (status, Json(json!({})));
    }

    (
        StatusCode::OK,
        Json(json!({
            "server_name": "mock.test",
            "verify_keys": {
                key_id: { "key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }
            },
            "old_verify_keys": {},
            "valid_until_ts": 9999999999999i64
        })),
    )
}

async fn start_key_server(
    delay_ms: u64,
    fail_status: Option<StatusCode>,
) -> (String, Arc<KeyServerMetrics>) {
    let metrics = Arc::new(KeyServerMetrics::new(delay_ms, fail_status));
    let app = Router::new()
        .route("/_matrix/key/v2/server", get(handle_server_keys))
        .route(
            "/_matrix/key/v2/query/{server_name}/{key_id}",
            get(handle_key_query),
        )
        .with_state(metrics.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("127.0.0.1:{}", addr.port()), metrics)
}

async fn setup_test_app_with_federation_key_fetch_config(
    key_fetch_timeout_ms: u64,
    key_fetch_max_concurrency: usize,
) -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.federation.key_fetch_timeout_ms = key_fetch_timeout_ms;
    container.config.federation.key_fetch_max_concurrency = key_fetch_max_concurrency;

    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    Some(synapse_rust::web::create_router(state))
}

#[tokio::test]
async fn test_federation_key_fetch_respects_timeout_config() {
    let Some(app) = setup_test_app_with_federation_key_fetch_config(1, 8).await else {
        return;
    };
    let (origin, _metrics) = start_key_server(50, None).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/key/v2/query/{}/ed25519:any", origin))
        .body(Body::empty())
        .unwrap();
    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_federation_key_fetch_global_concurrency_limit_is_enforced() {
    let Some(app) = setup_test_app_with_federation_key_fetch_config(5000, 1).await else {
        return;
    };
    let (origin, metrics) = start_key_server(50, None).await;

    let mut tasks = Vec::new();
    for i in 0..8usize {
        let app = app.clone();
        let origin = origin.clone();
        tasks.push(tokio::spawn(async move {
            let request = Request::builder()
                .method("GET")
                .uri(format!("/_matrix/key/v2/query/{}/ed25519:{}", origin, i))
                .body(Body::empty())
                .unwrap();
            let response = app
                .oneshot(super::with_local_connect_info(request))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }

    assert_eq!(metrics.max_inflight.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_federation_key_fetch_backoff_skips_retries() {
    let Some(app) = setup_test_app_with_federation_key_fetch_config(5000, 8).await else {
        return;
    };
    let (origin, metrics) = start_key_server(0, Some(StatusCode::INTERNAL_SERVER_ERROR)).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/key/v2/query/{}/ed25519:backoff", origin))
        .body(Body::empty())
        .unwrap();
    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let first_count = metrics.total_requests.load(Ordering::SeqCst);
    assert!(first_count > 0);

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/key/v2/query/{}/ed25519:backoff", origin))
        .body(Body::empty())
        .unwrap();
    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let second_count = metrics.total_requests.load(Ordering::SeqCst);
    assert_eq!(second_count, first_count);
}
