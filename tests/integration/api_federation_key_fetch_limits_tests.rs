use axum::{
    body::Body,
    extract::Path,
    http::{Request, StatusCode},
    routing::get,
    Json, Router,
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use synapse_common::canonical_json;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::ServiceContainer;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;

struct KeyServerMetrics {
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
    total_requests: AtomicUsize,
    delay_ms: u64,
    fail_status: Option<StatusCode>,
    /// The server name returned in key responses (matches the mock server's
    /// bind address so that `validate_server_key_response` accepts it).
    server_name: String,
    /// Ed25519 signing key used to self-sign key responses.
    signing_key: SigningKey,
    /// Base64 (no-pad) public key corresponding to `signing_key`.
    pub_key_b64: String,
}

impl KeyServerMetrics {
    fn new(delay_ms: u64, fail_status: Option<StatusCode>, server_name: String) -> Self {
        let mut rng = rand::rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let pub_key_b64 = STANDARD_NO_PAD.encode(signing_key.verifying_key().as_bytes());
        Self {
            inflight: AtomicUsize::new(0),
            max_inflight: AtomicUsize::new(0),
            total_requests: AtomicUsize::new(0),
            delay_ms,
            fail_status,
            server_name,
            signing_key,
            pub_key_b64,
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
            if self.max_inflight.compare_exchange(max, now, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                break;
            }
        }
    }

    fn on_end(&self) {
        self.inflight.fetch_sub(1, Ordering::SeqCst);
    }

    /// Build a self-signed server key response for the given key_id.
    fn signed_key_response(&self, key_id: &str) -> Value {
        let mut body = json!({
            "server_name": self.server_name,
            "verify_keys": {
                key_id: { "key": self.pub_key_b64 }
            },
            "old_verify_keys": {},
            "valid_until_ts": 9999999999999i64
        });
        let canonical = canonical_json(&body).unwrap();
        let sig = self.signing_key.sign(canonical.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("signatures".to_string(), json!({ self.server_name.clone(): { key_id: sig_b64 } }));
        }
        body
    }
}

async fn handle_server_keys(metrics: axum::extract::State<Arc<KeyServerMetrics>>) -> (StatusCode, Json<Value>) {
    metrics.on_start();
    tokio::time::sleep(std::time::Duration::from_millis(metrics.delay_ms)).await;
    metrics.on_end();

    if let Some(status) = metrics.fail_status {
        return (status, Json(json!({})));
    }

    (StatusCode::OK, Json(metrics.signed_key_response("ed25519:fixed")))
}

async fn handle_key_query(
    metrics: axum::extract::State<Arc<KeyServerMetrics>>,
    Path((_server_name, key_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    metrics.on_start();
    tokio::time::sleep(std::time::Duration::from_millis(metrics.delay_ms)).await;
    metrics.on_end();

    if let Some(status) = metrics.fail_status {
        return (status, Json(json!({})));
    }

    (StatusCode::OK, Json(metrics.signed_key_response(&key_id)))
}

async fn start_key_server(delay_ms: u64, fail_status: Option<StatusCode>) -> (String, Arc<KeyServerMetrics>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let origin = format!("127.0.0.1:{}", addr.port());
    let metrics = Arc::new(KeyServerMetrics::new(delay_ms, fail_status, origin.clone()));
    let app = Router::new()
        .route("/_matrix/key/v2/server", get(handle_server_keys))
        .route("/_matrix/key/v2/query/{server_name}/{key_id}", get(handle_key_query))
        .with_state(metrics.clone());

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (origin, metrics)
}

/// 发请求并返回 `(状态码, 响应体)`。
///
/// **2026-10-24（审计 TEST-02）**：本文件原先 4 处只断言状态码。这里补的是"状态码相同、
/// 响应体不同"的两类回归：
///   * 失败面：404 必须带 `M_NOT_FOUND`（客户端按 errcode 区分"对端没有这把 key"与
///     "本服务器坏了"；空 body 会让它误判）；
///   * 成功面：`/_matrix/key/v2/query/...` 必须回 `{ "server_keys": [...] }`，否则
///     规范化/签名校验的调用方拿到的是 `null` 而不是 key 集合。
async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024).await.unwrap_or_default();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

/// 断言"对端取 key 失败"的完整形状：404 + `M_NOT_FOUND` + 非空 error。
#[track_caller]
fn assert_key_fetch_not_found(response: (StatusCode, serde_json::Value), context: &str) {
    let (status, body) = response;
    assert_eq!(status, StatusCode::NOT_FOUND, "{context}：取 key 失败必须是 404；实际 body = {body}");
    assert_eq!(
        body["errcode"], "M_NOT_FOUND",
        "{context}：404 必须带 M_NOT_FOUND（调用方按 errcode 区分「对端没有这把 key」与「本服务器故障」）；\
         实际 body = {body}"
    );
    assert!(!body["error"].as_str().unwrap_or_default().is_empty(), "{context}：必须带可读 error；实际 body = {body}");
}

async fn setup_test_app_with_federation_key_fetch_config(
    key_fetch_timeout_ms: u64,
    key_fetch_max_concurrency: usize,
) -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool).await;
    super::config_mut(&mut container).federation.key_fetch_timeout_ms = key_fetch_timeout_ms;
    super::config_mut(&mut container).federation.key_fetch_max_concurrency = key_fetch_max_concurrency;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(synapse_web::create_router(state))
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
    assert_key_fetch_not_found(send(&app, request).await, "key_fetch_timeout_ms=1 且对端 50ms 才应答");
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
            let (status, body) = send(&app, request).await;
            assert_eq!(status, StatusCode::OK, "并发限流不该把请求变成失败；实际 body = {body}");
            assert!(
                body["server_keys"].as_array().is_some_and(|keys| !keys.is_empty()),
                "取 key 成功必须回 {{server_keys: [...]}}（空或 null 会让调用方以为对端没有 key）；\
                 实际 body = {body}"
            );
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
    let response = send(&app, request).await;
    assert_key_fetch_not_found(response, "对端 500 ⇒ 首次取 key 失败");
    let first_count = metrics.total_requests.load(Ordering::SeqCst);
    assert!(first_count > 0);

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/key/v2/query/{}/ed25519:backoff", origin))
        .body(Body::empty())
        .unwrap();
    assert_key_fetch_not_found(send(&app, request).await, "退避窗口内重试同一 key");

    let second_count = metrics.total_requests.load(Ordering::SeqCst);
    assert_eq!(second_count, first_count);
}
