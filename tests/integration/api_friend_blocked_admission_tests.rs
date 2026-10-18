//! B-3：`blocked` 必须真的拦人。
//!
//! 历史缺陷：`update_friend_status` 只把 `blocked` 写进 `m.friends.list` 的
//! `friends[].status`，**没有任何调用方读它做判断**。于是「拉黑」只是个标签：
//! 被拉黑方照样能把拉黑者拉进房间、继续发好友请求、继续建 DM。
//!
//! 语义刻意做成**单向**（与 MSC3873 ignore 一致）：看「被作用方是否拉黑了发起方」。
//! 拉黑方自己仍可发起动作 —— 否则拉黑就变成不可逆的了。
//!
//! 三条路径各一条用例：邀请 / 好友请求 / 建 DM。

use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::setup_fresh_test_app;

const PASSWORD: &str = "Password123!";

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

/// 让 `a` 与 `b` 成为好友（a 发请求、b 接受）。
async fn befriend(app: &axum::Router, a: &User, b: &User) {
    let (status, body) =
        call(app, "POST", "/_matrix/vendor/v1/friends", &a.token, Some(json!({ "user_id": b.user_id }))).await;
    assert_eq!(status, StatusCode::OK, "send friend request failed: {body}");

    let (status, body) =
        call(app, "POST", &format!("/_matrix/vendor/v1/friends/request/{}/accept", a.user_id), &b.token, None).await;
    assert_eq!(status, StatusCode::OK, "accept friend request failed: {body}");
}

/// `owner` 把 `other` 标记为 blocked。
async fn block(app: &axum::Router, owner: &User, other: &User) {
    let (status, body) = call(
        app,
        "PUT",
        &format!("/_matrix/vendor/v1/friends/{}/status", other.user_id),
        &owner.token,
        Some(json!({ "status": "blocked" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "block failed: {body}");
}

#[tokio::test]
async fn blocked_user_cannot_invite_the_blocker() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "blk_invite_a").await;
    let bob = register(&app, "blk_invite_b").await;
    befriend(&app, &alice, &bob).await;
    block(&app, &alice, &bob).await;

    // Bob 建房，然后邀请 Alice —— 必须被拒
    let (status, body) =
        call(&app, "POST", "/_matrix/client/v3/createRoom", &bob.token, Some(json!({ "preset": "private_chat" })))
            .await;
    assert_eq!(status, StatusCode::OK, "createRoom failed: {body}");
    let room_id = body["room_id"].as_str().expect("room_id").to_string();

    let (status, body) = call(
        &app,
        "POST",
        &format!("/_matrix/client/v3/rooms/{room_id}/invite"),
        &bob.token,
        Some(json!({ "user_id": alice.user_id })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "invite must be rejected once the invitee blocked the inviter (B-3), got {status} {body}"
    );
}

#[tokio::test]
async fn blocked_user_cannot_create_dm_with_the_blocker() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "blk_dm_a").await;
    let bob = register(&app, "blk_dm_b").await;
    befriend(&app, &alice, &bob).await;
    block(&app, &alice, &bob).await;

    let (status, body) =
        call(&app, "POST", "/_matrix/vendor/v1/create_dm", &bob.token, Some(json!({ "user_id": alice.user_id }))).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "creating a DM with someone who blocked you must fail (B-3), got {status} {body}"
    );
}

#[tokio::test]
async fn blocked_user_cannot_send_friend_request() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "blk_req_a").await;
    let bob = register(&app, "blk_req_b").await;
    befriend(&app, &alice, &bob).await;
    block(&app, &alice, &bob).await;

    // Bob 先把自己那边的 Alice 移除（单向：Alice 侧仍保留 blocked 记录），
    // 这样「已是好友」不会再抢在拉黑判定之前返回 409。
    let (status, body) =
        call(&app, "DELETE", &format!("/_matrix/vendor/v1/friends/{}", alice.user_id), &bob.token, None).await;
    assert_eq!(status, StatusCode::OK, "remove friend failed: {body}");

    let (status, body) =
        call(&app, "POST", "/_matrix/vendor/v1/friends", &bob.token, Some(json!({ "user_id": alice.user_id }))).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a blocked user must not be able to send a friend request again (B-3), got {status} {body}"
    );
}

/// 反向不成立：拉黑方自己仍可以建 DM —— 拉黑不该变成不可逆操作。
#[tokio::test]
async fn blocker_can_still_create_dm() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let alice = register(&app, "blk_rev_a").await;
    let bob = register(&app, "blk_rev_b").await;
    befriend(&app, &alice, &bob).await;
    block(&app, &alice, &bob).await;

    let (status, body) =
        call(&app, "POST", "/_matrix/vendor/v1/create_dm", &alice.token, Some(json!({ "user_id": bob.user_id }))).await;

    assert_eq!(status, StatusCode::OK, "the blocker keeps control and may still create a DM: {body}");
}
