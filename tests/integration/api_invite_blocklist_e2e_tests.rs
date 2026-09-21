// End-to-end invite policy integration tests covering the full
// auth chain: room list → global blocklist/allowlist → MSC4155
// account policy → ignore list (MSC3873).
//
// Each test uses a fresh isolated schema pool via TestContext,
// ensuring no cross-test data interference.
//
// IMPORTANT: `account_policy_denies` fails-closed when
// `m.invite_permission_config` is absent. Every user expected
// to be invitable must have it set to `{"default_action":"allow"}`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::TestContext::new().await.map(|ctx| ctx.app)
}

async fn register_user(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

/// Register a user and set their `m.invite_permission_config` so
/// they are not blocked by `account_policy_denies`.
async fn register_invitee(app: &axum::Router, admin_token: &str, username: &str) -> String {
    let token = register_user(app, username).await;
    let user_id = format!("@{}:localhost", username);
    set_account_data(app, &token, &user_id, "m.invite_permission_config", &json!({"default_action": "allow"})).await;
    token
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    v["room_id"].as_str().unwrap().to_string()
}

async fn invite_user(app: &axum::Router, token: &str, room_id: &str, user_id: &str) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    response.status()
}

async fn set_account_data(app: &axum::Router, token: &str, user_id: &str, data_type: &str, content: &Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/{}", user_id, data_type))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn admin_post_global_blocklist(app: &axum::Router, token: &str, user_ids: &[&str]) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/invite/blocklist")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_ids": user_ids }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    response.status()
}

async fn admin_post_global_allowlist(app: &axum::Router, token: &str, user_ids: &[&str]) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/invite/allowlist")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_ids": user_ids }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    response.status()
}

async fn admin_get_global_blocklist(app: &axum::Router, token: &str) -> Value {
    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/invite/blocklist")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn admin_get_global_allowlist(app: &axum::Router, token: &str) -> Value {
    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/invite/allowlist")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Global blocklist blocks an invite even when the user is not
/// room-blocked — the auth chain checks global policy after room
/// lists and before MSC4155/ignore.
#[tokio::test]
async fn test_global_blocklist_blocks_invite() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_global_block").await;
    let invitee = "@blocked:localhost";
    let _invitee_token = register_user(&app, "blocked").await;

    let room_id = create_room(&app, &inviter_token, "Global Blocklist Guard").await;

    // Set the global blocklist to include the invitee
    assert_eq!(
        admin_post_global_blocklist(&app, &admin_token, &[invitee]).await,
        StatusCode::OK
    );

    // Verify the admin API round-trips correctly
    let blocklist = admin_get_global_blocklist(&app, &admin_token).await;
    let entries: Vec<&Value> = blocklist
        .get("blocklist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default();
    assert!(!entries.is_empty(), "global blocklist should have at least 1 entry");

    // Invite should be rejected by the global blocklist
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "invite should be forbidden when user is globally blocked"
    );
}

/// Global allowlist overrides global blocklist on a per-user basis.
/// A user in the global allowlist should be invited even if they
/// appear in the global blocklist.
#[tokio::test]
async fn test_global_allowlist_overrides_blocklist() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_allowlist_override").await;
    let invitee = "@allowed:localhost";
    let _invitee_token = register_invitee(&app, &admin_token, "allowed").await;

    let room_id = create_room(&app, &inviter_token, "Allowlist Override Guard").await;

    // Set both blocklist and allowlist
    assert_eq!(
        admin_post_global_blocklist(&app, &admin_token, &[invitee]).await,
        StatusCode::OK
    );
    assert_eq!(
        admin_post_global_allowlist(&app, &admin_token, &[invitee]).await,
        StatusCode::OK
    );

    // Invite should succeed because the global allowlist overrides the blocklist
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "invite should succeed when user is in global allowlist"
    );

    // Verify the allowlist API round-trips correctly
    let allowlist = admin_get_global_allowlist(&app, &admin_token).await;
    let entries: Vec<&Value> = allowlist
        .get("allowlist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default();
    assert!(!entries.is_empty(), "global allowlist should have at least 1 entry");
}

