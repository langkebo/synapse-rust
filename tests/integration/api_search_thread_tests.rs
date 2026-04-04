use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn create_room(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Thread Test Room" }).to_string(),
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
    json["room_id"].as_str().unwrap().to_string()
}

async fn send_message(app: &axum::Router, token: &str, room_id: &str, txn_id: &str) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            room_id, txn_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "msgtype": "m.text",
                "body": "thread root"
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
    json["event_id"].as_str().unwrap().to_string()
}

async fn create_thread(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    root_event_id: &str,
) -> String {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/rooms/{}/threads", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "root_event_id": root_event_id,
                "content": {
                    "body": "thread"
                }
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
    json["thread_id"].as_str().unwrap().to_string()
}

async fn add_reply(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    thread_id: &str,
    root_event_id: &str,
    event_id: &str,
) {
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v1/rooms/{}/threads/{}/replies",
            room_id, thread_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "event_id": event_id,
                "root_event_id": root_event_id,
                "content": {
                    "msgtype": "m.text",
                    "body": "thread reply"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_v3_search_validation_is_preserved_after_router_refactor() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/search")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_categories": {
                    "room_events": {
                        "search_term": "   ",
                        "keys": ["content.body"]
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Search term cannot be empty");
}

#[tokio::test]
async fn test_r0_search_recipients_route_still_works_after_nesting() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/search_recipients")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_term": "   "
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Search term cannot be empty");
}

#[tokio::test]
async fn test_thread_compat_route_is_available_from_thread_module() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/user/@alice:localhost/rooms/!room:localhost/threads")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_thread_returns_real_thread_details() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    let root_event_id = send_message(&app, &token, &room_id, "thread_root_txn").await;
    let thread_id = create_thread(&app, &token, &room_id, &root_event_id).await;
    let reply_event_id = format!("$thread_reply_{}", rand::random::<u64>());

    add_reply(
        &app,
        &token,
        &room_id,
        &thread_id,
        &root_event_id,
        &reply_event_id,
    )
    .await;

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v1/rooms/{}/threads/{}?include_replies=true",
            room_id, thread_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["root"]["thread_id"], thread_id);
    assert_eq!(json["root"]["root_event_id"], root_event_id);
    assert_eq!(json["reply_count"], 1);
    assert_eq!(json["replies"].as_array().unwrap().len(), 1);
    assert_eq!(json["replies"][0]["thread_id"], thread_id);
}

#[tokio::test]
async fn test_global_thread_routes_return_real_data() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let token = super::create_test_user(&app).await;
    let room_id = create_room(&app, &token).await;
    let root_event_id = send_message(&app, &token, &room_id, "global_thread_root_txn").await;

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v1/threads")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "root_event_id": root_event_id,
                "content": {
                    "body": "global thread"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 2048)
        .await
        .unwrap();
    let create_json: Value = serde_json::from_slice(&body).unwrap();
    let thread_id = create_json["thread_id"].as_str().unwrap().to_string();

    let list_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/threads")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let list_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_request)
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(list_response.into_body(), 4096)
        .await
        .unwrap();
    let list_json: Value = serde_json::from_slice(&body).unwrap();
    let threads = list_json["threads"].as_array().unwrap();

    assert!(threads
        .iter()
        .any(|thread| thread["thread_id"] == thread_id && thread["room_id"] == room_id));
    assert!(list_json["total"].as_i64().unwrap_or_default() >= 1);

    let subscribe_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v1/rooms/{}/threads/{}/subscribe",
            room_id, thread_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "notification_level": "all"
            })
            .to_string(),
        ))
        .unwrap();

    let subscribe_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), subscribe_request)
            .await
            .unwrap();
    assert_eq!(subscribe_response.status(), StatusCode::OK);

    let subscribed_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/threads/subscribed")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let subscribed_response =
        ServiceExt::<Request<Body>>::oneshot(app, subscribed_request)
            .await
            .unwrap();
    assert_eq!(subscribed_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(subscribed_response.into_body(), 4096)
        .await
        .unwrap();
    let subscribed_json: Value = serde_json::from_slice(&body).unwrap();

    assert!(subscribed_json["threads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|thread| thread["thread_id"] == thread_id));
    assert!(subscribed_json["subscribed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|subscription| {
            subscription["thread_id"] == thread_id
                && subscription["notification_level"] == "all"
        }));
}
