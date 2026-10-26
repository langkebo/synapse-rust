#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第五波 1/3 —— `media_quota` 端点族的**真 HTTP 往返**。
//!
//! `tests/unit/media_quota_shape_only_tests.rs` 只断言自己构造的字面量，改坏
//! `routes/media/quota.rs` 或 `media_domain_service` 都不会红。本文件打真 router：
//!
//! * `GET /_matrix/media/v1/quota/check` —— `{limit, used, remaining, rule}`
//! * `GET /_matrix/media/v1/quota/stats` —— `{user_id, storage_bytes, media_count, limit_bytes, statistics}`
//! * `GET /_matrix/media/v1/quota/alerts` —— `{alerts: [...]}`
//! * `POST /_matrix/media/v3/delete/{server}/{media_id}` —— 只能删**自己的**媒体
//!
//! ⚠️ **quota 只挂在 legacy v1 前缀**（`/_matrix/media/v1/quota/*`），**不在**现代
//! `/_matrix/client/v1/media/` 下 —— 这不是笔误：`routes/media/mod.rs` 的单测
//! `test_media_nested_router_boundaries` 用 `v1_only_paths = ["/_matrix/media/v1/quota/check"]`
//! 显式钉住了这个边界。写成本文件的 v1 路径才能打到；用现代前缀会 404 `M_UNRECOGNIZED`。
//!
//! 主干不是"三个端点各返回 200"，而是**额度账要能对上**：上传后 `used` 涨、`stats.storage_bytes`
//! 与 `check.used` **必须相等**（两个端点各算一遍，算错就会让配额显示与实际占用分叉），
//! 删除后 `used` 回落。只断言状态码的话，"`used` 永远返回 0"这种缺陷照样绿。

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

fn post_json(uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn upload(uri: &str, token: &str, bytes: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/octet-stream")
        .body(Body::from(bytes))
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

const CHECK: &str = "/_matrix/media/v1/quota/check";
const STATS: &str = "/_matrix/media/v1/quota/stats";

async fn used_bytes(app: &axum::Router, token: &str) -> i64 {
    let (status, value) = call(app, get(CHECK, token)).await;
    assert_eq!(status, StatusCode::OK, "quota/check 必须 200：{value}");
    value["used"].as_i64().unwrap_or_else(|| panic!("check 必须带数值 used：{value}"))
}

/// `check` 的算术必须自洽：`remaining == limit - used`（`used >= limit` 时 `remaining` 是 0）；
/// `limit == 0`（不限额）时 `remaining` 必须是 `null`。
#[tokio::test]
async fn quota_check_reports_consistent_math() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "quota_math_user").await;

    let (status, check) = call(&app, get(CHECK, &token)).await;
    assert_eq!(status, StatusCode::OK, "quota/check 必须 200：{check}");

    let limit = check["limit"].as_i64().unwrap_or_else(|| panic!("必须带数值 limit：{check}"));
    let used = check["used"].as_i64().unwrap_or_else(|| panic!("必须带数值 used：{check}"));

    assert!(limit >= 0, "limit 不能是负数（0 表示**不限额**，校验处靠 `if max_storage > 0` 才拦）：{check}");
    assert!(used >= 0, "已用字节不能是负数：{check}");
    if limit == 0 {
        // `limit == 0` 是"不限额"（上传实际能成功，见主干用例）。此时 remaining 报 `null`
        // 而不是 0，否则客户端会把 0 显示成"空间已满"（审计 NEW-16 裁决）。
        assert!(check["remaining"].is_null(), "不限额时 remaining 必须为 null（不是 0）：{check}");
    } else {
        let remaining = check["remaining"].as_i64().unwrap_or_else(|| panic!("必须带数值 remaining：{check}"));
        let expected = if used >= limit { 0 } else { limit - used };
        assert_eq!(remaining, expected, "remaining 必须等于 limit-used（算错会让客户端误判剩余空间）：{check}");
    }
    assert!(
        check["rule"].as_str().is_some_and(|rule| !rule.is_empty()),
        "必须说明命中的配额规则（客户端拿它提示用户）：{check}"
    );
}

