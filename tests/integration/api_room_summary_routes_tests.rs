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
    register_user_with_id(app, username).await.0
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

#[tokio::test]
async fn test_room_summary_route_rejects_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = register_user(&app, "room_summary_owner_read_guard").await;
    let guest_token = register_user(&app, "room_summary_guest_read_guard").await;
    let (admin_token, _) = super::get_admin_token(&app).await;
    let room_id = create_room(&app, &owner_token, "Protected summary room").await;

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_summary_members_route_rejects_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = register_user(&app, "room_summary_owner_members_guard").await;
    let guest_token = register_user(&app, "room_summary_guest_members_guard").await;
    let (admin_token, _) = super::get_admin_token(&app).await;
    let room_id = create_room(&app, &owner_token, "Protected summary members room").await;

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/summary/members",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/summary/members",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_summary_create_rejects_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = register_user(&app, "room_summary_owner_write_guard").await;
    let guest_token = register_user(&app, "room_summary_guest_write_guard").await;
    let (admin_token, _) = super::get_admin_token(&app).await;
    let room_id = create_room(&app, &owner_token, "Protected summary write room").await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "name": "should be rejected"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "name": "admin should be rejected"
            })
            .to_string(),
        ))
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_summary_create_rejects_joined_non_creator_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (owner_token, _) = register_user_with_id(&app, "room_summary_owner_creator_guard").await;
    let (member_token, member_user_id) =
        register_user_with_id(&app, "room_summary_member_creator_guard").await;
    let room_id = create_room(&app, &owner_token, "Creator-only summary room").await;

    invite_user(&app, &owner_token, &room_id, &member_user_id).await;
    join_room(&app, &member_token, &room_id).await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "name": "joined member should be rejected"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_summary_read_routes_share_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(&app, "room_summary_routes_shared").await;
    let room_id = create_room(&app, &token, "Shared summary room").await;

    let v3_get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let v3_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_get_request)
        .await
        .unwrap();
    assert_eq!(v3_get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_get_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["name"], "Shared summary room");

    let r0_get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let r0_get_response = ServiceExt::<Request<Body>>::oneshot(app, r0_get_request)
        .await
        .unwrap();
    assert_eq!(r0_get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_get_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["name"], "Shared summary room");
}

#[tokio::test]
async fn test_room_summary_create_rejects_path_body_mismatch() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(&app, "room_summary_path_body_mismatch").await;
    let room_id = create_room(&app, &token, "Mismatch summary room").await;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/summary", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": "!another-room:localhost",
                "name": "mismatch"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_room_summary_route_boundaries_are_preserved() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let room_id = "!room-summary-boundary:localhost";

    let r0_write_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/summary", room_id))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "name": "should not exist on r0"
            })
            .to_string(),
        ))
        .unwrap();
    let r0_write_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_write_request)
        .await
        .unwrap();
    assert_eq!(r0_write_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let v3_unread_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/summary/unread/clear",
            room_id
        ))
        .body(Body::empty())
        .unwrap();
    let v3_unread_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_unread_request)
        .await
        .unwrap();
    assert_eq!(v3_unread_response.status(), StatusCode::UNAUTHORIZED);

    let r0_unread_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/summary/unread/clear",
            room_id
        ))
        .body(Body::empty())
        .unwrap();
    let r0_unread_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_unread_request)
        .await
        .unwrap();
    assert_eq!(r0_unread_response.status(), StatusCode::NOT_FOUND);

    let synapse_request = Request::builder()
        .method("GET")
        .uri("/_synapse/room_summary/v1/summaries")
        .body(Body::empty())
        .unwrap();
    let synapse_response = ServiceExt::<Request<Body>>::oneshot(app, synapse_request)
        .await
        .unwrap();
    assert_eq!(synapse_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_room_summary_snapshot_exposes_members_state_and_stats() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user_with_id(&app, "room_summary_snapshot").await;
    let room_id = create_room(&app, &token, "Snapshot summary room").await;

    let members_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/summary/members",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let members_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), members_request)
        .await
        .unwrap();
    assert_eq!(members_response.status(), StatusCode::OK);

    let members_body = axum::body::to_bytes(members_response.into_body(), 4096)
        .await
        .unwrap();
    let members_json: Value = serde_json::from_slice(&members_body).unwrap();
    let members = members_json.as_array().unwrap();
    assert!(!members.is_empty());
    assert!(members.iter().any(|member| {
        member["user_id"] == user_id
            && member["membership"] == "join"
            && member["is_hero"] == Value::Bool(true)
    }));

    let state_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/summary/state",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let state_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), state_request)
        .await
        .unwrap();
    assert_eq!(state_response.status(), StatusCode::OK);

    let state_body = axum::body::to_bytes(state_response.into_body(), 4096)
        .await
        .unwrap();
    let state_json: Value = serde_json::from_slice(&state_body).unwrap();
    let states = state_json.as_array().unwrap();
    assert!(!states.is_empty());
    assert!(states
        .iter()
        .any(|state| state["event_type"] == "m.room.create"));

    let stats_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/summary/stats",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let stats_response = ServiceExt::<Request<Body>>::oneshot(app, stats_request)
        .await
        .unwrap();
    assert_eq!(stats_response.status(), StatusCode::OK);

    let stats_body = axum::body::to_bytes(stats_response.into_body(), 4096)
        .await
        .unwrap();
    let stats_json: Value = serde_json::from_slice(&stats_body).unwrap();
    assert_eq!(stats_json["room_id"], room_id);
    assert!(stats_json["total_events"].as_i64().unwrap_or_default() > 0);
    assert!(
        stats_json["total_state_events"]
            .as_i64()
            .unwrap_or_default()
            > 0
    );
}

#[tokio::test]
async fn test_room_summary_internal_summaries_route_returns_list_object() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(&app, "room_summary_internal_list").await;
    let room_id = create_room(&app, &token, "Internal list summary room").await;

    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/room_summary/v1/summaries")
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

    assert!(json.is_object());
    let summaries = json["summaries"]
        .as_array()
        .expect("summaries should be an array");
    let rooms = json["rooms"].as_array().expect("rooms should be an array");
    let chunk = json["chunk"].as_array().expect("chunk should be an array");

    assert!(!summaries.is_empty());
    assert_eq!(summaries[0]["room_id"], room_id);
    assert_eq!(rooms, summaries);
    assert_eq!(chunk, summaries);
    assert!(json.get("next_batch").is_none());
}
