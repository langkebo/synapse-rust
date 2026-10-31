#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第二波 1/3 —— `friend` 端点族的**真 HTTP 往返**。
//!
//! 这是目前 21 个缺口里链路最长的一族：`tests/unit/friend_shape_only_tests.rs` 只测了
//! 本地 `is_valid_user_id` 与几个枚举，改坏生产 handler 也不会红。本文件跑完整状态机：
//!
//! `POST /friends/request` → 被请求方 `GET /friends/requests/incoming`
//! → `POST /friends/request/{user}/accept` → 双方 `GET /friends/check/{user}` 与 `GET /friends`
//!
//! 另加三条边界：给自己发请求 400、非法 user_id 400、未认证 401。
//!
//! 私有端点唯一规范位置是 `/_matrix/vendor/v1`（ISSUE-13：client 别名已删除）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

fn bearer(builder: axum::http::request::Builder, token: Option<&str>) -> axum::http::request::Builder {
    match token {
        Some(token) => builder.header("Authorization", format!("Bearer {token}")),
        None => builder,
    }
}

fn get(uri: &str, token: Option<&str>) -> Request<Body> {
    bearer(Request::builder().method("GET").uri(uri), token).body(Body::empty()).unwrap()
}

fn post_json(uri: &str, token: Option<&str>, body: &Value) -> Request<Body> {
    let builder = Request::builder().method("POST").uri(uri).header("Content-Type", "application/json");
    bearer(builder, token).body(Body::from(body.to_string())).unwrap()
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn register(app: &axum::Router, username: &str) -> (String, String) {
    let (status, value) = call(
        app,
        post_json(
            "/_matrix/client/v3/register",
            None,
            &json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "注册必须是 200：{value}");
    let token = value["access_token"].as_str().expect("注册必须返回 access_token").to_string();
    let user_id = value["user_id"].as_str().expect("注册必须返回 user_id").to_string();
    (token, user_id)
}

fn incoming_has_user(incoming: &Value, user_id: &str) -> bool {
    incoming["requests"]
        .as_array()
        .unwrap_or_else(|| panic!("incoming 必须形如 {{requests: [...]}}：{incoming}"))
        .iter()
        .any(|request| {
            request["user_id"] == user_id
                || request["from_user_id"] == user_id
                || request["sender"] == user_id
                || request["requester_id"] == user_id
        })
}

/// 主干：请求 → 收到 → 接受 → 双向成为好友。
#[tokio::test]
async fn friend_request_flow_creates_friendship() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (alice_token, alice_id) = register(&app, "friend_alice").await;
    let (bob_token, bob_id) = register(&app, "friend_bob").await;

    // ① 发出请求
    let (status, sent) =
        call(&app, post_json("/_matrix/vendor/v1/friends/request", Some(&alice_token), &json!({ "user_id": bob_id })))
            .await;
    assert_eq!(status, StatusCode::OK, "发好友请求必须是 200：{sent}");
    assert_eq!(sent["status"], "pending", "刚发出的请求状态必须是 pending：{sent}");
    assert!(sent["request_id"].is_string(), "必须返回 request_id：{sent}");

    // ② 被请求方能看到它
    let (status, incoming) = call(&app, get("/_matrix/vendor/v1/friends/requests/incoming", Some(&bob_token))).await;
    assert_eq!(status, StatusCode::OK, "{incoming}");
    assert!(incoming_has_user(&incoming, &alice_id), "Bob 的 incoming 里必须出现 Alice：{incoming}");

    // ③ 接受（后端会建一条 DM 房间，返回 room_id）
    let accept_uri = format!("/_matrix/vendor/v1/friends/request/{alice_id}/accept");
    let (status, accepted) = call(&app, post_json(&accept_uri, Some(&bob_token), &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "接受好友请求必须是 200：{accepted}");
    assert_eq!(accepted["status"], "accepted", "{accepted}");
    assert!(
        accepted["room_id"].as_str().is_some_and(|room| room.starts_with('!')),
        "接受后必须返回 DM 的 room_id：{accepted}"
    );

    // ④ 双向都是好友
    for (token, target, who) in
        [(&alice_token, bob_id.as_str(), "Alice→Bob"), (&bob_token, alice_id.as_str(), "Bob→Alice")]
    {
        let uri = format!("/_matrix/vendor/v1/friends/check/{target}");
        let (status, check) = call(&app, get(&uri, Some(token))).await;
        assert_eq!(status, StatusCode::OK, "{who} check 必须 200：{check}");
        assert_eq!(check["is_friend"], true, "{who} 接受后必须互为好友：{check}");
        assert_eq!(check["are_friends"], true, "两个字段必须一致（客户端两种都在读）：{check}");
    }

    // ⑤ 列表里能看到对方
    let (status, list) = call(&app, get("/_matrix/vendor/v1/friends", Some(&alice_token))).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let friends = list["friends"].as_array().unwrap_or_else(|| panic!("必须是 {{friends: [...]}}：{list}"));
    assert_eq!(friends.len(), 1, "接受后 Alice 应当恰有 1 个好友：{list}");
    assert_eq!(list["total"], 1, "total 必须与实际条数一致：{list}");
}

/// 给自己发请求 ⇒ 400（否则会自建 DM 房间）。
#[tokio::test]
async fn friend_request_to_self_is_rejected() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (token, user_id) = register(&app, "friend_self").await;

    let (status, value) =
        call(&app, post_json("/_matrix/vendor/v1/friends/request", Some(&token), &json!({ "user_id": user_id }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "给自己发请求必须 400：{value}");
    assert_eq!(value["errcode"], "M_BAD_JSON", "errcode：{value}");
}

/// 非法 user_id ⇒ 400（`validate_user_id` 拦在服务层之前）。
#[tokio::test]
async fn friend_request_with_invalid_user_id_is_rejected() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (token, _) = register(&app, "friend_badid").await;

    let (status, value) =
        call(&app, post_json("/_matrix/vendor/v1/friends/request", Some(&token), &json!({ "user_id": "notauser" })))
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法 user_id 必须 400：{value}");
    assert!(value["errcode"].as_str().is_some_and(|code| code.starts_with("M_")), "必须带 Matrix errcode：{value}");
}

/// 未认证访问 ⇒ 401（不能返回空好友列表假装成功）。
#[tokio::test]
async fn friend_endpoints_require_authentication() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) = call(&app, get("/_matrix/vendor/v1/friends", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未认证必须 401：{value}");
    assert_eq!(value["errcode"], "M_UNAUTHORIZED", "本仓 auth 中间件的 errcode：{value}");
}

/// I-14：目标用户不存在 ⇒ 404 `M_NOT_FOUND`（曾因 `ApiError` 经 `?` 被降级成
/// `FriendRoomError::Internal` 而返回 500 `M_UNKNOWN`）。
#[tokio::test]
async fn friend_request_to_unknown_user_is_not_found() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (token, _) = register(&app, "friend_ghost_sender").await;

    let (status, value) = call(
        &app,
        post_json(
            "/_matrix/vendor/v1/friends/request",
            Some(&token),
            &json!({ "user_id": "@friend_no_such_user:example.com", "message": "probe" }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "目标用户不存在必须是 404：{value}");
    assert_eq!(value["errcode"], "M_NOT_FOUND", "errcode 必须是 M_NOT_FOUND：{value}");
}
