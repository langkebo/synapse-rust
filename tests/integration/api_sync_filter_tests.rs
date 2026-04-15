use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "device_id": "FILTERSYNC"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
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
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn send_event(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    event_type: &str,
    txn_id: &str,
    content: Value,
) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/{}/{}",
            room_id, event_type, txn_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn create_filter(app: &axum::Router, token: &str, user_id: &str, filter: Value) -> String {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/user/{}/filter", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(filter.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["filter_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_sync_filter_applies_room_timeline_matchers_before_limit() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("sync_filter_{}", rand::random::<u32>());
    let (token, user_id) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "sync_filter_room").await;

    send_event(
        &app,
        &token,
        &room_id,
        "m.room.message",
        "timeline_msg_1",
        json!({
            "msgtype": "m.text",
            "body": "message event"
        }),
    )
    .await;
    send_event(
        &app,
        &token,
        &room_id,
        "m.custom.test",
        "timeline_custom_1",
        json!({
            "body": "custom event"
        }),
    )
    .await;

    let filter_id = create_filter(
        &app,
        &token,
        &user_id,
        json!({
            "room": {
                "timeline": {
                    "limit": 1,
                    "types": ["m.room.message"]
                }
            }
        }),
    )
    .await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/sync?filter={}", filter_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 128 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let timeline = json["rooms"]["join"][room_id.as_str()]["timeline"]["events"]
        .as_array()
        .expect("timeline events should be an array");

    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0]["type"], "m.room.message");
    assert_eq!(timeline[0]["content"]["body"], "message event");
}
