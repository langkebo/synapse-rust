//! B-6：Megolm 轮换参数。
//!
//! Megolm **没有 break-in recovery**：叶棘轮一旦泄露，泄露前的内容在旧 session 里永久可解。
//! 缩短 session 生命周期是唯一可用的缓解，所以私密房间不能把轮换交给客户端默认值
//! （规范默认 7 天 / 100 条），必须显式写进 `m.room.encryption`：
//! **1 天 / 50 条**。
//!
//! 同时验证「可调」这条要求：房间管理员可以通过标准的
//! `PUT /rooms/{id}/state/m.room.encryption` 调整（该事件 PL 要求 100），而普通成员不能。

use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::setup_fresh_test_app;

const MEGOLM: &str = "m.megolm.v1.aes-sha2";
const DAY_MS: i64 = 86_400_000;

struct User {
    token: String,
    user_id: String,
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
                "password": "Password123!",
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

async fn create_dm(app: &axum::Router, from: &User, to: &User) -> String {
    let (status, body) =
        call(app, "POST", "/_matrix/vendor/v1/create_dm", &from.token, Some(json!({ "user_id": to.user_id }))).await;
    assert_eq!(status, StatusCode::OK, "create_dm failed: {body}");
    body["room_id"].as_str().expect("room_id").to_string()
}

async fn room_encryption(app: &axum::Router, token: &str, room_id: &str) -> Option<Value> {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.encryption"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    if response.status() != StatusCode::OK {
        return None;
    }
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).ok()
}

#[tokio::test]
async fn dm_writes_short_megolm_rotation() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "rot_dm_a").await;
    let bob = register(&app, "rot_dm_b").await;

    let room_id = create_dm(&app, &alice, &bob).await;
    let enc = room_encryption(&app, &alice.token, &room_id).await.expect("m.room.encryption must exist");

    assert_eq!(enc["algorithm"].as_str(), Some(MEGOLM));
    assert_eq!(
        enc["rotation_period_ms"].as_i64(),
        Some(DAY_MS),
        "DM must rotate its Megolm session every day, got {enc}"
    );
    assert_eq!(enc["rotation_period_msgs"].as_i64(), Some(50), "DM must rotate every 50 messages, got {enc}");
}

#[tokio::test]
async fn friend_dm_writes_short_megolm_rotation() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "rot_fr_a").await;
    let bob = register(&app, "rot_fr_b").await;

    let (status, body) =
        call(&app, "POST", &format!("/_matrix/vendor/v1/friends/dm/{}", bob.user_id), &alice.token, Some(json!({})))
            .await;
    assert_eq!(status, StatusCode::OK, "friends/dm failed: {body}");
    let room_id = body["room_id"].as_str().expect("room_id").to_string();

    let enc = room_encryption(&app, &alice.token, &room_id).await.expect("m.room.encryption must exist");
    assert_eq!(enc["rotation_period_ms"].as_i64(), Some(DAY_MS), "got {enc}");
    assert_eq!(enc["rotation_period_msgs"].as_i64(), Some(50), "got {enc}");
}

/// 可调：房间管理员能通过标准状态事件调整轮换参数（该事件 PL=100）。
#[tokio::test]
async fn room_admin_can_tune_rotation() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "rot_tune_a").await;
    let bob = register(&app, "rot_tune_b").await;

    let room_id = create_dm(&app, &alice, &bob).await;

    let (status, body) = call(
        &app,
        "PUT",
        &format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.encryption"),
        &alice.token,
        Some(json!({
            "algorithm": MEGOLM,
            "rotation_period_ms": 3_600_000,
            "rotation_period_msgs": 20
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin must be able to tune rotation: {body}");

    let enc = room_encryption(&app, &alice.token, &room_id).await.expect("state must exist");
    assert_eq!(enc["rotation_period_ms"].as_i64(), Some(3_600_000), "got {enc}");
    assert_eq!(enc["rotation_period_msgs"].as_i64(), Some(20), "got {enc}");
}

/// 非管理员不能改 m.room.encryption（PL 100）。
#[tokio::test]
async fn plain_member_cannot_tune_rotation() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "rot_deny_a").await;
    let bob = register(&app, "rot_deny_b").await;

    let room_id = create_dm(&app, &alice, &bob).await;

    // bob 是被邀请人，先加入房间成为普通成员（PL 0）
    let (status, body) =
        call(&app, "POST", &format!("/_matrix/client/v3/rooms/{room_id}/join"), &bob.token, Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "join failed: {body}");

    let (status, body) = call(
        &app,
        "PUT",
        &format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.encryption"),
        &bob.token,
        Some(json!({ "algorithm": MEGOLM, "rotation_period_ms": 1_000, "rotation_period_msgs": 1 })),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a PL-0 member must not be able to rewrite the encryption config, got {status} {body}"
    );

    // 原值必须不变
    let enc = room_encryption(&app, &alice.token, &room_id).await.expect("state must exist");
    assert_eq!(enc["rotation_period_ms"].as_i64(), Some(DAY_MS), "rotation must remain unchanged: {enc}");
}
