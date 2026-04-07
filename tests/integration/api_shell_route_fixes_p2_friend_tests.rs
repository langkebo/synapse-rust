// Integration tests for P2 shell route fixes - Friend management
// Tests verify friend management operations return confirmation data

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String, String) {
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
        json["device_id"].as_str().unwrap().to_string(),
    )
}

async fn send_friend_request(app: &axum::Router, access_token: &str, target_user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/friends/request")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", access_token))
        .body(Body::from(
            json!({
                "user_id": target_user_id
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

async fn accept_friend_request(app: &axum::Router, access_token: &str, requester_user_id: &str) {
    let encoded_user_id = urlencoding::encode(requester_user_id);
    let request = Request::builder()
        .method("POST")
        .uri(&format!(
            "/_matrix/client/v1/friends/request/{}/accept",
            encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_update_friend_note_returns_confirmation() {
    let app = match setup_test_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let (alice_token, alice_id, _) = register_user(&app, "alice").await;
    let (bob_token, bob_id, _) = register_user(&app, "bob").await;

    // Send and accept friend request
    send_friend_request(&app, &alice_token, &bob_id).await;
    accept_friend_request(&app, &bob_token, &alice_id).await;

    // Update friend note
    let encoded_bob_id = urlencoding::encode(&bob_id);
    let request = Request::builder()
        .method("PUT")
        .uri(&format!(
            "/_matrix/client/v1/friends/{}/note",
            encoded_bob_id
        ))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::from(
            json!({
                "note": "Best friend"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to update friend note");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("user_id").is_some(),
        "Response should contain user_id"
    );
    assert_eq!(body["user_id"], bob_id);
    assert!(body.get("note").is_some(), "Response should contain note");
    assert_eq!(body["note"], "Best friend");
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}

#[tokio::test]
async fn test_update_friend_status_returns_confirmation() {
    let app = match setup_test_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let (alice_token, alice_id, _) = register_user(&app, "alice2").await;
    let (bob_token, bob_id, _) = register_user(&app, "bob2").await;

    // Send and accept friend request
    send_friend_request(&app, &alice_token, &bob_id).await;
    accept_friend_request(&app, &bob_token, &alice_id).await;

    // Update friend status
    let encoded_bob_id = urlencoding::encode(&bob_id);
    let request = Request::builder()
        .method("PUT")
        .uri(&format!(
            "/_matrix/client/v1/friends/{}/status",
            encoded_bob_id
        ))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::from(
            json!({
                "status": "favorite"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to update friend status");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("user_id").is_some(),
        "Response should contain user_id"
    );
    assert_eq!(body["user_id"], bob_id);
    assert!(
        body.get("status").is_some(),
        "Response should contain status"
    );
    assert_eq!(body["status"], "favorite");
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}

#[tokio::test]
async fn test_update_friend_displayname_returns_confirmation() {
    let app = match setup_test_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let (alice_token, alice_id, _) = register_user(&app, "alice3").await;
    let (bob_token, bob_id, _) = register_user(&app, "bob3").await;

    // Send and accept friend request
    send_friend_request(&app, &alice_token, &bob_id).await;
    accept_friend_request(&app, &bob_token, &alice_id).await;

    // Update friend displayname
    let encoded_bob_id = urlencoding::encode(&bob_id);
    let request = Request::builder()
        .method("PUT")
        .uri(&format!(
            "/_matrix/client/v1/friends/{}/displayname",
            encoded_bob_id
        ))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::from(
            json!({
                "displayname": "Bobby"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Failed to update friend displayname");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("Failed to read response body");
    let body: Value = serde_json::from_slice(&body).expect("Failed to parse response");

    // Verify response contains confirmation data
    assert!(
        body.get("user_id").is_some(),
        "Response should contain user_id"
    );
    assert_eq!(body["user_id"], bob_id);
    assert!(
        body.get("displayname").is_some(),
        "Response should contain displayname"
    );
    assert_eq!(body["displayname"], "Bobby");
    assert!(
        body.get("updated_ts").is_some(),
        "Response should contain updated_ts"
    );
    assert!(
        body["updated_ts"].is_number(),
        "updated_ts should be a number"
    );
}
