// MSC4108 rendezvous —— **真实 HTTP 往返**用例（W8 / NEW-05，补 TEST-02 的最后一个盲区）。
//
// 为什么另起一个文件而不是改 `tests/unit/msc4108_rendezvous_route_tests.rs`：
// 那个文件把 handler 的 URL 拼装、`http_date_from_millis`、ETag 格式**各自抄了一份**再断言
// 自己抄的那份（`let status = StatusCode::OK; assert_eq!(status, StatusCode::OK)`），
// 20 个用例里 12 个改坏生产代码也不会红 —— 典型的「自证测试」。
// 而真正的端到端需要 `AppState`（DB 池 + ServiceContainer + 30+ 字段的 FederationContext），
// 那正是 `tests/integration/mod.rs::setup_fresh_test_app` 的职责（`tests/unit` 的测试目标是
// 同一个 crate 的另一个 target，拿不到这个夹具，硬造一份等于给第二份实现开门，违反"铁律 2"）。
//
// 因此本文件的每个用例都：**真 router（生产装配 `build_router`）→ 真请求 → 真库**，
// 断言的是线上真的会返回的状态码、头、body 与 errcode。
// 判据（实测过）：把 `create_session` 的 ETag 头删掉、把 412 的
// `org.matrix.msc4108.errcode` 改名、把 404 换成 400 —— 对应用例都会红。

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

const BASE: &str = "/_matrix/client/unstable/org.matrix.msc4108/rendezvous";

/// 真实装配的 app（生产 `build_router`，含 6 层中间件）。
async fn app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.expect("router must answer");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body must read").to_vec();
    (status, headers, body)
}

fn text_plain_request(method: &str, uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(body.to_string()))
        .expect("request must build")
}

fn header_str<'a>(headers: &'a axum::http::HeaderMap, name: axum::http::header::HeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn errcode(body: &[u8]) -> String {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| v.get("errcode").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_else(|| format!("<no errcode in body: {}>", String::from_utf8_lossy(body)))
}

/// POST → 200 + `{"url": …}` + ETag/Expires/Last-Modified/Cache-Control/Pragma。
/// 返回 `(session_id, etag)` 给后续用例用。
async fn create_session(app: &axum::Router, payload: &str) -> (String, String) {
    let (status, headers, body) = send(app, text_plain_request("POST", BASE, payload)).await;
    assert_eq!(status, StatusCode::OK, "POST 创建会话必须 200，body={}", String::from_utf8_lossy(&body));
    let json: Value = serde_json::from_slice(&body).expect("创建响应必须是 JSON");
    let url = json.get("url").and_then(|v| v.as_str()).expect("创建响应必须带 url").to_string();
    let session_id = url.rsplit('/').next().expect("url 末段必须是 session id").to_string();
    let etag = header_str(&headers, header::ETAG).expect("创建响应必须带 ETag").to_string();
    (session_id, etag)
}

#[tokio::test]
async fn create_session_returns_url_and_the_mandated_caching_headers() {
    let Some(app) = app().await else { return };
    let (status, headers, body) = send(&app, text_plain_request("POST", BASE, "initial-blob")).await;

    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_slice(&body).expect("必须是 JSON");
    let url = json.get("url").and_then(|v| v.as_str()).expect("必须带 url");
    assert!(url.contains(BASE), "url 必须包含 rendezvous 前缀，实际 {url}");

    // 头契约：ETag 必须是被引号包住的字符串（SDK 用它做 If-None-Match / If-Match）。
    let etag = header_str(&headers, header::ETAG).expect("必须带 ETag");
    assert!(etag.starts_with('"') && etag.ends_with('"'), "ETag 必须是带引号的字符串，实际 {etag}");
    assert_eq!(header_str(&headers, header::CACHE_CONTROL), Some("no-store"));
    assert_eq!(header_str(&headers, header::PRAGMA), Some("no-cache"));
    // RFC 7231 IMF-fixdate：`Tue, 15 Sep 2026 04:21:55 GMT`
    let expires = header_str(&headers, header::EXPIRES).expect("必须带 Expires");
    assert!(expires.ends_with("GMT"), "Expires 必须是 HTTP 日期，实际 {expires}");
    let last_modified = header_str(&headers, header::LAST_MODIFIED).expect("必须带 Last-Modified");
    assert!(last_modified.ends_with("GMT"), "Last-Modified 必须是 HTTP 日期，实际 {last_modified}");
}

#[tokio::test]
async fn create_session_without_content_type_is_rejected() {
    let Some(app) = app().await else { return };
    let request = Request::builder().method("POST").uri(BASE).body(Body::from("blob")).expect("request must build");
    let (status, _headers, body) = send(&app, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 Content-Type 必须 400");
    assert_eq!(errcode(&body), "M_MISSING_PARAM");
}

#[tokio::test]
async fn create_session_with_json_content_type_is_rejected() {
    let Some(app) = app().await else { return };
    let request = Request::builder()
        .method("POST")
        .uri(BASE)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .expect("request must build");
    let (status, _headers, body) = send(&app, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "非 text/plain 必须 400");
    assert_eq!(errcode(&body), "M_INVALID_PARAM");
}

#[tokio::test]
async fn create_session_payload_over_4k_is_rejected() {
    let Some(app) = app().await else { return };
    let payload = "x".repeat(4 * 1024 + 1);
    let (status, _headers, body) = send(&app, text_plain_request("POST", BASE, &payload)).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "超 4KB 必须 413");
    assert_eq!(errcode(&body), "M_TOO_LARGE");
}

