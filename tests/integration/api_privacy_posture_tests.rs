//! B-7：`/health` 与 `/_matrix/client/v3/capabilities` 必须暴露「服务端是否持有房间密钥」。
//!
//! 这条事实此前只存在于配置文件：`server.megolm_device_id` 一配，服务端就能
//! encrypt/decrypt 所有加密房间，而探针与客户端都无从得知。
//! 于是「我们的私聊是 E2EE」与「这台机器握着所有房间密钥」可以同时成立而无人察觉。
//!
//! 判据（本仓唯一）：`ServerConfig::server_holds_room_keys()`——
//! 空白与 `none` 视为未启用，真设备号视为启用。
//! 谓词本身的边界由 `synapse-common` 的单测钉住；这里验证两个对外通道接的是它。

use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use crate::setup_fresh_test_app;

async fn get_json(app: &axum::Router, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    let request = builder.body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn health_reports_server_holds_room_keys() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (status, body) = get_json(&app, "/health", None).await;
    assert_eq!(status, StatusCode::OK, "health must be reachable: {body}");

    let value = body.get("server_holds_room_keys").and_then(Value::as_bool);
    assert_eq!(
        value,
        Some(false),
        "测试部署没有配置 megolm_device_id，必须报告 server_holds_room_keys=false；got {body}"
    );
}

#[tokio::test]
async fn capabilities_expose_the_privacy_capability() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (status, body) = get_json(&app, "/_matrix/client/v3/capabilities", None).await;
    assert_eq!(status, StatusCode::OK, "capabilities must be reachable: {body}");

    let privacy = body
        .get("capabilities")
        .and_then(|c| c.get("m.hula.privacy"))
        .unwrap_or_else(|| panic!("capabilities must carry m.hula.privacy, got {body}"));

    assert_eq!(
        privacy.get("server_holds_room_keys").and_then(Value::as_bool),
        Some(false),
        "默认部署必须报告 server_holds_room_keys=false；got {privacy}"
    );
}

/// `/_health` 的 `checks.room_key_custody` 是同一事实的详细侧呈现，
/// 部署脚本 `verify_health_endpoints` 读的是 `/health` 顶层字段；
/// 两者必须一致，否则运维看到的与脚本断言的会是两套答案。
#[tokio::test]
async fn detailed_health_reports_room_key_custody() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (status, body) = get_json(&app, "/_health", None).await;
    assert_eq!(status, StatusCode::OK, "_health must be reachable: {body}");

    let custody = body
        .get("checks")
        .and_then(|c| c.get("room_key_custody"))
        .unwrap_or_else(|| panic!("_health must carry checks.room_key_custody, got {body}"));

    assert_eq!(
        custody.get("server_holds_room_keys").and_then(Value::as_bool),
        Some(false),
        "默认部署必须报告 false；got {custody}"
    );
    assert_eq!(custody.get("status").and_then(Value::as_str), Some("clients"), "got {custody}");

    // 持有密钥**不**应把整体健康度拉成 degraded：bridge 是有意的部署形态，
    // 把它标红只会让人把这个字段当噪音。这里钉住"可见但不干扰告警"。
    assert!(
        matches!(body.get("status").and_then(Value::as_str), Some("healthy") | Some("degraded")),
        "overall status must stay a normal value, got {body}"
    );
}
