//! 「目标账户是管理员」时有守卫的账号可用性族。
//!
//! P0 只修了改密码（`reset_user_password`）。同一形状还有八条：硬删账号、停用、
//! 批量停用、踢下线、吊销会话、吊销 refresh token、shadow ban / unshadow ban ——
//! 普通 admin 用它们可以让 super_admin 消失、无法登录或静默失声。
//! 裁决与实现见 `docs/audit/ADMIN_RBAC_SURFACE_2026-10-21.md` §4.2。
//!
//! 这里只断言**方向**：目标是 super_admin ⇒ 403。每条再配一个"目标换成普通用户仍成功"
//! 的负控（守卫最容易的失败方式不是漏，而是把正常运维一起堵掉）。
//!
//! **2026-10-24（审计 TEST-02）**：原来只断言状态码，于是"403 但 errcode 是 `M_UNKNOWN`"
//! 或"403 但 error 为空"这类回归完全看不见 —— 客户端按 errcode 分支（`M_FORBIDDEN` vs
//! `M_UNKNOWN`）时会走错分支，而状态码一样。现在每个拒绝都断言
//! **状态码 + `errcode` + 非空 `error`** 三件套（`assert_forbidden`）。
//!
//! 判据（可失败）：把 `assert_forbidden` 的期望 errcode 改掉 ⇒ 立刻红，说明这三个字段
//! 真的被读取，而不是"解析失败退化成空对象也照样通过"。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

/// 把 `@user:localhost` 变成可直接放进路径的形式。
fn path_user_id(localpart: &str) -> String {
    format!("@{localpart}:localhost").replace('@', "%40").replace(':', "%3A")
}

/// 发一个请求，返回 `(状态码, 响应体解析出的 JSON)`。
///
/// 响应体解析失败时返回 `Value::Null` 而**不是** panic：403/500 的响应体形状本身就是要断言
/// 的东西，解析失败应该表现为断言失败（带可读的 body），而不是一个 `unwrap` 堆栈。
async fn call(app: &axum::Router, method: &str, uri: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri).header("Authorization", format!("Bearer {token}"));
    let request = match body {
        Some(body) => {
            builder = builder.header("Content-Type", "application/json");
            builder.body(Body::from(body.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap_or_default();
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

/// 断言"被目标守卫拒绝"的**完整**形状：403 + `M_FORBIDDEN` + 非空 `error`。
///
/// 只断言 403 会漏掉两类回归：errcode 被换成 `M_UNKNOWN`（客户端据此走"未知错误"分支），
/// 以及 error 为空（用户看到 403 但没有任何可读原因）。
#[track_caller]
fn assert_forbidden(response: (StatusCode, Value), context: &str) {
    let (status, body) = response;
    assert_eq!(status, StatusCode::FORBIDDEN, "{context}：普通 admin 对 super_admin 必须 403");
    assert_eq!(
        body["errcode"], "M_FORBIDDEN",
        "{context}：403 必须带 M_FORBIDDEN，否则客户端会按 errcode 走错分支；实际 body = {body}"
    );
    let message = body["error"].as_str().unwrap_or_default();
    assert!(!message.is_empty(), "{context}：403 必须带可读 error，实际 body = {body}");
}

/// 注册一个普通用户，返回可直接放进路径的 user id。
async fn register_plain_user(app: &axum::Router) -> String {
    let username = format!("guard_target_{}", rand::random::<u32>());
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
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the test user must exist before the guard can be exercised");
    format!("@{username}:localhost").replace('@', "%40").replace(':', "%3A")
}

#[tokio::test]
async fn test_plain_admin_cannot_delete_super_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;
    let uri = format!("/_synapse/admin/v1/users/{}", path_user_id(&super_localpart));

    assert_forbidden(
        call(&app, "DELETE", &uri, &plain_token, None).await,
        "hard-deleting a super_admin is strictly worse than resetting their password",
    );

    // v2 是同一个 handler 的第二条路径，也要挡住。
    let uri_v2 = format!("/_synapse/admin/v2/users/{}", path_user_id(&super_localpart));
    assert_forbidden(call(&app, "DELETE", &uri_v2, &plain_token, None).await, "DELETE /v2/users/{user}");
}

#[tokio::test]
async fn test_plain_admin_cannot_deactivate_super_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;
    let uri = format!("/_synapse/admin/v1/users/{}/deactivate", path_user_id(&super_localpart));

    assert_forbidden(call(&app, "POST", &uri, &plain_token, None).await, "POST /v1/users/{user}/deactivate");
}

#[tokio::test]
async fn test_plain_admin_cannot_deactivate_super_admin_via_legacy_path() {
    // `deactivate/{user_id}` 只是路径别名（B-32：上游 Element Admin 仍在用），
    // 共用同一个 handler，不能成为绕过守卫的旁路。
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;
    let uri = format!("/_synapse/admin/v1/deactivate/{}", path_user_id(&super_localpart));

    assert_forbidden(call(&app, "POST", &uri, &plain_token, None).await, "POST /v1/deactivate/{user}（legacy 别名）");
}

#[tokio::test]
async fn test_plain_admin_cannot_batch_deactivate_super_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;

    // 混在一个普通用户里：只要目标集合含管理员，整请求拒绝（不给"静默跳过"的假成功）。
    let plain_target = register_plain_user(&app).await.replace("%40", "@").replace("%3A", ":");
    let body = json!({ "users": [plain_target, format!("@{super_localpart}:localhost")] });

    assert_forbidden(
        call(&app, "POST", "/_synapse/admin/v1/users/batch_deactivate", &plain_token, Some(body)).await,
        "POST /v1/users/batch_deactivate（混合集合必须整请求拒绝）",
    );
}

#[tokio::test]
async fn test_plain_admin_cannot_logout_super_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;
    let uri = format!("/_synapse/admin/v1/users/{}/logout", path_user_id(&super_localpart));

    assert_forbidden(
        call(&app, "POST", &uri, &plain_token, None).await,
        "kicking a super_admin offline also purges their device keys",
    );
}

