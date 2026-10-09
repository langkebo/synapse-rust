//! S25 / WEB-01：`POST /createRoom` 鉴权回归测试。
//!
//! 问题背景：create_room 此前手动 `bearer_token` + `validate_token`，
//! 跳过了 AuthenticatedUser 提取器内置的审计埋点，且丢弃了 `is_guest`
//! 标志 —— 访客账号可以自由建房（Matrix 规范与 Synapse 行为均为拒绝）。
//!
//! 本文件锁定两条行为：
//!   1. 访客 token 建房必须返回 403 M_FORBIDDEN；
//!   2. 普通用户建房不受影响（防止提取器改造误伤 happy path）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

// 注意：必须持有 TestContext 直到测试结束。`setup_fresh_test_app()` 只返回
// Router 并立即 drop 租约，后台 TRUNCATE 会清空 schema 并被其他并发测试
// 复用 —— 本文件的用例有两个串行请求，必现竞态（guest 用户被清空 → 401）。
async fn setup_test_ctx() -> Option<super::TestContext> {
    super::TestContext::new().await
}

/// 通过 `kind=guest` 注册访客账号，返回 access_token。
///
/// 测试配置固定 `enable_registration = true`（见 `test_config::build_test_config`），
/// 访客注册因此必然可用。此处**不得**静默跳过：那样一来，「访客建房必须 403」的
/// 断言会在注册意外失败时（路由回归 / DB 故障）完全不执行而假绿。失败即真实缺陷，
/// 直接 panic。
async fn register_guest(app: &axum::Router) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register?kind=guest")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "test config enables registration, so guest registration must succeed; body={}",
        String::from_utf8_lossy(&body)
    );
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().expect("register response must include access_token").to_owned()
}

#[tokio::test]
async fn test_create_room_rejects_guest_token() {
    let Some(ctx) = setup_test_ctx().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let guest_token = register_guest(app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {guest_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "guest room"}).to_string()))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request)).await.unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "访客不得创建房间; body={}", String::from_utf8_lossy(&body));
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"].as_str(), Some("M_FORBIDDEN"));
}

#[tokio::test]
async fn test_create_private_room_rejects_guest_token() {
    let Some(ctx) = setup_test_ctx().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let guest_token = register_guest(app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/vendor/v1/rooms/create_private")
        .header("Authorization", format!("Bearer {guest_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "guest private room"}).to_string()))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request)).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN, "访客不得通过 create_private 建房");
}

#[tokio::test]
async fn test_create_room_regular_user_still_succeeds() {
    let Some(ctx) = setup_test_ctx().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let token = super::create_test_user(app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "normal room"}).to_string()))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK, "普通用户建房不得被提取器改造误伤");
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["room_id"].as_str().is_some(), "响应必须包含 room_id");
}

#[tokio::test]
async fn test_create_room_rejects_duplicate_name_then_allows_ignore_flag() {
    let Some(ctx) = setup_test_ctx().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let token = super::create_test_user(app).await;

    // 第一次创建 "dup-room" 成功
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "dup-room"}).to_string()))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "首次创建同名房间应成功");

    // 第二次同名创建应返回 409 M_ROOM_IN_USE
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "dup-room"}).to_string()))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT, "同名创建应返回 409");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"].as_str(), Some("M_ROOM_IN_USE"));

    // 带 ignore_duplicate_name=true 重发应绕过查重并成功
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "dup-room", "ignore_duplicate_name": true}).to_string()))
        .unwrap();
    let response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(request)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "ignore_duplicate_name=true 应跳过查重");
}
