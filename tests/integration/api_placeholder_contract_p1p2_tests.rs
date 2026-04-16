use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

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

async fn create_room_with_initial_state(
    app: &axum::Router,
    token: &str,
    name: &str,
    initial_state: Vec<Value>,
) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": name, "initial_state": initial_state }).to_string(),
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

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    create_room_with_initial_state(app, token, name, vec![]).await
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
                "body": "contract message"
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

async fn put_state_event_empty_key(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    event_type: &str,
    content: Value,
) -> String {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/state/{}/",
            room_id, event_type
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().unwrap().to_string()
}

fn encode_room_id(room_id: &str) -> String {
    room_id.replace('!', "%21").replace(':', "%3A")
}

async fn assert_matrix_error(
    app: &axum::Router,
    request: Request<Body>,
    expected_status: StatusCode,
    expected_errcode: &str,
) {
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), expected_status);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], expected_errcode);
}

#[tokio::test]
async fn test_power_levels_contract_allows_explicit_room_admin_to_kick() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("power_levels_alice_{}", rand::random::<u32>());
    let bob = format!("power_levels_bob_{}", rand::random::<u32>());
    let charlie = format!("power_levels_charlie_{}", rand::random::<u32>());
    let (alice_token, alice_user_id) = register_user(&app, &alice).await;
    let (bob_token, bob_user_id) = register_user(&app, &bob).await;
    let (charlie_token, charlie_user_id) = register_user(&app, &charlie).await;

    let room_id = create_room(&app, &alice_token, "Power Level Kick Contract").await;

    let power_levels_response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/state/m.room.power_levels",
                room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "users": {
                        alice_user_id.clone(): 100,
                        bob_user_id.clone(): 100,
                        charlie_user_id.clone(): 0
                    },
                    "users_default": 0,
                    "events_default": 0,
                    "state_default": 50,
                    "ban": 50,
                    "kick": 50,
                    "redact": 50,
                    "invite": 0
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(power_levels_response.status(), StatusCode::OK);

    invite_user(&app, &alice_token, &room_id, &bob_user_id).await;
    invite_user(&app, &alice_token, &room_id, &charlie_user_id).await;
    join_room(&app, &bob_token, &room_id).await;
    join_room(&app, &charlie_token, &room_id).await;

    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/kick", room_id))
            .header("Authorization", format!("Bearer {}", bob_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "user_id": charlie_user_id, "reason": "moderation" }).to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_power_levels_contract_rejects_non_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("power_levels_owner_{}", rand::random::<u32>());
    let outsider = format!("power_levels_outsider_{}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (outsider_token, _) = register_user(&app, &outsider).await;

    let room_id = create_room(&app, &owner_token, "Power Levels Membership Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v1/rooms/{}/state/m.room.power_levels/",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", outsider_token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_membership_events_contract_rejects_non_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("membership_events_owner_{}", rand::random::<u32>());
    let outsider = format!("membership_events_outsider_{}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (outsider_token, _) = register_user(&app, &outsider).await;

    let room_id = create_room(&app, &owner_token, "Membership Events Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/get_membership_events",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", outsider_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({ "limit": 10 }).to_string()))
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_pinned_events_contract_rejects_non_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("pinned_owner_{}", rand::random::<u32>());
    let outsider = format!("pinned_outsider_{}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (outsider_token, _) = register_user(&app, &outsider).await;

    let room_id = create_room(&app, &owner_token, "Pinned Events Visibility Contract").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/pinned_events",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", outsider_token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_pin_event_contract_rejects_non_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("pin_event_owner_{}", rand::random::<u32>());
    let outsider = format!("pin_event_outsider_{}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (outsider_token, _) = register_user(&app, &outsider).await;

    let room_id = create_room(&app, &owner_token, "Pinned Events Write Contract").await;
    let event_id = send_message(&app, &owner_token, &room_id, "pin_non_member_txn").await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/pinned_events",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", outsider_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({ "event_id": event_id }).to_string()))
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_pin_event_contract_rejects_regular_members_without_power() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("pin_member_owner_{}", rand::random::<u32>());
    let member = format!("pin_member_joined_{}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (member_token, member_user_id) = register_user(&app, &member).await;

    let room_id = create_room(&app, &owner_token, "Pinned Events Power Contract").await;
    let event_id = send_message(&app, &owner_token, &room_id, "pin_member_txn").await;
    invite_user(&app, &owner_token, &room_id, &member_user_id).await;
    join_room(&app, &member_token, &room_id).await;
    let encoded_room_id = encode_room_id(&room_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/pinned_events",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", member_token))
            .header("Content-Type", "application/json")
            .body(Body::from(json!({ "event_id": event_id }).to_string()))
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_search_rooms_contract_hides_private_rooms_from_outsiders() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("search_private_owner_{}", rand::random::<u32>());
    let outsider = format!("search_private_outsider_{}", rand::random::<u32>());
    let room_name = format!("Private Search Contract {}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (outsider_token, _) = register_user(&app, &outsider).await;

    let room_id = create_room(&app, &owner_token, &room_name).await;

    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/_matrix/client/r0/search_rooms")
            .header("Authorization", format!("Bearer {}", outsider_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "search_term": room_name, "limit": 10 }).to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let results = json["results"].as_array().unwrap();
    assert!(
        !results
            .iter()
            .any(|room| room["room_id"].as_str() == Some(room_id.as_str())),
        "Outsiders should not discover private rooms via search_rooms"
    );
}

#[tokio::test]
async fn test_search_rooms_contract_allows_members_to_find_joined_private_rooms() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("search_joined_owner_{}", rand::random::<u32>());
    let member = format!("search_joined_member_{}", rand::random::<u32>());
    let room_name = format!("Joined Search Contract {}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (member_token, member_user_id) = register_user(&app, &member).await;

    let room_id = create_room(&app, &owner_token, &room_name).await;
    invite_user(&app, &owner_token, &room_id, &member_user_id).await;
    join_room(&app, &member_token, &room_id).await;

    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/_matrix/client/r0/search_rooms")
            .header("Authorization", format!("Bearer {}", member_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "search_term": room_name, "limit": 10 }).to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let results = json["results"].as_array().unwrap();
    assert!(
        results
            .iter()
            .any(|room| room["room_id"].as_str() == Some(room_id.as_str())),
        "Joined members should still be able to find their private rooms via search_rooms"
    );
}

#[tokio::test]
async fn test_protected_state_events_contract_rejects_regular_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("protected_state_alice_{}", rand::random::<u32>());
    let bob = format!("protected_state_bob_{}", rand::random::<u32>());
    let (alice_token, _) = register_user(&app, &alice).await;
    let (bob_token, bob_user_id) = register_user(&app, &bob).await;

    let room_id = create_room(&app, &alice_token, "Protected State Events").await;
    invite_user(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    for (event_type, body) in [
        ("m.room.join_rules", json!({ "join_rule": "public" })),
        (
            "m.room.power_levels",
            json!({
                "users": {},
                "users_default": 0,
                "events_default": 0,
                "state_default": 50,
                "ban": 50,
                "kick": 50,
                "redact": 50,
                "invite": 0
            }),
        ),
    ] {
        assert_matrix_error(
            &app,
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/_matrix/client/r0/rooms/{}/state/{}",
                    room_id, event_type
                ))
                .header("Authorization", format!("Bearer {}", bob_token))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            StatusCode::FORBIDDEN,
            "M_FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn test_state_default_contract_rejects_regular_member_topic_write() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("state_default_alice_{}", rand::random::<u32>());
    let bob = format!("state_default_bob_{}", rand::random::<u32>());
    let (alice_token, _) = register_user(&app, &alice).await;
    let (bob_token, bob_user_id) = register_user(&app, &bob).await;

    let room_id = create_room(&app, &alice_token, "State Default Contract").await;
    invite_user(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;

    assert_matrix_error(
        &app,
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/state/m.room.topic",
                room_id
            ))
            .header("Authorization", format!("Bearer {}", bob_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "topic": "member overwrite" }).to_string(),
            ))
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_invite_only_join_contract_rejects_uninvited_user() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("invite_only_alice_{}", rand::random::<u32>());
    let bob = format!("invite_only_bob_{}", rand::random::<u32>());
    let (alice_token, _) = register_user(&app, &alice).await;
    let (bob_token, _) = register_user(&app, &bob).await;

    let room_id = create_room_with_initial_state(
        &app,
        &alice_token,
        "Invite Only Contract",
        vec![json!({
            "type": "m.room.join_rules",
            "content": { "join_rule": "invite" }
        })],
    )
    .await;

    assert_matrix_error(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
            .header("Authorization", format!("Bearer {}", bob_token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_room_info_contract_reflects_invites_and_guest_access() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("room_info_alice_{}", rand::random::<u32>());
    let bob = format!("room_info_bob_{}", rand::random::<u32>());
    let (alice_token, _) = register_user(&app, &alice).await;
    let (_, bob_user_id) = register_user(&app, &bob).await;

    let room_id = create_room(&app, &alice_token, "Room Info Contract").await;

    put_state_event_empty_key(
        &app,
        &alice_token,
        &room_id,
        "m.room.guest_access",
        json!({ "guest_access": "can_join" }),
    )
    .await;

    invite_user(&app, &alice_token, &room_id, &bob_user_id).await;

    let encoded_room_id = encode_room_id(&room_id);
    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!("/_matrix/client/r0/rooms/{}", encoded_room_id))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["room_id"], room_id);
    assert!(
        json["invited_members_count"].as_i64().unwrap_or(0) >= 1,
        "invited_members_count should reflect real invites"
    );
    assert_eq!(json["guest_can_join"], true);
}

#[tokio::test]
async fn test_room_members_recent_contract_uses_coherent_index_tokens() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let alice = format!("members_recent_alice_{}", rand::random::<u32>());
    let bob = format!("members_recent_bob_{}", rand::random::<u32>());
    let charlie = format!("members_recent_charlie_{}", rand::random::<u32>());
    let (alice_token, _) = register_user(&app, &alice).await;
    let (bob_token, bob_user_id) = register_user(&app, &bob).await;
    let (charlie_token, charlie_user_id) = register_user(&app, &charlie).await;

    let room_id = create_room(&app, &alice_token, "Members Recent Contract").await;

    invite_user(&app, &alice_token, &room_id, &bob_user_id).await;
    join_room(&app, &bob_token, &room_id).await;
    invite_user(&app, &alice_token, &room_id, &charlie_user_id).await;
    join_room(&app, &charlie_token, &room_id).await;

    let encoded_room_id = encode_room_id(&room_id);
    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/members/recent?from=0&limit=1",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let page1: Value = serde_json::from_slice(&body).unwrap();
    let page1_chunk_len = page1["chunk"].as_array().map(|v| v.len()).unwrap_or(0);
    assert_eq!(page1_chunk_len, 1);
    assert_eq!(page1["start"], "0");
    assert_eq!(page1["end"], "1");

    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/r0/rooms/{}/members/recent?from=1&limit=1",
                encoded_room_id
            ))
            .header("Authorization", format!("Bearer {}", alice_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let page2: Value = serde_json::from_slice(&body).unwrap();
    let page2_chunk_len = page2["chunk"].as_array().map(|v| v.len()).unwrap_or(0);
    assert_eq!(page2_chunk_len, 1);
    assert_eq!(page2["start"], "1");
    assert_eq!(page2["end"], "2");
}

#[tokio::test]
async fn test_scanner_info_contract_is_not_empty_success() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("scanner_info_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Scanner Info Contract").await;
    let event_id = send_message(&app, &token, &room_id, "scanner_txn").await;

    let encoded_room_id = encode_room_id(&room_id);
    let encoded_event_id = urlencoding::encode(&event_id);
    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v1/rooms/{}/report/{}/scanner_info",
                encoded_room_id, encoded_event_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["scanner_enabled"], false);
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["event_id"], event_id);
    assert_eq!(json["status"], "not_configured");
    assert!(
        !json["message"].as_str().unwrap_or("").trim().is_empty(),
        "message should not be empty"
    );
}

