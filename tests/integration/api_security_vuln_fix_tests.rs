//! P0/P1 安全漏洞回归测试
//!
//! 对应文档：docs/路由安全漏洞修复清单-2026-08-06.md
//!
//! P0 — media r1 下载未授权访问（VULN-01 / VULN-02）
//! P1 — webhook 端点认证顺序错误（VULN-03 / VULN-04 / VULN-05）
//!
//! 这些测试断言修复后的行为：无凭证请求应返回 401 Unauthorized。
//! 修复前会失败（media 返回 200，webhook 返回 422），修复后通过。
//! 遵循 TDD red-green：先写失败测试，再实现修复使其通过。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

// --------------------------------------------------------------------------- //
// 测试辅助
// --------------------------------------------------------------------------- //

async fn setup_app() -> Option<axum::Router> {
    // 项目规范：使用 setup_fresh_test_app() 而非 setup_test_app() 以获得隔离 schema
    super::setup_fresh_test_app().await
}

/// 注册一个测试用户，返回 (access_token, user_id)。
async fn register_user(app: &axum::Router, username: &str) -> Option<(String, String)> {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .ok()?;

    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.ok()?;

    if response.status() != StatusCode::OK {
        return None;
    }

    let body = axum::body::to_bytes(response.into_body(), 4096).await.ok()?;
    let json: Value = serde_json::from_slice(&body).ok()?;
    Some((json.get("access_token")?.as_str()?.to_string(), json.get("user_id")?.as_str()?.to_string()))
}

/// 读取响应 body 为 serde_json::Value。
async fn read_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap_or_default();
    serde_json::from_slice(&body).unwrap_or(json!({}))
}

// --------------------------------------------------------------------------- //
// P0 — VULN-01: GET /_matrix/media/r1/download/{server_name}/{media_id} 无认证
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn vuln_01_media_r1_download_without_auth_returns_401() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过 VULN-01");
        return;
    };

    // 不存在的 media_id — 修复前返回 200 + {}，修复后应返回 401（无 token）
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/media/r1/download/matrix.test/nonexistent_media_id_12345")
        .body(Body::empty())
        .unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = read_json(response).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "VULN-01: media r1 download 无 Authorization 应返回 401，实际返回 {} | body: {}",
        status,
        body
    );
}

#[tokio::test]
async fn vuln_01_media_r1_download_with_auth_still_works() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过 VULN-01 正向测试");
        return;
    };

    let username = format!("vuln01_auth_{}", rand::random::<u32>());
    let Some((token, _user_id)) = register_user(&app, &username).await else {
        eprintln!("[skip] 用户注册失败，跳过 VULN-01 正向测试");
        return;
    };

    // 带 token 请求不存在的 media — 修复后应返回 404（资源不存在），而非 401
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/media/r1/download/matrix.test/nonexistent_media_id_12345")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();

    // 认证通过后，资源不存在应返回 404（或 404 M_NOT_FOUND）。
    // 关键：不应返回 401（认证应通过）也不应返回 200 + {}（旧行为）。
    assert_ne!(status, StatusCode::UNAUTHORIZED, "VULN-01 正向: 带 token 的请求不应返回 401，实际返回 {}", status);
    assert_ne!(status, StatusCode::OK, "VULN-01 正向: 不存在的媒体不应返回 200（旧行为），实际返回 {}", status);
}

// --------------------------------------------------------------------------- //
// P0 — VULN-02: GET /_matrix/media/r1/download/{server_name}/{media_id}/{filename} 无认证
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn vuln_02_media_r1_download_with_filename_without_auth_returns_401() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过 VULN-02");
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/media/r1/download/matrix.test/nonexistent_media_id_12345/test.png")
        .body(Body::empty())
        .unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = read_json(response).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "VULN-02: media r1 download (带文件名) 无 Authorization 应返回 401，实际返回 {} | body: {}",
        status,
        body
    );
}

// --------------------------------------------------------------------------- //
// P1 — VULN-04: POST /_synapse/external/trendradar/{service_id}/webhook 认证顺序
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn vuln_04_trendradar_webhook_without_auth_returns_401() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过 VULN-04");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/external/trendradar/test_service_id/webhook")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = read_json(response).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "VULN-04: trendradar webhook 无认证头应返回 401，实际返回 {} | body: {}",
        status,
        body
    );
}

// --------------------------------------------------------------------------- //
// P1 — VULN-05: POST /_synapse/external/webhook/{service_id} 认证顺序
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn vuln_05_generic_webhook_without_auth_returns_401() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过 VULN-05");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/external/webhook/test_service_id")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = read_json(response).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "VULN-05: generic webhook 无认证头应返回 401，实际返回 {} | body: {}",
        status,
        body
    );
}

// --------------------------------------------------------------------------- //
// P1 — webhook 认证正向测试：带有效 token 的请求不应被 401 拦截
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn vuln_04_05_webhook_with_auth_not_blocked_by_401() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过 webhook 正向测试");
        return;
    };

    let username = format!("vuln_webhook_{}", rand::random::<u32>());
    let Some((token, _user_id)) = register_user(&app, &username).await else {
        eprintln!("[skip] 用户注册失败，跳过 webhook 正向测试");
        return;
    };

    // 带 Bearer token 的 webhook 请求 — 不应返回 401。
    // 可能返回 422（body 不完整）、403（权限不足）、200 等，但不应是 401。
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/external/webhook/test_service_id")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();

    assert_ne!(status, StatusCode::UNAUTHORIZED, "webhook 正向: 带 token 的请求不应返回 401，实际返回 {}", status);
}

// --------------------------------------------------------------------------- //
// 回归保护：确保修复不破坏已注册路由的正常认证行为
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn regression_protected_route_still_requires_auth() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过回归测试");
        return;
    };

    // /_matrix/client/v3/account/whoami 是标准受保护路由，无 token 应返回 401。
    // 此测试确保 media/webhook 的认证修复未意外破坏全局认证中间件。
    let request =
        Request::builder().method("GET").uri("/_matrix/client/v3/account/whoami").body(Body::empty()).unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "回归: whoami 无 token 应返回 401，实际返回 {}",
        response.status()
    );
}

#[tokio::test]
async fn regression_public_route_still_accessible() {
    let Some(app) = setup_app().await else {
        eprintln!("[skip] 测试数据库不可用，跳过公开路由回归测试");
        return;
    };

    // /_matrix/client/versions 是公开路由，无需认证。
    // 此测试确保 media 认证修复未意外将公开路由也加入认证要求。
    let request = Request::builder().method("GET").uri("/_matrix/client/versions").body(Body::empty()).unwrap();

    let request = super::with_local_connect_info(request);
    let response = app.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK, "回归: /versions 公开路由应返回 200，实际返回 {}", response.status());
}
