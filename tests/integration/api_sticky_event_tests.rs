use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

async fn read_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn register_user_with_id(app: &axum::Router, username: &str) -> (String, String) {
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

    let json = read_json(response).await;
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

    let json = read_json(response).await;
    json["room_id"].as_str().unwrap().to_string()
}

async fn invite_user(app: &axum::Router, token: &str, room_id: &str, user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn send_message(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    txn_id: &str,
    body: Value,
) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            room_id, txn_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = read_json(response).await;
    json["event_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_set_sticky_event_rejects_event_from_different_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (token, _) =
        register_user_with_id(&app, &format!("sticky_owner_{}", rand::random::<u32>())).await;
    let room_a = create_room(&app, &token, "Sticky A").await;
    let room_b = create_room(&app, &token, "Sticky B").await;
    let foreign_event_id = send_message(
        &app,
        &token,
        &room_b,
        "sticky-foreign-event",
        json!({ "msgtype": "m.text", "body": "foreign" }),
    )
    .await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/sticky_events", room_a))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "events": [{
                    "event_type": "m.room.message",
                    "event_id": foreign_event_id
                }]
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_sticky_events_are_scoped_to_the_authenticated_user() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (owner_token, _) =
        register_user_with_id(&app, &format!("sticky_owner_{}", rand::random::<u32>())).await;
    let (member_token, member_user_id) =
        register_user_with_id(&app, &format!("sticky_member_{}", rand::random::<u32>())).await;
    let room_id = create_room(&app, &owner_token, "Sticky Shared Room").await;
    invite_user(&app, &owner_token, &room_id, &member_user_id).await;
    join_room(&app, &member_token, &room_id).await;

    let member_event_id = send_message(
        &app,
        &member_token,
        &room_id,
        "sticky-member-event",
        json!({ "msgtype": "m.text", "body": "member event" }),
    )
    .await;

    let set_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/sticky_events",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", member_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "events": [{
                    "event_type": "m.room.message",
                    "event_id": member_event_id
                }]
            })
            .to_string(),
        ))
        .unwrap();
    let set_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), set_request)
        .await
        .unwrap();
    assert_eq!(set_response.status(), StatusCode::OK);

    let member_get_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/sticky_events",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let member_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), member_get_request)
        .await
        .unwrap();
    assert_eq!(member_get_response.status(), StatusCode::OK);
    let member_json = read_json(member_get_response).await;
    assert_eq!(member_json["events"].as_array().unwrap().len(), 1);
    assert_eq!(member_json["events"][0]["event_id"], member_event_id);

    let owner_get_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/sticky_events",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let owner_get_response = ServiceExt::<Request<Body>>::oneshot(app, owner_get_request)
        .await
        .unwrap();
    assert_eq!(owner_get_response.status(), StatusCode::OK);
    let owner_json = read_json(owner_get_response).await;
    assert_eq!(owner_json["events"].as_array().unwrap().len(), 0);
}
