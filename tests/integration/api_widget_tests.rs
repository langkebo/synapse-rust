use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn register_user(app: &axum::Router, username: &str) -> String {
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
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Widget Room"}).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn read_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn create_widget(app: &axum::Router, token: &str, room_id: &str) -> String {
    let create_widget_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/widgets")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "widget_type": "m.custom",
                "url": "https://example.com/widget",
                "name": "Test Widget",
                "data": { "source": "integration-test" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_widget_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = read_json(response).await;
    json["widget"]["widget_id"].as_str().unwrap().to_string()
}

async fn create_widget_session(app: &axum::Router, token: &str, widget_id: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/widgets/{}/sessions", widget_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": "DEVICE123",
                "expires_in_ms": 60000
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = read_json(response).await;
    json["session"]["session_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_create_widget_succeeds_for_existing_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(&app, &format!("widget_user_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &token).await;

    let create_widget_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/widgets")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "widget_type": "m.custom",
                "url": "https://example.com/widget",
                "name": "Test Widget",
                "data": { "source": "integration-test" }
            })
            .to_string(),
        ))
        .unwrap();

    let create_widget_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_widget_request)
            .await
            .unwrap();
    assert_eq!(create_widget_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_widget_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["widget"]["room_id"], room_id);
    assert_eq!(json["widget"]["name"], "Test Widget");
}

#[tokio::test]
async fn test_create_widget_returns_not_found_for_missing_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(&app, &format!("widget_missing_{}", rand::random::<u32>())).await;

    let create_widget_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/widgets")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": "!missing:localhost",
                "widget_type": "m.custom",
                "url": "https://example.com/widget",
                "name": "Broken Widget"
            })
            .to_string(),
        ))
        .unwrap();

    let create_widget_response = ServiceExt::<Request<Body>>::oneshot(app, create_widget_request)
        .await
        .unwrap();
    assert_eq!(create_widget_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_widget_requires_authentication() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(&app, &format!("widget_auth_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &token).await;
    let widget_id = create_widget(&app, &token, &room_id).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/widgets/{}", widget_id))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_widget_forbidden_for_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let owner_token = register_user(&app, &format!("widget_owner_{}", rand::random::<u32>())).await;
    let viewer_token =
        register_user(&app, &format!("widget_viewer_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &owner_token).await;
    let widget_id = create_widget(&app, &owner_token, &room_id).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/widgets/{}", widget_id))
        .header("Authorization", format!("Bearer {}", viewer_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let json = read_json(response).await;
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_create_widget_session_uses_path_widget_id() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(&app, &format!("widget_session_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &token).await;
    let widget_id = create_widget(&app, &token, &room_id).await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/widgets/{}/sessions", widget_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": "DEVICE123",
                "expires_in_ms": 60000
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = read_json(response).await;
    assert_eq!(json["session"]["widget_id"], widget_id);
}

#[tokio::test]
async fn test_create_widget_session_rejects_mismatched_body_widget_id() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(&app, &format!("widget_mismatch_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &token).await;
    let widget_id = create_widget(&app, &token, &room_id).await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/widgets/{}/sessions", widget_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "widget_id": "widget_other",
                "device_id": "DEVICE123"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = read_json(response).await;
    assert_eq!(json["errcode"], "M_BAD_JSON");
}

#[tokio::test]
async fn test_get_room_widget_capabilities_forbidden_for_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let owner_token = register_user(&app, &format!("widget_caps_owner_{}", rand::random::<u32>()))
        .await;
    let outsider_token =
        register_user(&app, &format!("widget_caps_outsider_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &owner_token).await;
    let widget_id = create_widget(&app, &owner_token, &room_id).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/widgets/{}/capabilities",
            room_id, widget_id
        ))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let json = read_json(response).await;
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_get_room_widget_capabilities_rejects_widget_room_mismatch() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let token = register_user(
        &app,
        &format!("widget_caps_mismatch_{}", rand::random::<u32>()),
    )
    .await;
    let room_id = create_room(&app, &token).await;
    let other_room_id = create_room(&app, &token).await;
    let widget_id = create_widget(&app, &token, &room_id).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/widgets/{}/capabilities",
            other_room_id, widget_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = read_json(response).await;
    assert_eq!(json["errcode"], "M_BAD_JSON");
}

#[tokio::test]
async fn test_get_widget_session_forbidden_for_unrelated_user() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let owner_token =
        register_user(&app, &format!("widget_session_owner_{}", rand::random::<u32>())).await;
    let outsider_token = register_user(
        &app,
        &format!("widget_session_outsider_{}", rand::random::<u32>()),
    )
    .await;
    let room_id = create_room(&app, &owner_token).await;
    let widget_id = create_widget(&app, &owner_token, &room_id).await;
    let session_id = create_widget_session(&app, &owner_token, &widget_id).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/widgets/sessions/{}", session_id))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let json = read_json(response).await;
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}
