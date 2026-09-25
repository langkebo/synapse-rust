//! 设备验证只走规范 to-device 中继的可执行证据。
//!
//! **这个测试为什么存在**：本仓曾把设备验证实现成"服务端托管设备私钥"的两套自定义端点
//! （`/keys/device_signing/verify_*`、`/device_verification/*`）。规范里 `m.key.verification.*`
//! 是**客户端之间**的 to-device 流程，homeserver 只负责投递 —— 拆除那些端点之后，
//! "客户端还能不能完成验证"完全取决于中继通路是否够用。
//!
//! 光删掉违规代码只能证明"不再做错事"，不能证明"对的事还能做"。因此这里锁定三件事：
//!   1. `PUT /sendToDevice/{eventType}/{txnId}` 把 `m.key.verification.*` **原样**中继；
//!   2. 目标设备能从 `/sync` 的 `to_device.events` 里把它取走；
//!   3. 那两组服务端 SAS 端点已不存在（404），不会再有任何服务端密钥参与验证。
//!
//! **判据（红）**：第 3 条在删除前必然失败（端点当时返回 200/4xx 而非 404），
//! 而第 1、2 条在删除前后都必须通过 —— 若中继通路本身不成立，本重构的前提就被推翻。

use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{get_admin_token, setup_fresh_test_app};

/// 注册一个用户，返回 `(access_token, user_id, device_id)`。
async fn register_user(app: &axum::Router, prefix: &str) -> (String, String, String) {
    let username = format!("{prefix}_{}", rand::random::<u32>());
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

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "register must succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().expect("access_token").to_string(),
        json["user_id"].as_str().expect("user_id").to_string(),
        json["device_id"].as_str().expect("device_id").to_string(),
    )
}

/// `m.key.verification.*` to-device 事件必须被原样中继、并可从 `/sync` 取走。
#[tokio::test]
async fn verification_to_device_events_are_relayed_verbatim() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (_admin_token, _) = get_admin_token(&app).await;

    let (sender_token, sender_user_id, sender_device_id) = register_user(&app, "vfy_sender").await;
    let (receiver_token, receiver_user_id, receiver_device_id) = register_user(&app, "vfy_receiver").await;

    // 规范 SAS 的起始事件，内容取真实形状（method + 双方设备 + 事务号）。
    // `from_device` 必须是发起方**真实**设备 ID（规范要求，且服务端有权据此校验）。
    let content = json!({
        "from_device": sender_device_id,
        "method": "m.sas.v1",
        "transaction_id": "relay-tx-1",
        "key_agreement_protocols": ["curve25519-hkdf-sha256"],
        "hashes": ["sha256"],
        "message_authentication_codes": ["hkdf-hmac-sha256.v2"],
        "short_authentication_string": ["decimal", "emoji"]
    });

    // `messages` 是规范要求的**两层**结构 `{user_id: {device_id: content}}`（不是
    // `{device_id: content}` —— 少一层会让服务端把 content 的字段数当成收件人数），
    // 两层的键都是运行时才知道的，因此用 Map 构造。
    let messages = Value::Object(serde_json::Map::from_iter([(
        receiver_user_id.clone(),
        Value::Object(serde_json::Map::from_iter([(receiver_device_id.clone(), content.clone())])),
    )]));

    let send_request = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/v3/sendToDevice/m.key.verification.start/relay_txn_1")
        .header("Authorization", format!("Bearer {sender_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "messages": messages }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(send_request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "sendToDevice must accept m.key.verification.start，实得 {status}，响应体：{}",
        String::from_utf8_lossy(&body)
    );

    // 接收方从 /sync 取走该事件：type 与 content 必须原样（服务端不得改写或丢弃）。
    let sync_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync?timeout=0")
        .header("Authorization", format!("Bearer {receiver_token}"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(sync_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "sync must succeed");

    let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    let sync_json: Value = serde_json::from_slice(&body).unwrap();

    let events = sync_json["to_device"]["events"].as_array().expect("to_device.events must be an array");
    let relayed = events
        .iter()
        .find(|event| event["type"] == "m.key.verification.start")
        .expect("m.key.verification.start must be delivered to the target device");

    assert_eq!(relayed["sender"], sender_user_id, "sender must be preserved");
    assert_eq!(relayed["content"], content, "content must be relayed verbatim");
}

/// 拆除后：两组服务端 SAS 端点必须不存在。
///
/// 这是本重构的核心断言 —— 只要任一端点还活着，服务端就仍在参与验证判定。
#[tokio::test]
async fn server_side_sas_endpoints_are_gone() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };
    let (_admin_token, _) = get_admin_token(&app).await;

    let (token, _user_id, _device_id) = register_user(&app, "vfy_gone").await;

    // (method, uri) —— 覆盖两条违规面的代表端点。
    let removed_endpoints: [(&str, &str); 8] = [
        ("POST", "/_matrix/client/v3/keys/device_signing/verify_start"),
        ("PUT", "/_matrix/client/v3/keys/device_signing/verify_accept"),
        ("POST", "/_matrix/client/v3/keys/device_signing/verify_key_agreement"),
        ("POST", "/_matrix/client/v3/keys/device_signing/verify_mac"),
        ("POST", "/_matrix/client/v3/keys/device_signing/verify_done"),
        ("GET", "/_matrix/client/v3/keys/device_signing/requests"),
        ("POST", "/_matrix/client/v3/keys/qr_code/show"),
        ("POST", "/_matrix/client/v3/device_verification/request"),
    ];

    for (method, uri) in removed_endpoints {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{method} {uri} 必须已随去服务端私钥重构移除（应 404），实得 {status}，响应体：{}",
            String::from_utf8_lossy(&body)
        );
    }

    // device_trust 面的其余端点。
    let removed_gets: [&str; 3] = [
        "/_matrix/client/v3/device_trust",
        "/_matrix/client/v3/device_trust/DEV",
        "/_matrix/client/v3/security/summary",
    ];
    for uri in removed_gets {
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "GET {uri} 必须已随去服务端私钥重构移除（应 404），实得 {status}，响应体：{}",
            String::from_utf8_lossy(&body)
        );
    }
}
