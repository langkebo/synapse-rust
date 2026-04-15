use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
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
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": name,
                "preset": "private_chat"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn set_typing(app: &axum::Router, token: &str, room_id: &str, user_id: &str) -> StatusCode {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/typing/{}",
            room_id, user_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "typing": true,
                "timeout": 30000
            })
            .to_string(),
        ))
        .unwrap();

    ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn test_typing_read_routes_reject_non_members() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (owner_token, owner_user_id) = register_user(&app, "typing_guard_owner_reads").await;
    let (guest_token, _) = register_user(&app, "typing_guard_guest_reads").await;
    let room_id = create_room(&app, &owner_token, "Typing Guard Reads").await;

    assert_eq!(set_typing(&app, &owner_token, &room_id, &owner_user_id).await, StatusCode::OK);

    let room_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/typing", room_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let room_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), room_request)
        .await
        .unwrap();
    assert_eq!(room_response.status(), StatusCode::FORBIDDEN);

    let user_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/typing/{}",
            room_id, owner_user_id
        ))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let user_response = ServiceExt::<Request<Body>>::oneshot(app, user_request)
        .await
        .unwrap();
    assert_eq!(user_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_typing_write_and_bulk_routes_require_room_access() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (owner_token, owner_user_id) = register_user(&app, "typing_guard_owner_write").await;
    let (guest_token, guest_user_id) = register_user(&app, "typing_guard_guest_write").await;
    let room_id = create_room(&app, &owner_token, "Typing Guard Writes").await;

    assert_eq!(set_typing(&app, &owner_token, &room_id, &owner_user_id).await, StatusCode::OK);
    assert_eq!(
        set_typing(&app, &guest_token, &room_id, &guest_user_id).await,
        StatusCode::FORBIDDEN
    );

    let bulk_guest_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/rooms/typing")
        .header("Authorization", format!("Bearer {}", guest_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "rooms": [room_id.clone()]
            })
            .to_string(),
        ))
        .unwrap();
    let bulk_guest_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), bulk_guest_request)
        .await
        .unwrap();
    assert_eq!(bulk_guest_response.status(), StatusCode::FORBIDDEN);

    let bulk_owner_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/rooms/typing")
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "rooms": [room_id.clone()]
            })
            .to_string(),
        ))
        .unwrap();
    let bulk_owner_response = ServiceExt::<Request<Body>>::oneshot(app, bulk_owner_request)
        .await
        .unwrap();
    assert_eq!(bulk_owner_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(bulk_owner_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_entry = json.get(&room_id).and_then(|value| value.get("typing"));
    assert!(room_entry.is_some());
}
