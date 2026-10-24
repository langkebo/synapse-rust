#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第二波 2/3 —— `telemetry` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/telemetry_shape_only_tests.rs` 只断言自己构造的字面量，改坏
//! `routes/telemetry.rs` 也不会红。本文件打真 router（`/_synapse/admin/v1/telemetry/*`），
//! 覆盖四个只读端点 + 一条 RBAC：
//!
//! * `/status` —— 服务身份与开关（`service_name` / `enabled` / `sampling_ratio`）；
//! * `/health` —— 就绪状态 + `checks` + `database`（这条把「健康检查真的跑了」钉住）；
//! * `/alerts` —— 告警列表必须是数组（空数组也要是数组，不能是 null）；
//! * 非管理员 ⇒ 403 `M_FORBIDDEN`（这是 `AdminUser` 提取器的责任，但这条族是它的消费者，
//!   必须有一条线级用例证明「管理员门禁真的挂在这些路由上」）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

fn get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn register(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();
    let (status, value) = call(app, request).await;
    assert_eq!(status, StatusCode::OK, "注册必须是 200：{value}");
    value["access_token"].as_str().expect("注册必须返回 access_token").to_string()
}

/// `/status` 必须报告服务身份与三个开关；`sampling_ratio` 必须在 `[0, 1]` 内
/// （超出范围会让下游按比例采样算错）。
#[tokio::test]
async fn telemetry_status_reports_service_identity() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, value) = call(&app, get("/_synapse/admin/v1/telemetry/status", &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "{value}");

    assert!(
        value["service_name"].as_str().is_some_and(|name| !name.is_empty()),
        "status 必须带非空 service_name：{value}"
    );
    for field in ["enabled", "trace_enabled", "metrics_enabled"] {
        assert!(value[field].is_boolean(), "{field} 必须是布尔值：{value}");
    }
    let ratio = value["sampling_ratio"].as_f64().unwrap_or_else(|| panic!("sampling_ratio 必须是数字：{value}"));
    assert!((0.0..=1.0).contains(&ratio), "sampling_ratio 必须在 [0,1] 内，实际 {ratio}：{value}");
}

/// `/health` 是这条族里唯一「真的去探依赖」的端点：必须带 `status` 与 `checks`，
/// 且 `database` 字段存在（哪怕是空对象）。
#[tokio::test]
async fn telemetry_health_reports_checks() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, value) = call(&app, get("/_synapse/admin/v1/telemetry/health", &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "{value}");

    assert!(
        value["status"].as_str().is_some_and(|s| !s.is_empty()),
        "health 必须带非空 status（healthy / degraded / unhealthy）：{value}"
    );
    assert!(value.get("checks").is_some(), "health 必须带 checks：{value}");
    assert!(value.get("database").is_some(), "health 必须带 database：{value}");
    assert!(value.get("service").is_some(), "health 必须带 service：{value}");
}

/// `/alerts` 的空态必须是 `[]` 而不是 `null` —— 客户端按数组解，`null` 会让列表页崩。
#[tokio::test]
async fn telemetry_alerts_empty_state_is_an_array() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, value) = call(&app, get("/_synapse/admin/v1/telemetry/alerts", &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert!(value["alerts"].is_array(), "alerts 必须是数组（空态也要是 []，不能是 null）：{value}");
}

/// 非管理员 ⇒ 403（证明管理员门禁真的挂在这一族路由上，而不是只在 handler 里「以为有」）。
#[tokio::test]
async fn telemetry_requires_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "telemetry_plain_user").await;

    let (status, value) = call(&app, get("/_synapse/admin/v1/telemetry/status", &token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通用户必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}
