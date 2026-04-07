use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Json, Router,
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::Signer;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::federation::signing::canonical_federation_request_bytes;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

struct KeyFetchMetrics {
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
    delay_ms: u64,
    key_id: String,
    key_b64: String,
}

impl KeyFetchMetrics {
    fn new(delay_ms: u64, key_id: String, key_b64: String) -> Self {
        Self {
            inflight: AtomicUsize::new(0),
            max_inflight: AtomicUsize::new(0),
            delay_ms,
            key_id,
            key_b64,
        }
    }

    fn on_start(&self) {
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
    metrics: axum::extract::State<Arc<KeyFetchMetrics>>,
) -> (StatusCode, Json<Value>) {
    metrics.on_start();
    tokio::time::sleep(std::time::Duration::from_millis(metrics.delay_ms)).await;
    metrics.on_end();

    (
        StatusCode::OK,
        Json(json!({
            "server_name": "mock.test",
            "verify_keys": {
                metrics.key_id.clone(): { "key": metrics.key_b64.clone() }
            },
            "old_verify_keys": {},
            "valid_until_ts": 9999999999999i64
        })),
    )
}

async fn start_key_server(
    delay_ms: u64,
    key_id: &str,
    key_b64: &str,
) -> (String, Arc<KeyFetchMetrics>) {
    let metrics = Arc::new(KeyFetchMetrics::new(
        delay_ms,
        key_id.to_string(),
        key_b64.to_string(),
    ));
    let app = Router::new()
        .route("/_matrix/key/v2/server", get(handle_server_keys))
        .with_state(metrics.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("127.0.0.1:{}", addr.port()), metrics)
}

async fn setup_ingress_app(
    server_name: &str,
    key_fetch_max_concurrency: usize,
) -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.server.name = server_name.to_string();
    container.server_name = server_name.to_string();
    container.config.federation.enabled = true;
    container.config.federation.allow_ingress = true;
    container.config.federation.server_name = server_name.to_string();
    container.config.federation.key_fetch_max_concurrency = key_fetch_max_concurrency;
    container.config.federation.key_fetch_timeout_ms = 5000;
    container.config.federation.signing_key = None;
    container.config.federation.key_id = None;

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(synapse_rust::web::create_router(state))
}

fn signed_request(
    method: &str,
    uri: &str,
    origin: &str,
    destination: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    content: &Value,
) -> Request<Body> {
    let signed_bytes =
        canonical_federation_request_bytes(method, uri, origin, destination, Some(content));
    let sig = signing_key.sign(&signed_bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());

    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            "Authorization",
            format!(
                "X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"",
                origin, key_id, sig_b64
            ),
        )
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_join_related_requests_can_bypass_general_key_fetch_saturation() {
    let destination = "dest.test";
    let key_id = "ed25519:test";
    let signing_key_seed = [9u8; 32];
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);
    let verify_key_b64 = STANDARD_NO_PAD.encode(signing_key.verifying_key().as_bytes());

    let Some(app) = setup_ingress_app(destination, 2).await else {
        return;
    };
    let (origin, metrics) = start_key_server(200, key_id, &verify_key_b64).await;

    let txn_uri = "/_matrix/federation/v1/send/txn1";
    let txn_body = json!({ "origin": origin, "pdus": [] });
    let invite_uri = format!(
        "/_matrix/federation/v1/invite/!roomid:{}/$event",
        destination
    );
    let invite_body = json!({ "origin": origin });

    let txn_req = signed_request(
        "PUT",
        txn_uri,
        &origin,
        destination,
        key_id,
        &signing_key,
        &txn_body,
    );
    let invite_req = signed_request(
        "PUT",
        &invite_uri,
        &origin,
        destination,
        key_id,
        &signing_key,
        &invite_body,
    );

    let app_txn = app.clone();
    let app_invite = app.clone();
    let t1 = tokio::spawn(async move {
        app_txn
            .oneshot(super::with_local_connect_info(txn_req))
            .await
            .unwrap()
    });
    let t2 = tokio::spawn(async move {
        app_invite
            .oneshot(super::with_local_connect_info(invite_req))
            .await
            .unwrap()
    });

    let r1 = t1.await.unwrap();
    let r2 = t2.await.unwrap();

    assert_ne!(r1.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(r2.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(metrics.max_inflight.load(Ordering::SeqCst), 2);
}