#[tokio::test]
async fn test_scanner_info_contract_rejects_non_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner = format!("scanner_info_owner_{}", rand::random::<u32>());
    let outsider = format!("scanner_info_outsider_{}", rand::random::<u32>());
    let (owner_token, _) = register_user(&app, &owner).await;
    let (outsider_token, _) = register_user(&app, &outsider).await;
    let room_id = create_room(&app, &owner_token, "Scanner Info Membership Contract").await;
    let event_id = send_message(&app, &owner_token, &room_id, "scanner_membership_txn").await;

    let encoded_room_id = encode_room_id(&room_id);
    let encoded_event_id = urlencoding::encode(&event_id);

    assert_matrix_error(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v1/rooms/{}/report/{}/scanner_info",
                encoded_room_id, encoded_event_id
            ))
            .header("Authorization", format!("Bearer {}", outsider_token))
            .body(Body::empty())
            .unwrap(),
        StatusCode::FORBIDDEN,
        "M_FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn test_room_account_data_write_ack_persists_value() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let username = format!("room_account_data_{}", rand::random::<u32>());
    let (token, _) = register_user(&app, &username).await;
    let room_id = create_room(&app, &token, "Room Account Data Contract").await;
    let data_type = format!("com.example.contract.{}", rand::random::<u32>());
    let payload = json!({ "enabled": true, "value": rand::random::<u32>() });

    let encoded_room_id = encode_room_id(&room_id);
    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/account_data/{}",
                encoded_room_id, data_type
            ))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = ServiceExt::<Request<Body>>::oneshot(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/account_data/{}",
                encoded_room_id, data_type
            ))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, payload);
}
