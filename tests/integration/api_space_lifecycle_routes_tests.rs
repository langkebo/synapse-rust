#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第三波 1/3 —— `space` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/space_shape_only_tests.rs` 只断言自己构造的字面量，改坏 `routes/space/**`
//! 的任何 handler 都不会红。本文件打真 router：
//!
//! * 私有面在 `/_matrix/vendor/v1`（ISSUE-13：client 别名已删），
//!   `POST /spaces` 的 `room_id` 是**必填**（space 包在一条既有房间上）；
//! * 规范面（MSC2946 hierarchy）留在 `/_matrix/client/{v1,v3}`。
//!
//! 覆盖：创建 → 读回 → 加子项 → 列子项 → 空 room_id 校验 → 未认证。

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

async fn register(app: &axum::Router, username: &str) -> String {
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
    value["access_token"].as_str().expect("注册必须返回 access_token").to_string()
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let (status, value) =
        call(app, post_json("/_matrix/client/v3/createRoom", Some(token), &json!({ "name": name }))).await;
    assert_eq!(status, StatusCode::OK, "建房必须是 200：{value}");
    value["room_id"].as_str().expect("建房必须返回 room_id").to_string()
}

/// 主干：把一条既有房间包成 space，再按 `space_id` 读回。
#[tokio::test]
async fn create_space_then_read_back() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "space_owner").await;
    let room_id = create_room(&app, &token, "Space Backing Room").await;

    let (status, created) = call(
        &app,
        post_json("/_matrix/vendor/v1/spaces", Some(&token), &json!({ "room_id": room_id, "name": "My Space" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "创建 space 必须是 201：{created}");
    let space_id = created["space_id"].as_str().unwrap_or_else(|| panic!("必须返回 space_id：{created}"));
    assert_eq!(created["room_id"], room_id.as_str(), "space 必须挂在传入的 room 上：{created}");
    assert_eq!(created["name"], "My Space", "名字必须原样落库：{created}");
    assert!(created["creator"].as_str().is_some_and(|c| c.starts_with('@')), "必须带 creator：{created}");

    let uri = format!("/_matrix/vendor/v1/spaces/{space_id}");
    let (status, fetched) = call(&app, get(&uri, Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "读回 space 必须 200：{fetched}");
    assert_eq!(fetched["name"], "My Space", "读回的名字必须与写入一致：{fetched}");
    assert_eq!(fetched["space_id"], space_id, "读回的 space_id 必须与创建时一致：{fetched}");
}

/// 子项：加一条房间 → 列表里能看到；`via_servers` 原样保存。
#[tokio::test]
async fn space_child_add_and_list() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "space_parent").await;
    let backing = create_room(&app, &token, "Backing").await;
    let child_room = create_room(&app, &token, "Child Room").await;

    let (status, created) = call(
        &app,
        post_json("/_matrix/vendor/v1/spaces", Some(&token), &json!({ "room_id": backing, "name": "Parent" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let space_id = created["space_id"].as_str().unwrap().to_string();

    let children_uri = format!("/_matrix/vendor/v1/spaces/{space_id}/children");
    let (status, added) = call(
        &app,
        post_json(&children_uri, Some(&token), &json!({ "room_id": child_room, "via_servers": ["localhost"] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "加子项必须 201：{added}");
    assert_eq!(added["space_id"], space_id.as_str(), "{added}");
    assert_eq!(added["room_id"], child_room.as_str(), "{added}");
    assert_eq!(added["via_servers"], json!(["localhost"]), "via_servers 必须原样保存：{added}");

    let (status, listed) = call(&app, get(&children_uri, Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let children = listed.as_array().unwrap_or_else(|| panic!("children 必须是数组：{listed}"));
    assert!(
        children.iter().any(|child| child["room_id"] == child_room.as_str()),
        "列表里必须能看到刚加的子房间：{listed}"
    );
}

/// `room_id` 是必填且非空（`#[validate(length(min = 1))]`）⇒ 空串必须 400，不能建出孤儿 space。
#[tokio::test]
async fn create_space_with_empty_room_id_is_rejected() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "space_empty").await;

    let (status, value) =
        call(&app, post_json("/_matrix/vendor/v1/spaces", Some(&token), &json!({ "room_id": "" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "空 room_id 必须 400：{value}");
    assert!(value["errcode"].as_str().is_some_and(|code| code.starts_with("M_")), "必须带 Matrix errcode：{value}");
}

/// 未认证 ⇒ 401（space 是全私有面，不能匿名列/建）。
#[tokio::test]
async fn space_endpoints_require_authentication() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) =
        call(&app, post_json("/_matrix/vendor/v1/spaces", None, &json!({ "room_id": "!x:localhost" }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未认证必须 401：{value}");
    assert_eq!(value["errcode"], "M_UNAUTHORIZED", "errcode：{value}");
}
