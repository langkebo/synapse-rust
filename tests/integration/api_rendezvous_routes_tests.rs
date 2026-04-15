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

async fn create_rendezvous_session(app: &axum::Router) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/rendezvous")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "intent": "login.start",
                "transport": "http.v1"
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
    (
        json["session_id"].as_str().unwrap().to_string(),
        json["key"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_rendezvous_session_requires_session_key_before_binding() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (session_id, session_key) = create_rendezvous_session(&app).await;

    let unauthorized_get = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/rendezvous/{}", session_id))
        .body(Body::empty())
        .unwrap();
    let unauthorized_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthorized_get)
        .await
        .unwrap();
    assert_eq!(unauthorized_get_response.status(), StatusCode::UNAUTHORIZED);

    let authorized_get = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/rendezvous/{}", session_id))
        .header("X-Matrix-Rendezvous-Key", &session_key)
        .body(Body::empty())
        .unwrap();
    let authorized_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), authorized_get)
        .await
        .unwrap();
    assert_eq!(authorized_get_response.status(), StatusCode::OK);

    let unauthorized_send = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v1/rendezvous/{}/messages",
            session_id
        ))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "type": "m.login.progress",
                "content": { "stage": "waiting" }
            })
            .to_string(),
        ))
        .unwrap();
    let unauthorized_send_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), unauthorized_send)
            .await
            .unwrap();
    assert_eq!(unauthorized_send_response.status(), StatusCode::UNAUTHORIZED);

    let authorized_send = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v1/rendezvous/{}/messages",
            session_id
        ))
        .header("X-Matrix-Rendezvous-Key", &session_key)
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "type": "m.login.progress",
                "content": { "stage": "waiting" }
            })
            .to_string(),
        ))
        .unwrap();
    let authorized_send_response = ServiceExt::<Request<Body>>::oneshot(app, authorized_send)
        .await
        .unwrap();
    assert_eq!(authorized_send_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_rendezvous_bound_user_can_access_without_key_but_other_user_cannot() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (session_id, session_key) = create_rendezvous_session(&app).await;
    let owner_token = register_user(&app, "rendezvous_bound_owner").await;
    let guest_token = register_user(&app, "rendezvous_bound_guest").await;

    let connect_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v1/rendezvous/{}", session_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("X-Matrix-Rendezvous-Key", &session_key)
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "status": "connected" }).to_string()))
        .unwrap();
    let connect_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), connect_request)
        .await
        .unwrap();
    assert_eq!(connect_response.status(), StatusCode::OK);

    let owner_get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/rendezvous/{}", session_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let owner_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), owner_get_request)
        .await
        .unwrap();
    assert_eq!(owner_get_response.status(), StatusCode::OK);

    let guest_get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/rendezvous/{}", session_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let guest_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), guest_get_request)
        .await
        .unwrap();
    assert_eq!(guest_get_response.status(), StatusCode::FORBIDDEN);

    let complete_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v1/rendezvous/{}", session_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "status": "completed" }).to_string()))
        .unwrap();
    let complete_response = ServiceExt::<Request<Body>>::oneshot(app, complete_request)
        .await
        .unwrap();
    assert_eq!(complete_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(complete_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "completed");
    assert!(json["login_finish"]["access_token"].is_string());
}
