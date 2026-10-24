#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第四波 1/3 —— `registration_token` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/registration_token_shape_only_tests.rs` 只断言自己构造的字面量，改坏
//! `routes/admin/token.rs` 的五个 handler 都不会红；此前这一族在测试里只出现在
//! `route_ledger_*.snapshot` 台账快照中（连请求都没发过）。
//!
//! 端点（`/_synapse/admin/v1/registration_tokens`，`AdminUser` 门禁）：
//! `GET` 列表 / `POST` 创建 / `GET|POST|DELETE` 单个。
//!
//! 三条值得钉住的语义：
//! 1. `uses_allowed: 0` 的对外表示是 **`null`（= 不限次数）**，不是 0 —— 客户端按 null 显示
//!    「无限制」，写成 0 会被读成「一次都不能用」；
//! 2. 不传 `token` 时服务端**生成**一个（默认长度 16），客户端必须能拿到非空值；
//! 3. 删除不存在的 token ⇒ **404**（服务层先查存在性），不能 200 假装删掉；
//! 4. **RBAC 分工是刻意的**（`utils/admin_auth/allowlist.rs:109`）：
//!    `g(Read, Contains("/registration_tokens"))` ⇒ 普通 `admin` 只读，
//!    增删改要 `super_admin`（"令牌增删是部署级动作"）。所以写操作用
//!    `super::get_super_admin_token`，并**单独一条**用例把"admin 能读不能写"钉住 ——
//!    这既是权限边界，也是这条策略没被顺手放宽的证据。

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

fn delete(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn post_json(uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
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

async fn register(app: &axum::Router, username: &str) -> String {
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
    let (status, value) = call(app, request).await;
    assert_eq!(status, StatusCode::OK, "注册必须是 200：{value}");
    value["access_token"].as_str().expect("注册必须返回 access_token").to_string()
}

const LIST_URI: &str = "/_synapse/admin/v1/registration_tokens";

/// 主干：创建（显式 token）→ 详情 → 列表 → 改 `uses_allowed` → 删除 → 再取 404。
#[tokio::test]
async fn registration_token_crud_round_trip() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    // 增删改要 super_admin（admin 只读，见文件头第 4 条）
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    // ① 创建：显式指定 token + 5 次上限 + 远未来过期时间（毫秒）
    let expiry = 4_102_444_800_000_i64; // 2100-01-01T00:00:00Z
    let body = json!({ "token": "wave-four-token", "uses_allowed": 5, "expiry_time": expiry });
    let (status, created) = call(&app, post_json(LIST_URI, &admin_token, &body)).await;
    assert_eq!(status, StatusCode::OK, "创建 token 必须 200：{created}");
    assert_eq!(created["token"], "wave-four-token", "必须回显调用方指定的 token：{created}");
    assert_eq!(created["uses_allowed"], 5, "uses_allowed 必须原样保存：{created}");
    assert_eq!(created["completed"], 0, "新建 token 的已用次数必须是 0：{created}");
    assert_eq!(created["expiry_time"], expiry, "过期时间必须原样保存：{created}");
    assert!(
        created["created_ts"].as_i64().unwrap_or(0) > 0,
        "created_ts 必须是正的时间戳（客户端据此排序）：{created}"
    );

    // ② 详情
    let uri = format!("{LIST_URI}/wave-four-token");
    let (status, detail) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "详情必须 200：{detail}");
    assert_eq!(detail["uses_allowed"], 5, "{detail}");

    // ③ 列表：必须能找到它，且 `next_batch` 键存在（客户端按这两个键分页）
    let (status, list) = call(&app, get(LIST_URI, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "列表必须 200：{list}");
    assert!(list.get("next_batch").is_some(), "列表必须带 next_batch 键：{list}");
    let tokens = list["registration_tokens"]
        .as_array()
        .unwrap_or_else(|| panic!("列表必须是 {{registration_tokens: [...]}}：{list}"));
    assert!(tokens.iter().any(|t| t["token"] == "wave-four-token"), "列表里必须能找到刚建的那个 token：{list}");

    // ④ 更新上限 5 → 9
    let (status, updated) = call(&app, post_json(&uri, &admin_token, &json!({ "uses_allowed": 9 }))).await;
    assert_eq!(status, StatusCode::OK, "更新必须 200：{updated}");
    assert_eq!(updated["uses_allowed"], 9, "更新后的上限必须生效：{updated}");

    // ⑤ 删除 → 再取必须 404
    let (status, deleted) = call(&app, delete(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "删除必须 200：{deleted}");
    let (status, after) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "删除后详情必须 404：{after}");
    assert_eq!(after["errcode"], "M_NOT_FOUND", "errcode：{after}");
}

/// 不传 `token` ⇒ 服务端生成；`uses_allowed` 缺省（0）对外必须是 **null**（不限次数）。
#[tokio::test]
async fn registration_token_generated_when_token_omitted() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    let (status, created) = call(&app, post_json(LIST_URI, &admin_token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "缺省创建必须 200：{created}");

    let token = created["token"].as_str().unwrap_or_else(|| panic!("必须生成 token：{created}"));
    assert!(!token.is_empty(), "生成的 token 不能是空串：{created}");
    assert_eq!(token.len(), 16, "默认长度应为 16（`length` 未指定时）：{created}");

    // `max_uses == 0` ⇒ 对外 `null`（不限次数）。写成 0 会被客户端读成"一次都不能用"。
    assert!(created["uses_allowed"].is_null(), "不限次数必须表示为 null，而不是 0：{created}");
}

/// 删除不存在的 token ⇒ 404（不能 200 假装删掉）。
#[tokio::test]
async fn delete_unknown_registration_token_is_not_found() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    let uri = format!("{LIST_URI}/does-not-exist");
    let (status, value) = call(&app, delete(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "删除不存在的 token 必须 404：{value}");
    assert_eq!(value["errcode"], "M_NOT_FOUND", "errcode：{value}");
}

/// 非法游标 ⇒ 400（列表分页不能静默从头开始，否则客户端会无限翻同一页）。
#[tokio::test]
async fn list_rejects_invalid_cursor() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, value) = call(&app, get(&format!("{LIST_URI}?from=not-a-cursor"), &admin_token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法 from 游标必须 400：{value}");
    let errcode = value["errcode"].as_str().unwrap_or_default();
    assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{value}");
}

/// 普通 `admin` 能**读**列表，但**不能**创建 —— 增删令牌是部署级动作（只给 `super_admin`）。
#[tokio::test]
async fn admin_can_read_but_not_create_registration_tokens() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, list) = call(&app, get(LIST_URI, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "普通 admin 必须能读列表（allowlist 是 Read 级放行）：{list}");

    let (status, denied) = call(&app, post_json(LIST_URI, &admin_token, &json!({ "uses_allowed": 1 }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通 admin 不能创建令牌：{denied}");
    assert_eq!(denied["errcode"], "M_FORBIDDEN", "errcode：{denied}");
}

/// 普通用户 ⇒ 403 `M_FORBIDDEN`（这一族必须真的挂在 `AdminUser` 下）。
#[tokio::test]
async fn registration_token_requires_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let user_token = register(&app, "regtoken_plain_user").await;

    let (status, value) = call(&app, get(LIST_URI, &user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通用户必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}