/// 主干：上传 → `used` 涨 → `stats.storage_bytes` 与 `check.used` **一致** → 删除 → `used` 回落。
#[tokio::test]
async fn upload_then_delete_keeps_quota_accounting_consistent() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "quota_chain_user").await;
    let baseline = used_bytes(&app, &token).await;

    // 上传 1024 字节（额度账按字节算，别用 1 字节 —— 否则"少算一个字节"的缺陷测不出来）
    let payload = vec![0x41u8; 1024];
    let (status, uploaded) =
        call(&app, upload("/_matrix/media/v3/upload?filename=quota-probe.bin", &token, payload)).await;
    assert_eq!(status, StatusCode::OK, "上传必须 200：{uploaded}");
    let content_uri =
        uploaded["content_uri"].as_str().unwrap_or_else(|| panic!("上传必须返回 content_uri：{uploaded}"));
    let (server, media_id) = content_uri
        .strip_prefix("mxc://")
        .and_then(|rest| rest.split_once('/'))
        .unwrap_or_else(|| panic!("content_uri 必须是 mxc://server/id：{content_uri}"));

    // used 必须涨至少 1024
    let after_upload = used_bytes(&app, &token).await;
    assert!(
        after_upload >= baseline + 1024,
        "上传 1024 字节后 used 必须至少涨 1024（baseline={baseline}, after={after_upload}）"
    );

    // 两个端点各算一遍，必须相等 —— 否则配额显示与实际占用分叉
    let (status, stats) = call(&app, get(STATS, &token)).await;
    assert_eq!(status, StatusCode::OK, "quota/stats 必须 200：{stats}");
    assert_eq!(
        stats["storage_bytes"].as_i64(),
        Some(after_upload),
        "stats.storage_bytes 必须与 check.used 相等（两个端点不能各算一套）：{stats}"
    );
    assert!(stats["media_count"].as_i64().unwrap_or(0) >= 1, "上传后 media_count 至少是 1：{stats}");

    // 删除自己的媒体 → used 必须回落
    let delete_uri = format!("/_matrix/media/v3/delete/{server}/{media_id}");
    let (status, deleted) = call(&app, post_json(&delete_uri, &token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "删除自己的媒体必须 200：{deleted}");
    assert_eq!(deleted["deleted"], true, "{deleted}");

    let after_delete = used_bytes(&app, &token).await;
    assert_eq!(
        after_delete, baseline,
        "删除后 used 必须回到 baseline（{baseline}），实际 {after_delete} —— 不回落说明额度账只加不减"
    );
}

/// `alerts` 的空态必须是 `[]`（不是 `null`：客户端按数组解，null 会让告警列表崩）。
#[tokio::test]
async fn quota_alerts_empty_state_is_an_array() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let token = register(&app, "quota_alert_user").await;

    let (status, value) = call(&app, get("/_matrix/media/v1/quota/alerts", &token)).await;
    assert_eq!(status, StatusCode::OK, "quota/alerts 必须 200：{value}");
    let alerts = value["alerts"].as_array().unwrap_or_else(|| panic!("alerts 必须是数组：{value}"));
    assert!(alerts.is_empty(), "全新用户的告警列表应为空：{value}");
}

/// 别人上传的媒体不能删 —— 403（访问控制不是参数校验）。
#[tokio::test]
async fn delete_other_users_media_is_forbidden() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let owner = register(&app, "quota_owner_user").await;
    let intruder = register(&app, "quota_intruder_user").await;

    let (status, uploaded) =
        call(&app, upload("/_matrix/media/v3/upload?filename=owned.bin", &owner, vec![0x42u8; 64])).await;
    assert_eq!(status, StatusCode::OK, "上传必须 200：{uploaded}");
    let content_uri = uploaded["content_uri"].as_str().expect("必须返回 content_uri");
    let (server, media_id) = content_uri
        .strip_prefix("mxc://")
        .and_then(|rest| rest.split_once('/'))
        .unwrap_or_else(|| panic!("content_uri 必须是 mxc://server/id：{content_uri}"));

    let delete_uri = format!("/_matrix/media/v3/delete/{server}/{media_id}");
    let (status, denied) = call(&app, post_json(&delete_uri, &intruder, &json!({}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "删别人的媒体必须 403：{denied}");
    assert_eq!(denied["errcode"], "M_FORBIDDEN", "errcode：{denied}");
}

/// 未认证访问 `quota/check` ⇒ 401（额度是按用户算的，不能匿名查）。
#[tokio::test]
async fn quota_requires_authentication() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };

    let request = Request::builder().method("GET").uri(CHECK).body(Body::empty()).unwrap();
    let (status, value) = call(&app, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未认证必须 401：{value}");
    let errcode = value["errcode"].as_str().unwrap_or_default();
    assert!(errcode.starts_with("M_"), "必须带 Matrix errcode：{value}");
}
