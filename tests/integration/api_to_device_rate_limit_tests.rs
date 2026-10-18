//! B-8：to-device 反滥用限流。
//!
//! 历史缺陷：`sendToDevice` 只校验「用户存在 + 设备存在」，**没有任何速率约束**，
//! 于是任意登录用户都能对任意用户任意设备无限投递 —— 可以用来探测在线状态、
//! 灌垃圾、或放大骚扰，而这是端到端加密的必经通道，攻击面一直开着。
//!
//! 设计取舍：**只限流、不封通道**。to-device 承载 SAS 验证与房间密钥分发，
//! 硬拒绝会直接打断正常加密流程，所以只按关系分两档降速：
//! - 与接收方共享房间：每秒 2、突发 40（正常协商只有个位数消息）
//! - 无共享房间：每秒 1、突发 5
//! - 另有每发送方总桶（每秒 5、突发 100），挡住「换 recipient 扇出」

use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::setup_fresh_test_app;

const PASSWORD: &str = "Password123!";

struct User {
    token: String,
    user_id: String,
    device_id: String,
}

async fn register(app: &axum::Router, username: &str) -> User {
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": format!("{username}_{suffix}"),
                "password": PASSWORD,
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    User {
        token: json["access_token"].as_str().unwrap().to_string(),
        user_id: json["user_id"].as_str().unwrap().to_string(),
        device_id: json["device_id"].as_str().unwrap().to_string(),
    }
}

async fn call(app: &axum::Router, method: &str, uri: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 给 `to` 发一条 to-device（用递增的 transaction_id，避免幂等去重吃掉请求）。
async fn send_to_device(app: &axum::Router, from: &User, to: &User, seq: u64) -> StatusCode {
    let body = json!({
        "messages": {
            to.user_id.clone(): {
                to.device_id.clone(): { "type": "m.test", "seq": seq }
            }
        }
    });
    let (status, _) = call(
        app,
        "PUT",
        &format!("/_matrix/client/v3/sendToDevice/m.test/{}-{seq}", from.user_id),
        &from.token,
        Some(body),
    )
    .await;
    status
}

#[tokio::test]
async fn to_device_to_a_user_without_shared_rooms_is_rate_limited() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "td_rl_a").await;
    let bob = register(&app, "td_rl_b").await;

    // 无共享房间 ⇒ 低频档（突发 5）
    let mut ok = 0;
    let mut limited = 0;
    for seq in 0..10u64 {
        match send_to_device(&app, &alice, &bob, seq).await {
            StatusCode::OK => ok += 1,
            StatusCode::TOO_MANY_REQUESTS => limited += 1,
            other => panic!("unexpected status {other} at seq {seq}"),
        }
    }

    assert_eq!(ok, 5, "the untrusted tier must allow exactly the burst of 5 before throttling");
    assert_eq!(limited, 5, "further sends must be rejected with 429");
}

#[tokio::test]
async fn to_device_within_a_shared_room_keeps_a_higher_budget() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "td_room_a").await;
    let bob = register(&app, "td_room_b").await;

    // 建一个共享房间：alice 建房并邀请，bob 加入
    let (status, body) = call(
        &app,
        "POST",
        "/_matrix/client/v3/createRoom",
        &alice.token,
        Some(json!({ "preset": "private_chat", "invite": [bob.user_id] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "createRoom failed: {body}");
    let room_id = body["room_id"].as_str().expect("room_id").to_string();

    let (status, body) =
        call(&app, "POST", &format!("/_matrix/client/v3/rooms/{room_id}/join"), &bob.token, Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "join failed: {body}");

    // 共享房间 ⇒ 高档位（突发 40）：15 条必须全部通过
    for seq in 0..15u64 {
        let status = send_to_device(&app, &alice, &bob, seq).await;
        assert_eq!(status, StatusCode::OK, "shared-room to-device must not be throttled at seq {seq}");
    }
}
