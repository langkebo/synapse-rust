#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NEW-09 第四波 3/3 —— `background_update` 端点族的**真 HTTP 往返**（状态机）。
//!
//! `tests/unit/background_update_shape_only_tests.rs` 只断言自己构造的字面量，改坏
//! `routes/background_update.rs`（19 条路由）或 `background_update_service` 都不会红。
//!
//! 这一族有**真实状态机**（`pending → running → completed | cancelled | failed`），所以断言
//! 落在"转移是否被记录"上，而不是只看状态码：
//! * `start` 只接受 `pending`，重复 `start` 必须 400（不是静默成功）；
//! * `start` 后 `started_ts` 必须被填、`complete` 后 `completed_ts` 必须被填
//!   （运维靠这两个时间戳算耗时，缺了就只能看日志）；
//! * `progress` 必须真的累加 `processed_items`，并且留下 history 记录；
//! * 查询未知任务 ⇒ 404；**删除未知任务当前返回 204**（不查存在性，与 `registration_tokens`
//!   的"先查再删、未知 404"不一致 ⇒ 审计 NEW-15，测试钉住实际行为）。

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

const BASE: &str = "/_synapse/admin/v1/background_updates";

async fn create_job(app: &axum::Router, admin_token: &str, job_name: &str) -> Value {
    let body = json!({
        "job_name": job_name,
        "job_type": "populate",
        "description": "created by api_background_update_routes_tests",
        "table_name": "events",
        "column_name": "stream_ordering",
        "total_items": 100,
        "batch_size": 10,
        "sleep_ms": 0
    });
    let (status, created) = call(app, post_json(BASE, admin_token, &body)).await;
    assert_eq!(status, StatusCode::CREATED, "创建后台任务是 201：{created}");
    created
}

/// 主干：创建(pending) → start(running, 带 started_ts) → progress 累加 → complete(completed, 带 completed_ts) → history。
#[tokio::test]
async fn background_update_state_machine_round_trip() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let created = create_job(&app, &admin_token, "wave4_bg_job").await;
    assert_eq!(created["job_name"], "wave4_bg_job", "{created}");
    assert_eq!(created["status"], "pending", "新建任务必须是 pending：{created}");
    assert_eq!(created["total_items"], 100, "total_items 必须原样保存：{created}");
    assert!(created["started_ts"].is_null(), "还没开始时 started_ts 必须是 null：{created}");

    // start ⇒ running，并填 started_ts
    let (status, started) =
        call(&app, post_json(&format!("{BASE}/wave4_bg_job/start"), &admin_token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "start 必须 200：{started}");
    assert_eq!(started["status"], "running", "start 后必须是 running：{started}");
    assert!(
        started["started_ts"].as_i64().unwrap_or(0) > 0,
        "start 后 started_ts 必须被填（运维靠它算耗时）：{started}"
    );

    // progress：processed_items 必须累加（不是"最后一次覆盖成 0"）
    let progress_uri = format!("{BASE}/wave4_bg_job/progress");
    let (status, p1) = call(&app, post_json(&progress_uri, &admin_token, &json!({ "items_processed": 30 }))).await;
    assert_eq!(status, StatusCode::OK, "progress 必须 200：{p1}");
    let (status, p2) = call(&app, post_json(&progress_uri, &admin_token, &json!({ "items_processed": 40 }))).await;
    assert_eq!(status, StatusCode::OK, "progress 必须 200：{p2}");
    assert_eq!(p2["processed_items"], 70, "两次进度必须累加（30+40）：{p2}");

    // complete ⇒ completed，并填 completed_ts
    let (status, completed) =
        call(&app, post_json(&format!("{BASE}/wave4_bg_job/complete"), &admin_token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "complete 必须 200：{completed}");
    assert_eq!(completed["status"], "completed", "complete 后必须是 completed：{completed}");
    assert!(completed["completed_ts"].as_i64().unwrap_or(0) > 0, "complete 后 completed_ts 必须被填：{completed}");

    // history：状态转移必须留痕（否则运维查不到"什么时候跑的、跑了几次"）
    let (status, history) = call(&app, get(&format!("{BASE}/wave4_bg_job/history"), &admin_token)).await;
    assert_eq!(status, StatusCode::OK, "history 必须 200：{history}");
    let entries = history["history"]
        .as_array()
        .or_else(|| history["entries"].as_array())
        .or_else(|| history.as_array())
        .unwrap_or_else(|| panic!("history 必须是数组或带数组字段：{history}"));
    assert!(!entries.is_empty(), "跑完的任务必须有状态转移记录：{history}");
}

/// 重复 `start` ⇒ 400（不是把 running 又"启动"一次）。
#[tokio::test]
async fn start_non_pending_update_is_bad_request() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    create_job(&app, &admin_token, "wave4_bg_twice").await;
    let start_uri = format!("{BASE}/wave4_bg_twice/start");
    let (status, first) = call(&app, post_json(&start_uri, &admin_token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "首次 start 必须 200：{first}");

    let (status, second) = call(&app, post_json(&start_uri, &admin_token, &json!({}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非 pending 再 start 必须 400：{second}");
    assert_eq!(second["errcode"], "M_BAD_JSON", "errcode：{second}");
}

/// running 的任务可以 cancel ⇒ cancelled。
#[tokio::test]
async fn cancel_running_update_sets_cancelled() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    create_job(&app, &admin_token, "wave4_bg_cancel").await;
    let (status, started) =
        call(&app, post_json(&format!("{BASE}/wave4_bg_cancel/start"), &admin_token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{started}");

    let (status, cancelled) =
        call(&app, post_json(&format!("{BASE}/wave4_bg_cancel/cancel"), &admin_token, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "cancel 必须 200：{cancelled}");
    assert_eq!(cancelled["status"], "cancelled", "cancel 后必须是 cancelled：{cancelled}");
}

/// 未知任务 ⇒ 404（不能 200 返回空对象）。
#[tokio::test]
async fn unknown_update_is_not_found() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    let (status, value) = call(&app, get(&format!("{BASE}/no_such_job"), &admin_token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "未知任务必须 404：{value}");

    // ⚠️ DELETE 未知任务返回 **204**（不是 404）：`background_update_service::delete_update`
    // 只调 `storage.delete_update`，**不查存在性**；而 `registration_tokens` 的删除会先
    // `get_token` 再删、未知即 404。两处删除语义因此不一致（审计 NEW-15）。
    // 这里断言**实际行为**：DELETE 幂等返回 204 本身符合 HTTP 语义，但它会让"打错任务名的
    // 删除"静默成功，运维会以为删掉了。改不改由裁决决定，测试先把现状钉住。
    let (status, deleted) = call(&app, delete(&format!("{BASE}/no_such_job"), &admin_token)).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "删除未知任务当前返回 204（幂等，见 NEW-15）：{deleted}");
}

/// 普通用户 ⇒ 403。
#[tokio::test]
async fn background_updates_require_admin() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let user_token = register(&app, "bgupd_plain_user").await;

    let (status, value) = call(&app, get(BASE, &user_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通用户必须 403：{value}");
    assert_eq!(value["errcode"], "M_FORBIDDEN", "errcode：{value}");
}
