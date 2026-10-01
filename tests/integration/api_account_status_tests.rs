//! MSC3720 account-status endpoint tests.
//!
//! Covers the HTTP layer only; the domain logic (local/remote lookup, remote
//! response validation) is unit-tested in
//! `synapse_services::account_status_service`.
//!
//! Feature gate: the endpoints are always registered but fail closed with 403
//! `M_FORBIDDEN` unless `experimental.msc3720_enabled` is set — the same shape
//! as the MSC4452 preview-url gate.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

/// Cache the MSC3720-enabled app: building a fresh test app + schema takes
/// ~35s, and this suite has several enabled-path assertions. One build keeps
/// the suite inside the integration timeout instead of contending with itself.
static ENABLED_APP: tokio::sync::OnceCell<Option<(axum::Router, synapse_web::routes::state::AppState)>> =
    tokio::sync::OnceCell::const_new();

async fn enabled_app() -> Option<(axum::Router, synapse_web::routes::state::AppState)> {
    ENABLED_APP
        .get_or_init(|| async {
            super::setup_test_app_with_config(|container| {
                super::config_mut(container).experimental.msc3720_enabled = true;
            })
            .await
        })
        .await
        .clone()
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
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
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

fn account_status_request(token: Option<&str>, body: &Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/_matrix/client/unstable/org.matrix.msc3720/account_status")
        .header("Content-Type", "application/json");
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Default config has `msc3720_enabled = false` -> the endpoint must fail
/// closed with 403 `M_FORBIDDEN` even for an authenticated user.
#[tokio::test]
async fn account_status_returns_403_when_disabled_by_default() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("acct_status_off_{}", rand::random::<u32>())).await;

    let response =
        ServiceExt::<Request<Body>>::oneshot(app, account_status_request(Some(&token), &json!({ "user_ids": [] })))
            .await
            .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(response).await["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn account_status_requires_authentication() {
    let Some((app, _state)) = enabled_app().await else {
        return;
    };

    let response = ServiceExt::<Request<Body>>::oneshot(
        app,
        account_status_request(None, &json!({ "user_ids": ["@someone:example.com"] })),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn account_status_reports_existing_and_missing_local_users() {
    let Some((app, _state)) = enabled_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, &format!("acct_status_on_{}", rand::random::<u32>())).await;
    let domain = user_id.rsplit_once(':').expect("registered user id has a domain").1;
    let ghost = format!("@acct_status_ghost_{}:{}", rand::random::<u32>(), domain);

    let response = ServiceExt::<Request<Body>>::oneshot(
        app,
        account_status_request(Some(&token), &json!({ "user_ids": [user_id, ghost] })),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["account_statuses"][&user_id]["exists"], true);
    assert_eq!(body["account_statuses"][&user_id]["deactivated"], false);
    assert_eq!(body["account_statuses"][&ghost]["exists"], false);
    assert!(
        body["account_statuses"][&ghost].get("deactivated").is_none(),
        "deactivated must be omitted when the account does not exist"
    );
    assert_eq!(body["failures"], json!([]));
}

#[tokio::test]
async fn account_status_empty_user_ids_is_an_empty_object() {
    let Some((app, _state)) = enabled_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("acct_status_empty_{}", rand::random::<u32>())).await;

    let response =
        ServiceExt::<Request<Body>>::oneshot(app, account_status_request(Some(&token), &json!({ "user_ids": [] })))
            .await
            .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, json!({}));
}

#[tokio::test]
async fn account_status_missing_user_ids_is_m_missing_param() {
    let Some((app, _state)) = enabled_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("acct_status_missing_{}", rand::random::<u32>())).await;

    let response =
        ServiceExt::<Request<Body>>::oneshot(app, account_status_request(Some(&token), &json!({}))).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["errcode"], "M_MISSING_PARAM");
}

#[tokio::test]
async fn account_status_malformed_user_id_is_m_invalid_param() {
    let Some((app, _state)) = enabled_app().await else {
        return;
    };
    let (token, _) = register_user(&app, &format!("acct_status_badid_{}", rand::random::<u32>())).await;

    let response = ServiceExt::<Request<Body>>::oneshot(
        app,
        account_status_request(Some(&token), &json!({ "user_ids": ["not-a-matrix-id"] })),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["errcode"], "M_INVALID_PARAM");
}
