use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

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

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

fn encode_room_id(room_id: &str) -> String {
    room_id.replace('!', "%21").replace(':', "%3A")
}

fn encode_user_id(user_id: &str) -> String {
    user_id
        .replace('@', "%40")
        .replace(':', "%3A")
        .replace('+', "%2B")
}

async fn assert_routable(app: &axum::Router, request: Request<Body>) {
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();

    if status == StatusCode::BAD_REQUEST {
        let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
        assert_ne!(
            json.get("errcode").and_then(|value| value.as_str()),
            Some("M_UNRECOGNIZED"),
            "route unexpectedly returned M_UNRECOGNIZED: {}",
            String::from_utf8_lossy(&body)
        );
    }
}

#[tokio::test]
async fn test_room_metadata_returns_room_fields() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("metadata_{}", rand::random::<u32>());
    let alice_token = register_user(&app, &alice).await;
    let room_id = create_room(&app, &alice_token, "Room Metadata").await;
    let encoded_room_id = encode_room_id(&room_id);

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/metadata",
            encoded_room_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert!(json.get("join_rule").is_some());
    assert!(json.get("room_version").is_some());
    assert!(json.get("created_ts").is_some());
}

#[tokio::test]
async fn test_room_extension_routes_are_routable_for_member() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("placeholders_{}", rand::random::<u32>());
    let alice_token = register_user(&app, &alice).await;
    let room_id = create_room(&app, &alice_token, "Room Placeholder").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/message_queue",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/device/DEVICE",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/threads/$thread:localhost",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    let alice_user_id = format!("@{}:localhost", alice);
    let encoded_user_id = encode_user_id(&alice_user_id);
    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/fragments/{}",
                encoded_room_id, encoded_user_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/service_types",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/event_perspective",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/encrypted_events",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/invites",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/reduced_events",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/rendered/",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/event/$event:localhost/url",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/translate/$event:localhost",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/convert/$event:localhost",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/sign/$event:localhost",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/verify/$event:localhost",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/vault_data",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/vault_data",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/retention",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_routable(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/external_ids",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
}

#[tokio::test]
async fn test_room_extension_placeholders_reject_non_member() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("placeholder_owner_{}", rand::random::<u32>());
    let bob = format!("placeholder_guest_{}", rand::random::<u32>());
    let alice_token = register_user(&app, &alice).await;
    let bob_token = register_user(&app, &bob).await;

    let room_id = create_room(&app, &alice_token, "Room Placeholder Protected").await;
    let encoded_room_id = encode_room_id(&room_id);

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/message_queue",
            encoded_room_id
        ))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_membership_requires_membership() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("membership_owner_{}", rand::random::<u32>());
    let bob = format!("membership_guest_{}", rand::random::<u32>());
    let alice_token = register_user(&app, &alice).await;
    let bob_token = register_user(&app, &bob).await;

    let room_id = create_room(&app, &alice_token, "Room Membership").await;
    let encoded_room_id = encode_room_id(&room_id);
    let alice_user_id = format!("@{}:localhost", alice);
    let encoded_user_id = encode_user_id(&alice_user_id);

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/membership/{}",
            encoded_room_id, encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/membership/{}",
            encoded_room_id, encoded_user_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("membership").and_then(|v| v.as_str()).is_some());
}