#[tokio::test]
async fn get_session_returns_the_stored_payload_verbatim() {
    let Some(app) = app().await else { return };
    let (session_id, created_etag) = create_session(&app, "stored-blob").await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("{BASE}/{session_id}"))
        .body(Body::empty())
        .expect("request must build");
    let (status, headers, body) = send(&app, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8_lossy(&body), "stored-blob", "GET 必须原样返回创建时写入的 blob");
    assert_eq!(header_str(&headers, header::CONTENT_TYPE), Some("text/plain"));
    assert_eq!(header_str(&headers, header::ETAG), Some(created_etag.as_str()), "未修改时 ETag 必须与创建时一致");
}

#[tokio::test]
async fn get_session_with_matching_if_none_match_returns_304_without_body() {
    let Some(app) = app().await else { return };
    let (session_id, etag) = create_session(&app, "polled-blob").await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("{BASE}/{session_id}"))
        .header(header::IF_NONE_MATCH, etag.clone())
        .body(Body::empty())
        .expect("request must build");
    let (status, headers, body) = send(&app, request).await;

    assert_eq!(status, StatusCode::NOT_MODIFIED, "If-None-Match 命中必须 304");
    assert!(body.is_empty(), "304 不得带 body");
    assert_eq!(header_str(&headers, header::CACHE_CONTROL), Some("no-store"), "304 也必须带公共缓存头");
}

#[tokio::test]
async fn get_unknown_session_returns_404() {
    let Some(app) = app().await else { return };
    let request = Request::builder()
        .method("GET")
        .uri(format!("{BASE}/does-not-exist-{}", uuid::Uuid::new_v4()))
        .body(Body::empty())
        .expect("request must build");
    let (status, _headers, body) = send(&app, request).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(errcode(&body), "M_NOT_FOUND");
}

#[tokio::test]
async fn update_session_returns_202_with_a_new_etag_and_persists_the_new_blob() {
    let Some(app) = app().await else { return };
    let (session_id, old_etag) = create_session(&app, "v1").await;

    let (status, headers, body) = send(&app, text_plain_request("PUT", &format!("{BASE}/{session_id}"), "v2")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "PUT 成功必须 202，body={}", String::from_utf8_lossy(&body));
    let new_etag = header_str(&headers, header::ETAG).expect("202 必须带新 ETag").to_string();
    assert_ne!(new_etag, old_etag, "更新后 ETag 必须变化");

    let request = Request::builder()
        .method("GET")
        .uri(format!("{BASE}/{session_id}"))
        .body(Body::empty())
        .expect("request must build");
    let (status, _headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8_lossy(&body), "v2", "GET 必须读到更新后的 blob");
}

#[tokio::test]
async fn update_session_with_stale_if_match_returns_412_concurrent_write() {
    let Some(app) = app().await else { return };
    let (session_id, _etag) = create_session(&app, "v1").await;

    let request = Request::builder()
        .method("PUT")
        .uri(format!("{BASE}/{session_id}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .header(header::IF_MATCH, "\"1700000000000-stale\"")
        .body(Body::from("v2"))
        .expect("request must build");
    let (status, _headers, body) = send(&app, request).await;

    assert_eq!(status, StatusCode::PRECONDITION_FAILED, "If-Match 不匹配必须 412");
    let json: Value = serde_json::from_slice(&body).expect("412 必须是 JSON");
    assert_eq!(
        json.get("org.matrix.msc4108.errcode").and_then(|v| v.as_str()),
        Some("M_CONCURRENT_WRITE"),
        "412 必须带 MSC4108 的不稳定前缀 errcode，实际 {json}"
    );
}

#[tokio::test]
async fn delete_session_returns_204_and_the_session_disappears() {
    let Some(app) = app().await else { return };
    let (session_id, _etag) = create_session(&app, "to-be-deleted").await;

    let request = Request::builder()
        .method("DELETE")
        .uri(format!("{BASE}/{session_id}"))
        .body(Body::empty())
        .expect("request must build");
    let (status, headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "DELETE 成功必须 204");
    assert!(body.is_empty(), "204 不得带 body");
    assert_eq!(header_str(&headers, header::CACHE_CONTROL), Some("no-store"));

    let request = Request::builder()
        .method("GET")
        .uri(format!("{BASE}/{session_id}"))
        .body(Body::empty())
        .expect("request must build");
    let (status, _headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "已删除的会话必须 404");
    assert_eq!(errcode(&body), "M_NOT_FOUND");
}

#[tokio::test]
async fn delete_unknown_session_returns_404() {
    let Some(app) = app().await else { return };
    let request = Request::builder()
        .method("DELETE")
        .uri(format!("{BASE}/never-existed-{}", uuid::Uuid::new_v4()))
        .body(Body::empty())
        .expect("request must build");
    let (status, _headers, body) = send(&app, request).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(errcode(&body), "M_NOT_FOUND");
}
