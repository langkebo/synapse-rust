use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::ServiceContainer;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;

/// 发请求并返回 `(状态码, 响应体)`。
///
/// **2026-10-24（审计 TEST-02）**：本文件原先只断言状态码，于是"401 但 errcode 不是
/// `M_UNAUTHORIZED`"、"200 但心跳响应不再是 `{status:"ok"}`"、"404 但返回的是空 body
/// 而不是 Matrix 错误信封"这三类回归全都看不见 —— 而客户端/worker 都是按 errcode 与
/// 响应字段分支的。
async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024).await.unwrap_or_default();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 断言"被拒"的完整形状：指定状态码 + 指定 errcode + 非空 error。
#[track_caller]
fn assert_error(response: (StatusCode, Value), expected: StatusCode, errcode: &str, context: &str) {
    let (status, body) = response;
    assert_eq!(status, expected, "{context}：状态码不对；实际 body = {body}");
    assert_eq!(body["errcode"], errcode, "{context}：errcode 必须是 {errcode}（客户端按它分支）；实际 body = {body}");
    let message = body["error"].as_str().unwrap_or_default();
    assert!(!message.is_empty(), "{context}：必须带可读 error；实际 body = {body}");
}

fn heartbeat_request(worker_id: &str, secret: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/worker/v1/workers/{worker_id}/heartbeat"))
        .header("Content-Type", "application/json");
    if let Some(secret) = secret {
        builder = builder.header("x-synapse-worker-secret", secret);
    }
    builder.body(Body::from(json!({ "status": "running", "load_stats": null }).to_string())).unwrap()
}

async fn setup_test_app_with_replication_secret() -> Option<(axum::Router, String, String)> {
    let pool = super::require_test_pool().await;
    let mut container = ServiceContainer::new_test_with_pool(pool).await;
    super::config_mut(&mut container).worker.enabled = true;
    super::config_mut(&mut container).worker.replication.http.enabled = true;
    super::config_mut(&mut container).worker.replication.http.secret = Some("test_worker_secret".to_string());
    super::config_mut(&mut container).worker.replication.http.secret_path = None;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    let app = super::production_app(state);

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

    let (status, body) = send(app, request).await;
    assert_eq!(status, StatusCode::CREATED, "worker 注册必须先成功，否则后面的守卫无从触发；body = {body}");
    // 注册响应也断言一次：它由 `created_json_from` 生成，形状一变（比如少了 worker_id），
    // 依赖该字段的编排脚本会静默失效。
    assert_eq!(body["worker_id"], worker_id, "注册响应必须回显 worker_id；实际 body = {body}");
}

#[tokio::test]
async fn test_worker_endpoints_require_replication_secret_when_enabled() {
    let Some((app, _admin_token, worker_id)) = setup_test_app_with_replication_secret().await else {
        return;
    };

    // ① 完全不带凭据
    assert_error(
        send(&app, heartbeat_request(&worker_id, None)).await,
        StatusCode::UNAUTHORIZED,
        "M_UNAUTHORIZED",
        "心跳不带 x-synapse-worker-secret",
    );

    // ② 带错凭据（这条比①更重要：它证明中间件真的在比较，而不是"有 header 就放行"）
    assert_error(
        send(&app, heartbeat_request(&worker_id, Some("wrong_secret"))).await,
        StatusCode::UNAUTHORIZED,
        "M_UNAUTHORIZED",
        "心跳带错误 secret",
    );

    // ③ 正确凭据 ⇒ 200 且响应体是 `{status:"ok"}`
    let (status, body) = send(&app, heartbeat_request(&worker_id, Some("test_worker_secret"))).await;
    assert_eq!(status, StatusCode::OK, "正确 secret 必须被接受；实际 body = {body}");
    assert_eq!(
        body["status"], "ok",
        "心跳成功响应必须是 {{\"status\":\"ok\"}}（worker 侧据此判断心跳被接受）；实际 body = {body}"
    );
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
    let app = super::production_app(state);

    let (admin_token, _admin_user) = super::get_admin_token(&app).await;
    let worker_id = format!("worker-{}", rand::random::<u32>());
    register_worker(&app, &admin_token, &worker_id).await;

    let (status, body) = send(&app, heartbeat_request(&worker_id, None)).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the worker body surface must not be mounted (and must never be a credential-free pass-through) \
         when worker.replication.http.enabled is false; body = {body}"
    );

    // 404 还不够：它必须是**Matrix 错误信封**，而不是空 body 或 HTML 错误页。
    // 客户端/worker 拿 404 时按 errcode 判断"这条路不存在"，空 body 会让它把
    // "服务端没这条路由"误读成"服务端坏了"。
    let errcode = body["errcode"].as_str().unwrap_or_default();
    assert!(
        errcode.starts_with("M_"),
        "404 必须带 Matrix errcode（M_NOT_FOUND / M_UNRECOGNIZED 皆可，但不能是空 body）；实际 body = {body}"
    );
    assert!(!body["error"].as_str().unwrap_or_default().is_empty(), "404 必须带可读 error；实际 body = {body}");
}

#[tokio::test]
async fn test_admin_worker_endpoints_still_require_admin_jwt() {
    let Some((app, admin_token, worker_id)) = setup_test_app_with_replication_secret().await else {
        return;
    };

    // 只有共享 secret、没有 admin JWT ⇒ 401（这条守住"worker 凭据 ≠ 管理员凭据"）
    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/worker/v1/workers")
        .header("x-synapse-worker-secret", "test_worker_secret")
        .body(Body::empty())
        .unwrap();
    assert_error(
        send(&app, request).await,
        StatusCode::UNAUTHORIZED,
        "M_UNAUTHORIZED",
        "worker 列表只带共享 secret、不带 admin JWT",
    );

    // 带 admin JWT ⇒ 200 且列表里真的有刚注册的那台 worker
    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/worker/v1/workers")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK, "admin JWT 必须能读到 worker 列表；实际 body = {body}");
    let workers = body.as_array().unwrap_or_else(|| panic!("worker 列表必须是 JSON 数组；实际 body = {body}"));
    assert!(
        workers.iter().any(|worker| worker["worker_id"] == worker_id.as_str()),
        "刚注册的 worker `{worker_id}` 必须出现在列表里（空数组也满足 status 200，所以必须查内容）；\
         实际 body = {body}"
    );
}