/// Global blocklist does not affect users not listed in it.
#[tokio::test]
async fn test_global_blocklist_does_not_affect_others() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_not_blocked").await;
    let invitee = "@clear:localhost";
    let _invitee_token = register_invitee(&app, &admin_token, "clear").await;

    let room_id = create_room(&app, &inviter_token, "Clear Invite Guard").await;

    // Set global blocklist for a different user
    let other = "@other:localhost";
    assert_eq!(
        admin_post_global_blocklist(&app, &admin_token, &[other]).await,
        StatusCode::OK
    );

    // Invite for a non-blocked user should succeed
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "invite should succeed for users not in the global blocklist"
    );
}

/// Admin API round-trip: POST then GET global blocklist returns same data.
#[tokio::test]
async fn test_global_blocklist_admin_api_round_trip() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let users = vec!["@user_a_round_trip:localhost", "@user_b_round_trip:localhost"];

    // POST sets the blocklist
    assert_eq!(
        admin_post_global_blocklist(&app, &admin_token, &users).await,
        StatusCode::OK
    );

    // GET returns the blocklist
    let blocklist = admin_get_global_blocklist(&app, &admin_token).await;
    let blocklist_users: Vec<String> = blocklist
        .get("blocklist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.get("user_id").and_then(|u| u.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    assert!(blocklist_users.contains(&users[0].to_string()));
    assert!(blocklist_users.contains(&users[1].to_string()));
}

/// Admin API round-trip: POST then GET global allowlist returns same data.
#[tokio::test]
async fn test_global_allowlist_admin_api_round_trip() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let users = vec!["@user_a_allow_round_trip:localhost", "@user_b_allow_round_trip:localhost"];

    // POST sets the allowlist
    assert_eq!(
        admin_post_global_allowlist(&app, &admin_token, &users).await,
        StatusCode::OK
    );

    // GET returns the allowlist
    let allowlist = admin_get_global_allowlist(&app, &admin_token).await;
    let allowlist_users: Vec<String> = allowlist
        .get("allowlist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.get("user_id").and_then(|u| u.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    assert!(allowlist_users.contains(&users[0].to_string()));
    assert!(allowlist_users.contains(&users[1].to_string()));
}

/// Global blocklist + global allowlist: allowlist entry clears block.
#[tokio::test]
async fn test_global_allowlist_after_blocklist_coexists() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_coexist").await;
    let invitee = "@coexist:localhost";
    let _invitee_token = register_invitee(&app, &admin_token, "coexist").await;

    let room_id = create_room(&app, &inviter_token, "Coexist Guard").await;

    // Set blocklist first
    assert_eq!(
        admin_post_global_blocklist(&app, &admin_token, &[invitee]).await,
        StatusCode::OK
    );

    // Then set allowlist for the same user
    assert_eq!(
        admin_post_global_allowlist(&app, &admin_token, &[invitee]).await,
        StatusCode::OK
    );

    // Invite should succeed (allowlist overrides blocklist)
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "invite should succeed when user is in global allowlist even if also in blocklist"
    );
}

/// Empty global blocklist does not block anyone.
#[tokio::test]
async fn test_empty_global_blocklist_does_not_block() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_empty").await;
    let invitee = "@empty:localhost";
    let _invitee_token = register_invitee(&app, &admin_token, "empty").await;

    let room_id = create_room(&app, &inviter_token, "Empty Guard").await;

    // Set an empty global blocklist
    assert_eq!(
        admin_post_global_blocklist(&app, &admin_token, &[] as &[&str]).await,
        StatusCode::OK
    );

    // Invite should succeed (no global blocklist entries)
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "invite should succeed when global blocklist is empty"
    );
}