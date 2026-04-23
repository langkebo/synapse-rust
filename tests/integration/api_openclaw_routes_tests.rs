use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::{routes::create_router, AppState};
use tower::ServiceExt;

const OPENCLAW_UNSTABLE_PREFIX: &str = "/_matrix/client/unstable/org.synapse_rust.openclaw";

async fn setup_test_app(openclaw_enabled: bool) -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.experimental.openclaw_routes_enabled = openclaw_enabled;

    let cache = container.cache.clone();
    let state = AppState::new(container, cache);

    Some(create_router(state))
}

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

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .ok()?;

    if response.status() != StatusCode::OK {
        return None;
    }

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .ok()?;
    let json: Value = serde_json::from_slice(&body).ok()?;

    Some((
        json.get("access_token")?.as_str()?.to_string(),
        json.get("user_id")?.as_str()?.to_string(),
    ))
}

async fn register_guest_user(app: &axum::Router) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register/guest")
        .body(Body::empty())
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    assert_eq!(
        status,
        StatusCode::OK,
        "guest registration failed with status {}: {}",
        status,
        String::from_utf8_lossy(&body)
    );

    let json: Value = serde_json::from_slice(&body).unwrap();
    let token = json["access_token"].as_str().unwrap().to_string();
    let user_id = json["user_id"].as_str().unwrap().to_string();

    (token, user_id)
}

async fn get_json_response(
    app: &axum::Router,
    uri: &str,
    access_token: &str,
) -> (StatusCode, Value) {
    json_response(app, "GET", uri, access_token, None).await
}

async fn json_response(
    app: &axum::Router,
    method: &str,
    uri: &str,
    access_token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body(match body {
            Some(body) => Body::from(body.to_string()),
            None => Body::empty(),
        })
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    if body.is_empty() {
        return (status, Value::Null);
    }

    let json = serde_json::from_slice(&body).unwrap_or_else(|_| {
        panic!(
            "expected JSON response from {} with status {}, body: {}",
            uri,
            status,
            String::from_utf8_lossy(&body)
        )
    });

    (status, json)
}

