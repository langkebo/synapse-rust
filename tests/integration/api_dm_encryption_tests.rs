//! B-1：DM 端点必须默认加密。
//!
//! 历史缺陷：`/create_dm` 与 `/friends/dm/{user_id}` 只给房间 `preset: private_chat`，
//! **不写 `m.room.encryption`**。`private_chat` 只影响 join_rule 与 history_visibility，
//! 管不到消息加密 —— 于是任何直连 vendor 端点的客户端都能造出「看起来是私聊、实际是明文」的房间，
//! 服务端与联邦都能读内容。
//!
//! 这里走真实 HTTP 验证四件事：
//! 1. `/create_dm` 默认产出带 `m.room.encryption` 的房间；
//! 2. `allow_plaintext: true` 时才允许明文（逃生口可用且必须显式）；
//! 3. 群 DM（多被邀请人）那条分支也加密 —— 它走的是另一段代码；
//! 4. `/friends/dm/{user_id}` 同样加密。

use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::setup_fresh_test_app;

const MEGOLM: &str = "m.megolm.v1.aes-sha2";

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
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
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

/// 调 `/create_dm`，返回 `(status, room_id 或错误体)`。
async fn create_dm(app: &axum::Router, token: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/vendor/v1/create_dm")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 读取房间的 `m.room.encryption`；不存在时返回 `None`。
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
async fn create_dm_writes_megolm_encryption_by_default() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (token_a, _user_a) = register_user(&app, "dm_enc_creator").await;
    let (_token_b, user_b) = register_user(&app, "dm_enc_invitee").await;

    let (status, body) = create_dm(&app, &token_a, json!({ "user_id": user_b })).await;
    assert_eq!(status, StatusCode::OK, "create_dm failed: {body}");
    let room_id = body["room_id"].as_str().expect("room_id");

    let encryption = room_encryption(&app, &token_a, room_id).await.expect("m.room.encryption must exist");
    assert_eq!(encryption["algorithm"].as_str(), Some(MEGOLM), "DM must be created with Megolm, got {encryption}");
}

#[tokio::test]
async fn create_dm_allows_plaintext_only_when_explicitly_requested() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (token_a, _user_a) = register_user(&app, "dm_plain_creator").await;
    let (_token_b, user_b) = register_user(&app, "dm_plain_invitee").await;

    let (status, body) = create_dm(&app, &token_a, json!({ "user_id": user_b, "allow_plaintext": true })).await;
    assert_eq!(status, StatusCode::OK, "create_dm(allow_plaintext) failed: {body}");
    let room_id = body["room_id"].as_str().expect("room_id");

    assert!(
        room_encryption(&app, &token_a, room_id).await.is_none(),
        "allow_plaintext: true must create a room WITHOUT m.room.encryption"
    );
}

#[tokio::test]
async fn group_dm_branch_is_encrypted_too() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (token_a, _user_a) = register_user(&app, "gdm_creator").await;
    let (_t1, user_b) = register_user(&app, "gdm_invitee1").await;
    let (_t2, user_c) = register_user(&app, "gdm_invitee2").await;

    // 多个 invite → 走的是独立的 group-DM 分支（历史上最容易漏改的一段）
    let (status, body) = create_dm(&app, &token_a, json!({ "invite": [user_b, user_c] })).await;
    assert_eq!(status, StatusCode::OK, "group create_dm failed: {body}");
    let room_id = body["room_id"].as_str().expect("room_id");

    let encryption = room_encryption(&app, &token_a, room_id).await.expect("m.room.encryption must exist");
    assert_eq!(encryption["algorithm"].as_str(), Some(MEGOLM), "group DM must be encrypted too");
}

#[tokio::test]
async fn friend_dm_endpoint_writes_megolm_encryption() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (token_a, _user_a) = register_user(&app, "friend_dm_creator").await;
    let (_token_b, user_b) = register_user(&app, "friend_dm_peer").await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/friends/dm/{user_b}"))
        .header("Authorization", format!("Bearer {token_a}"))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "friends/dm failed: {body}");

    let room_id = body["room_id"].as_str().expect("room_id");
    let encryption = room_encryption(&app, &token_a, room_id).await.expect("m.room.encryption must exist");
    assert_eq!(encryption["algorithm"].as_str(), Some(MEGOLM));
}
