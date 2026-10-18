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