#[tokio::test]
async fn test_plain_admin_cannot_invalidate_super_admin_sessions() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;
    let uri = format!("/_synapse/admin/v1/user_sessions/{}/invalidate", path_user_id(&super_localpart));

    assert_forbidden(call(&app, "POST", &uri, &plain_token, None).await, "POST /v1/user_sessions/{user}/invalidate");
}

#[tokio::test]
async fn test_plain_admin_cannot_shadow_ban_super_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;
    let uri = format!("/_synapse/admin/v1/users/{}/shadow_ban", path_user_id(&super_localpart));

    assert_forbidden(call(&app, "POST", &uri, &plain_token, None).await, "POST /v1/users/{user}/shadow_ban");
}

/// 负控：守卫只看"目标是不是管理员"。普通用户仍然可以被普通 admin 停用
/// —— 否则这把守卫就把日常运维一起收走了。
///
/// 成功分支同样断言响应体：停用要回 `{id_server_unbind_result: "success"}`，
/// 只断言 200 的话，"200 但 body 形状变了"（客户端据此判断解绑结果）照样漏。
#[tokio::test]
async fn test_plain_admin_can_deactivate_plain_user() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let plain_target = register_plain_user(&app).await;
    let uri = format!("/_synapse/admin/v1/users/{plain_target}/deactivate");

    let (status, body) = call(&app, "POST", &uri, &plain_token, None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "deactivating a plain user is legitimate admin work and must not be caught by the target guard; body = {body}"
    );
    assert_eq!(
        body["id_server_unbind_result"], "success",
        "停用的成功响应必须带 id_server_unbind_result=success（客户端据此判断 3PID 解绑结果）；实际 body = {body}"
    );
}