#[tokio::test]
async fn test_openclaw_routes_are_not_mounted_by_default() {
    let Some(app) = setup_test_app(false).await else {
        return;
    };

    let user_token = super::create_test_user(&app).await;
    let (status, _) = get_json_response(
        &app,
        &format!("{}/connections", OPENCLAW_UNSTABLE_PREFIX),
        &user_token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_openclaw_routes_mount_only_under_unstable_prefix_when_enabled() {
    let Some(app) = setup_test_app(true).await else {
        return;
    };

    let user_token = super::create_test_user(&app).await;

    let (stable_status, _) =
        get_json_response(&app, "/_matrix/client/v3/openclaw/connections", &user_token).await;
    assert_eq!(stable_status, StatusCode::NOT_FOUND);

    let (unstable_status, unstable_json) = get_json_response(
        &app,
        &format!("{}/connections", OPENCLAW_UNSTABLE_PREFIX),
        &user_token,
    )
    .await;

    assert_eq!(unstable_status, StatusCode::OK);
    assert_eq!(unstable_json, serde_json::json!([]));
}

#[tokio::test]
async fn test_openclaw_routes_reject_guest_users() {
    let Some(app) = setup_test_app(true).await else {
        return;
    };

    let (guest_token, guest_user_id) = register_guest_user(&app).await;

    let (guest_info_status, guest_info_json) =
        get_json_response(&app, "/_matrix/client/v3/account/guest", &guest_token).await;
    assert_eq!(guest_info_status, StatusCode::OK);
    assert_eq!(guest_info_json["user_id"], guest_user_id);
    assert_eq!(guest_info_json["is_guest"], true);

    let (status, json) = get_json_response(
        &app,
        &format!("{}/connections", OPENCLAW_UNSTABLE_PREFIX),
        &guest_token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["errcode"], "M_FORBIDDEN");
    assert_eq!(json["error"], "Guest access to OpenClaw routes is disabled");
}

#[tokio::test]
async fn test_openclaw_create_connection_rejects_localhost_base_url() {
    let Some(app) = setup_test_app(true).await else {
        return;
    };

    let user_token = super::create_test_user(&app).await;
    let (status, body) = json_response(
        &app,
        "POST",
        &format!("{}/connections", OPENCLAW_UNSTABLE_PREFIX),
        &user_token,
        Some(json!({
            "name": "local",
            "provider": "openai",
            "base_url": "http://localhost:8080",
            "is_default": true
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["errcode"], "M_BAD_JSON");
    assert_eq!(body["error"], "OpenClaw base_url cannot target localhost");
}

#[tokio::test]
async fn test_openclaw_update_connection_rejects_private_ip_base_url() {
    let Some(app) = setup_test_app(true).await else {
        return;
    };

    let user_token = super::create_test_user(&app).await;
    let (create_status, create_body) = json_response(
        &app,
        "POST",
        &format!("{}/connections", OPENCLAW_UNSTABLE_PREFIX),
        &user_token,
        Some(json!({
            "name": "public",
            "provider": "openai",
            "base_url": "https://example.com",
            "is_default": true
        })),
    )
    .await;

    assert_eq!(create_status, StatusCode::OK);
    let connection_id = create_body["id"].as_i64().unwrap();

    let (update_status, update_body) = json_response(
        &app,
        "PUT",
        &format!("{}/connections/{}", OPENCLAW_UNSTABLE_PREFIX, connection_id),
        &user_token,
        Some(json!({
            "base_url": "http://10.0.0.5:11434"
        })),
    )
    .await;

    assert_eq!(update_status, StatusCode::BAD_REQUEST);
    assert_eq!(update_body["errcode"], "M_BAD_JSON");
    assert_eq!(
        update_body["error"],
        "OpenClaw base_url cannot target local or private IP ranges"
    );
}

#[tokio::test]
async fn test_openclaw_private_connection_returns_not_found_to_other_users() {
    let Some(app) = setup_test_app(true).await else {
        return;
    };

    let (owner_token, _) =
        register_user(&app, &format!("openclaw_owner_{}", rand::random::<u32>()))
            .await
            .unwrap();
    let (other_token, _) =
        register_user(&app, &format!("openclaw_other_{}", rand::random::<u32>()))
            .await
            .unwrap();

    let (create_status, create_body) = json_response(
        &app,
        "POST",
        &format!("{}/connections", OPENCLAW_UNSTABLE_PREFIX),
        &owner_token,
        Some(json!({
            "name": "owner-only",
            "provider": "openai",
            "base_url": "https://example.com",
            "is_default": true
        })),
    )
    .await;

    assert_eq!(create_status, StatusCode::OK);
    let connection_id = create_body["id"].as_i64().unwrap();

    let (read_status, read_body) = json_response(
        &app,
        "GET",
        &format!("{}/connections/{}", OPENCLAW_UNSTABLE_PREFIX, connection_id),
        &other_token,
        None,
    )
    .await;

    assert_eq!(read_status, StatusCode::NOT_FOUND);
    assert_eq!(read_body["errcode"], "M_NOT_FOUND");
    assert_eq!(read_body["error"], "Connection not found");
}

#[tokio::test]
async fn test_openclaw_private_role_is_hidden_but_public_role_remains_readable() {
    let Some(app) = setup_test_app(true).await else {
        return;
    };

    let (owner_token, _) = register_user(
        &app,
        &format!("openclaw_role_owner_{}", rand::random::<u32>()),
    )
    .await
    .unwrap();
    let (other_token, _) = register_user(
        &app,
        &format!("openclaw_role_other_{}", rand::random::<u32>()),
    )
    .await
    .unwrap();

    let (private_status, private_body) = json_response(
        &app,
        "POST",
        &format!("{}/roles", OPENCLAW_UNSTABLE_PREFIX),
        &owner_token,
        Some(json!({
            "name": "private-role",
            "system_message": "private system prompt",
            "is_public": false
        })),
    )
    .await;
    assert_eq!(private_status, StatusCode::OK);
    let private_role_id = private_body["id"].as_i64().unwrap();

    let (hidden_status, hidden_body) = json_response(
        &app,
        "GET",
        &format!("{}/roles/{}", OPENCLAW_UNSTABLE_PREFIX, private_role_id),
        &other_token,
        None,
    )
    .await;
    assert_eq!(hidden_status, StatusCode::NOT_FOUND);
    assert_eq!(hidden_body["errcode"], "M_NOT_FOUND");
    assert_eq!(hidden_body["error"], "Chat role not found");

    let (public_status, public_body) = json_response(
        &app,
        "POST",
        &format!("{}/roles", OPENCLAW_UNSTABLE_PREFIX),
        &owner_token,
        Some(json!({
            "name": "public-role",
            "system_message": "public system prompt",
            "is_public": true
        })),
    )
    .await;
    assert_eq!(public_status, StatusCode::OK);
    let public_role_id = public_body["id"].as_i64().unwrap();

    let (read_status, read_body) = json_response(
        &app,
        "GET",
        &format!("{}/roles/{}", OPENCLAW_UNSTABLE_PREFIX, public_role_id),
        &other_token,
        None,
    )
    .await;
    assert_eq!(read_status, StatusCode::OK);
    assert_eq!(read_body["id"].as_i64(), Some(public_role_id));
    assert_eq!(read_body["name"], "public-role");
    assert_eq!(read_body["is_public"], true);
}
