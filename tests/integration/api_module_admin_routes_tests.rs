#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第四波 2/3 —— `module` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/module_shape_only_tests.rs` 只断言自己构造的字面量（外加路由台账），改坏
//! `routes/module.rs` 的 23 个 handler 都不会红。本文件覆盖管理端模块注册表的完整生命周期：
//! 创建 → 详情 → 列表 → 改配置 → 启停 → 删除，另加账户有效期（`account_validity`）子资源。
//!
//! **RBAC 分工（实测，`utils/admin_auth/privileged.rs:64`）**：模块的**写**操作是
//! `super_admin` 专属（理由写在源码里：「可加载代码并介入消息流」），`/check_spam` 与
//! `/check_third_party_rule` 两个只读探测例外；`POST /account_validity`（全站有效期）同样是
//! super_admin，单用户 `…/{user_id}/renew` 保留给普通 admin。所以写操作用
//! `super::get_super_admin_token`，并单独一条用例钉住「admin 能读、不能建」。
//!
//! 为什么值得钉住「启停」：`is_enabled` 是**插件是否参与事件处理**的开关，若 `enable` 请求
//! 被静默忽略（比如 body 字段名写成 `enabled` 而不是 `is_enabled`），模块会一直停在创建时的
//! 状态，而管理端界面看上去"点过了"——这类漂移只有真往返能测出来。

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

fn put_json(uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
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

async fn register(app: &axum::Router, username: &str) -> (String, String) {
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
    let token = value["access_token"].as_str().expect("注册必须返回 access_token").to_string();
    let user_id = value["user_id"].as_str().expect("注册必须返回 user_id").to_string();
    (token, user_id)
}

const MODULES: &str = "/_synapse/admin/v1/modules";

/// 主干：创建(201) → 详情 → 列表 → 改配置 → 停用/启用 → 删除(204) → 再取 404。
#[tokio::test]
async fn module_lifecycle_round_trip() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    // 模块写操作是 super_admin 专属（见文件头 RBAC 说明）
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    let body = json!({
        "module_name": "wave_four_module",
        "module_type": "spam_checker",
        "version": "1.2.3",
        "description": "created by api_module_routes_tests",
        "is_enabled": true,
        "priority": 10,
        "config": { "threshold": 0.7 }
    });
    let (status, created) = call(&app, post_json(MODULES, &admin_token, &body)).await;
    assert_eq!(status, StatusCode::CREATED, "注册模块必须 201：{created}");
    assert_eq!(created["module_name"], "wave_four_module", "{created}");
    assert_eq!(created["module_type"], "spam_checker", "类型必须原样保存：{created}");
    assert_eq!(created["version"], "1.2.3", "{created}");
    assert_eq!(created["is_enabled"], true, "创建时 is_enabled 必须生效：{created}");
    assert_eq!(created["priority"], 10, "{created}");

    // 详情
    let uri = format!("{MODULES}/wave_four_module");
    let (status, detail) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "详情必须 200：{detail}");
    assert_eq!(detail["version"], "1.2.3", "{detail}");

    // 列表形状：{modules: [...], next_batch: ...}
    let (status, list) = call(&app, get(MODULES, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "列表必须 200：{list}");
    assert!(list.get("next_batch").is_some(), "列表必须带 next_batch 键：{list}");
    let modules = list["modules"].as_array().unwrap_or_else(|| panic!("列表必须是 {{modules: [...]}}：{list}"));
    assert!(modules.iter().any(|m| m["module_name"] == "wave_four_module"), "列表里必须能找到刚注册的模块：{list}");

    // 改配置
    let cfg_uri = format!("{uri}/config");
    let (status, updated) =
        call(&app, put_json(&cfg_uri, &admin_token, &json!({ "config": { "threshold": 0.9 } }))).await;
    assert_eq!(status, StatusCode::OK, "改配置必须 200：{updated}");
    assert_eq!(updated["config"]["threshold"], 0.9, "配置必须被覆盖（不是忽略）：{updated}");

    // 停用 → 详情必须看到 false（字段名是 is_enabled；写成 enabled 会被 deny_unknown_fields 拒掉）
    let enable_uri = format!("{uri}/enable");
    let (status, disabled) = call(&app, post_json(&enable_uri, &admin_token, &json!({ "is_enabled": false }))).await;
    assert_eq!(status, StatusCode::OK, "停用必须 200：{disabled}");
    assert_eq!(disabled["is_enabled"], false, "停用必须立刻反映：{disabled}");
    let (_, reread) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(reread["is_enabled"], false, "详情必须与停用结果一致（不能只是响应里改了）：{reread}");

    // 删除 → 204 → 再取 404
    let (status, deleted) = call(&app, delete(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "删除模块必须 204：{deleted}");
    let (status, after) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "删除后详情必须 404：{after}");
}

/// 缺必填字段 ⇒ 400（`module_name` / `module_type` / `version` 缺一个就不该建出记录）。
#[tokio::test]
async fn create_module_without_required_field_is_bad_json() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    let incomplete = json!({ "module_name": "no_type_module", "version": "1.0.0" });
    let (status, value) = call(&app, post_json(MODULES, &admin_token, &incomplete)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 module_type 必须 400：{value}");
    assert_eq!(value["errcode"], "M_BAD_JSON", "errcode：{value}");
}

/// `account_validity` 子资源：创建(201) → 查询 → 未登记的用户必须 404。
#[tokio::test]
async fn account_validity_round_trip() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let (_, user_id) = register(&app, "validity_target").await;

    // 未登记过有效期的用户 ⇒ 404（不能返回一个空壳让管理端以为"已设置"）
    let (status, missing) =
        call(&app, get(&format!("/_synapse/admin/v1/account_validity/{user_id}"), &admin_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "未登记有效期必须 404：{missing}");

    let body = json!({ "user_id": user_id, "expiration_ts": 4_102_444_800_000_i64, "is_valid": true });
    let (status, created) = call(&app, post_json("/_synapse/admin/v1/account_validity", &admin_token, &body)).await;
    assert_eq!(status, StatusCode::CREATED, "创建账户有效期必须 201：{created}");

    let uri = format!("/_synapse/admin/v1/account_validity/{user_id}");
    let (status, detail) = call(&app, get(&uri, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "查询账户有效期必须 200：{detail}");
    assert_eq!(detail["user_id"], user_id.as_str(), "必须是刚注册的那个用户：{detail}");
    assert_eq!(detail["is_valid"], true, "is_valid 必须原样保存：{detail}");
}

/// 普通 `admin` 能读模块列表，但**不能**注册模块（写操作是 `super_admin` 专属）。
#[tokio::test]
async fn admin_can_list_modules_but_not_register_them() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, list) = call(&app, get(MODULES, &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "普通 admin 必须能读模块列表：{list}");

    let body = json!({ "module_name": "denied_module", "module_type": "spam_checker", "version": "1.0.0" });
    let (status, denied) = call(&app, post_json(MODULES, &admin_token, &body)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通 admin 不能注册模块：{denied}");
    assert_eq!(denied["errcode"], "M_FORBIDDEN", "errcode：{denied}");
}

/// 普通用户 ⇒ 403（这一族的 23 条路由必须都在 `AdminUser` 下）。
#[tokio::test]
async fn module_endpoints_require_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (user_token, _) = register(&app, "module_plain_user").await;

    let (status, value) = call(&app, get(MODULES, &user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通用户必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}
