//! RBAC 的**线上链路**：中间件 → token → `user_type` → 角色 → 判定 → 403。
//!
//! `admin_rbac_tests`（纯函数层）证明"规则写得对不对"；这个文件证明"规则真的被用上"。
//! 两者缺一不可——判定再正确，只要中间件没调它、或者路径/角色传错，线上就是敞开的。
//!
//! 为什么现在才补：`permission_escalation_tests.rs` 里原有两条 `test_rbac_*` 是
//! `assert_eq!("super_admin", "super_admin")` 这种**空转**断言，看起来"RBAC 有测试覆盖"，
//! 实际上一条 HTTP 请求都没发过。那两条已删除，改为指向本文件。
//!
//! 判定用的三个事实（任何一条变化都会让这些测试红，这正是目的）：
//! + `default_admin_rbac_enabled()` = true ⇒ 测试栈默认就开着 RBAC；
//! + `admin_auth_middleware` 把 `request.uri().path()` 原样交给 `is_role_allowed`；
//! + 角色来自用户的 `user_type`（未设置 ⇒ `none` ⇒ 一律拒绝）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

async fn status(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<serde_json::Value>,
) -> StatusCode {
    let mut builder = Request::builder().method(method).uri(uri).header("Authorization", format!("Bearer {token}"));
    let request = match body {
        Some(body) => {
            builder = builder.header("Content-Type", "application/json");
            builder.body(Body::from(body.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap().status()
}

/// 同一条请求，普通 admin 得到 **403**（RBAC 在 handler 之前拦下），super_admin 得到
/// **404**（穿过了 RBAC，落到 handler 的"房间不存在"）。
///
/// 403 vs 404 这个对比是关键：如果只看 403，无法区分"RBAC 拦了"和"handler 自己拒绝了"；
/// 404 证明 super_admin 确实到了业务层。
#[tokio::test]
async fn plain_admin_is_denied_on_a_blacklisted_write_before_the_handler_runs() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;
    let (super_token, _) = super::get_super_admin_token(&app).await;
    // `DELETE /rooms/{room_id}` 属于"不可逆且影响全体成员"，第一批已收紧到 super_admin。
    let uri = "/_synapse/admin/v1/rooms/%21nonexistent%3Alocalhost";

    assert_eq!(
        status(&app, "DELETE", uri, &plain_token, None).await,
        StatusCode::FORBIDDEN,
        "a plain admin deleting a room must be stopped by RBAC, not by the handler"
    );
    assert_eq!(
        status(&app, "DELETE", uri, &super_token, None).await,
        StatusCode::NOT_FOUND,
        "super_admin must get past RBAC and reach the handler (room does not exist ⇒ 404)"
    );
}

/// 正控：允许面里的读端点在线上真的可用（否则"收紧"可能把整个 admin 面一起堵掉）。
#[tokio::test]
async fn plain_admin_can_reach_an_allowed_read() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_token, _) = super::get_admin_token(&app).await;

    assert_eq!(status(&app, "GET", "/_synapse/admin/v1/users", &plain_token, None).await, StatusCode::OK);
}

/// `auditor` 在**纯函数层**只有 12 条可及面（audit / event_reports / security 的读），
/// 这条测试把它放到线上验证：审计能读，用户列表读不了，审计也写不了。
#[tokio::test]
async fn auditor_role_is_read_only_and_confined_to_audit() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (auditor_token, _) = super::get_admin_token_with_role(&app, "auditor").await;

    assert_eq!(
        status(&app, "GET", "/_synapse/admin/v1/audit/events", &auditor_token, None).await,
        StatusCode::OK,
        "auditor 的职责就是读审计"
    );
    assert_eq!(
        status(&app, "GET", "/_synapse/admin/v1/users", &auditor_token, None).await,
        StatusCode::FORBIDDEN,
        "auditor 不该能翻用户列表——'只读'不等于'什么都能读'"
    );
    assert_eq!(
        status(
            &app,
            "POST",
            "/_synapse/admin/v1/audit/events",
            &auditor_token,
            Some(json!({ "action": "forged", "resource_type": "user", "resource_id": "@x:localhost" })),
        )
        .await,
        StatusCode::FORBIDDEN,
        "手写审计事件会污染审计链，只有 super_admin 能做"
    );
}

/// `media_admin` 同理：媒体能读，用户列表不能。
#[tokio::test]
async fn media_admin_role_is_confined_to_media() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (media_token, _) = super::get_admin_token_with_role(&app, "media_admin").await;

    assert_eq!(status(&app, "GET", "/_synapse/admin/v1/media", &media_token, None).await, StatusCode::OK);
    assert_eq!(status(&app, "GET", "/_synapse/admin/v1/users", &media_token, None).await, StatusCode::FORBIDDEN);
}

/// `user_admin`：用户能管，媒体不能。
#[tokio::test]
async fn user_admin_role_is_confined_to_users() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (user_admin_token, _) = super::get_admin_token_with_role(&app, "user_admin").await;
    assert_eq!(status(&app, "GET", "/_synapse/admin/v1/users", &user_admin_token, None).await, StatusCode::OK);
    // 目标不存在 ⇒ 404 说明穿过了 RBAC；403 就是被 RBAC 拦下。
    assert_eq!(
        status(&app, "GET", "/_synapse/admin/v1/media/abc123", &user_admin_token, None).await,
        StatusCode::FORBIDDEN,
        "user_admin 不该有机器的媒体面"
    );
}
