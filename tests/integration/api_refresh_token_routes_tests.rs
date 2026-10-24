#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第五波 3/3 —— `refresh_token` 端点族的**真 HTTP 往返**（含轮换与复用检测）。
//!
//! 此前这一族只有 `refresh_token_storage_tests_migrated`（storage 级）+ 一条"挂载烟测"
//! （`api_auth_routes_tests.rs` 里断言 `POST /refresh` 不是 404/405），**没有任何标签验证过
//! 轮换语义**。本文件补的是真正危险的那部分：
//!
//! 1. 登录 → 拿到 `refresh_token`；
//! 2. `POST /_matrix/client/v3/refresh` → **新的 access_token + 新的 refresh_token**
//!    （同一个 `device_id`，`expires_in > 0`）；
//! 3. 新 access_token 必须真的能用（`/account/whoami` 认得出同一个用户）；
//! 4. **复用旧 refresh_token ⇒ 401，且已下发的 access token 也立刻失效**（NEW-18 已修：
//!    复用分支除 `revoke_all_user_tokens` 外，还会按 hash 拉黑、失效 `revocation_ok` 缓存、
//!    删 `access_tokens` 行并置 `user:logout_all:` 标记）—— 这是防盗刷的核心契约。

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

fn post_json(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

const REFRESH: &str = "/_matrix/client/v3/refresh";

/// 注册 + 登录，返回 `(access_token, refresh_token, device_id, user_id)`。
async fn login(app: &axum::Router, username: &str) -> (String, String, String, String) {
    let register = Request::builder()
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
    let (status, value) = call(app, register).await;
    assert_eq!(status, StatusCode::OK, "注册必须是 200：{value}");

    let login_body = json!({
        "type": "m.login.password",
        "identifier": { "type": "m.id.user", "user": username },
        "password": "Password123!"
    });
    let (status, login) = call(app, post_json("/_matrix/client/v3/login", &login_body)).await;
    assert_eq!(status, StatusCode::OK, "登录必须是 200：{login}");

    let access = login["access_token"].as_str().expect("登录必须返回 access_token").to_string();
    let refresh = login["refresh_token"]
        .as_str()
        .unwrap_or_else(|| panic!("登录必须返回 refresh_token（否则客户端无法续期）：{login}"))
        .to_string();
    let device = login["device_id"].as_str().expect("登录必须返回 device_id").to_string();
    let user = login["user_id"].as_str().expect("登录必须返回 user_id").to_string();
    (access, refresh, device, user)
}

/// 主干：refresh 必须换成**一对新令牌**，device_id 不变，expires_in 为正。
#[tokio::test]
async fn refresh_rotates_both_tokens_and_keeps_device() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (access, refresh, device, _user) = login(&app, "refresh_rotate_user").await;

    let (status, rotated) = call(&app, post_json(REFRESH, &json!({ "refresh_token": refresh }))).await;
    assert_eq!(status, StatusCode::OK, "refresh 必须 200：{rotated}");

    let new_access = rotated["access_token"].as_str().expect("必须返回新 access_token");
    let new_refresh = rotated["refresh_token"].as_str().expect("必须返回新 refresh_token");
    assert_ne!(new_access, access.as_str(), "access_token 必须换新（不换等于没续期）：{rotated}");
    assert_ne!(new_refresh, refresh.as_str(), "refresh_token 必须轮换（同一串反复可用是漏洞）：{rotated}");
    assert_eq!(rotated["device_id"], device.as_str(), "续期必须保持同一个 device_id：{rotated}");
    assert!(
        rotated["expires_in"].as_i64().unwrap_or(0) > 0,
        "必须返回正的 expires_in（客户端据此安排下次续期）：{rotated}"
    );
}

/// 轮换后的新 access_token 必须真的能用（不是只在响应里改了个值）。
#[tokio::test]
async fn refreshed_access_token_is_usable() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (_access, refresh, _device, user) = login(&app, "refresh_usable_user").await;

    let (status, rotated) = call(&app, post_json(REFRESH, &json!({ "refresh_token": refresh }))).await;
    assert_eq!(status, StatusCode::OK, "refresh 必须 200：{rotated}");
    let new_access = rotated["access_token"].as_str().expect("必须返回新 access_token").to_string();

    let (status, whoami) = call(&app, get("/_matrix/client/v3/account/whoami", &new_access)).await;
    assert_eq!(status, StatusCode::OK, "新 access_token 必须能访问 whoami：{whoami}");
    assert_eq!(whoami["user_id"], user.as_str(), "whoami 必须认出同一个用户：{whoami}");
}

/// 复用已轮换掉的 refresh_token ⇒ 401，**并且新 token 也一起失效**（全局吊销）。
#[tokio::test]
async fn reusing_old_refresh_token_revokes_all_tokens() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (_access, refresh, _device, _user) = login(&app, "refresh_reuse_user").await;

    // 第一次续期：成功，拿到新的一对
    let (status, rotated) = call(&app, post_json(REFRESH, &json!({ "refresh_token": refresh }))).await;
    assert_eq!(status, StatusCode::OK, "首次 refresh 必须 200：{rotated}");
    let new_access = rotated["access_token"].as_str().expect("必须返回新 access_token").to_string();

    // 再用旧 token（模拟被抓包重放）
    let (status, reused) = call(&app, post_json(REFRESH, &json!({ "refresh_token": refresh }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "复用旧 refresh_token 必须 401：{reused}");
    let errcode = reused["errcode"].as_str().unwrap_or_default();
    assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{reused}");

    // NEW-18（已修）：复用检出必须**同时掐断已下发的 access token**。此前只吊销
    // `refresh_tokens` 表（`refresh_token/mod.rs:624` 只 UPDATE 那一张），刚换到的
    // access_token 仍能用到自然过期 —— 实测确实是 200。现在复用分支会
    // 按 hash 拉黑 + 失效 `revocation_ok` 缓存 + 删 `access_tokens` 行 + 置 `user:logout_all:` 标记，
    // 所以同一个 token 必须立刻 401。
    let (status, whoami) = call(&app, get("/_matrix/client/v3/account/whoami", &new_access)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "检出重放后必须立刻掐断已下发的 access token（NEW-18）：{whoami}");
}

/// 缺 `refresh_token` 字段 ⇒ 400（不是 500，也不是 401）。
#[tokio::test]
async fn refresh_without_token_is_bad_request() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) = call(&app, post_json(REFRESH, &json!({}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 refresh_token 必须 400：{value}");
    assert_eq!(value["errcode"], "M_BAD_JSON", "errcode：{value}");
}

/// 随便编一个不存在的 refresh_token ⇒ 401。
#[tokio::test]
async fn refresh_with_unknown_token_is_unauthorized() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let (status, value) = call(&app, post_json(REFRESH, &json!({ "refresh_token": "not-a-real-token" }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未知 refresh_token 必须 401：{value}");
    let errcode = value["errcode"].as_str().unwrap_or_default();
    assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{value}");
}
