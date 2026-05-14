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

async fn json_request(
    app: &axum::Router,
    method: &str,
    uri: String,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    }
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }

    let request = builder
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };

    (status, json)
}

async fn establish_friend_dm(
    app: &axum::Router,
    alice_name: &str,
    bob_name: &str,
) -> (String, String, String, String, String) {
    let (alice_token, alice_user_id) = register_user(app, alice_name).await;
    let (bob_token, bob_user_id) = register_user(app, bob_name).await;

    let (send_status, send_body) = json_request(
        app,
        "POST",
        "/_matrix/client/v1/friends".to_string(),
        Some(&alice_token),
        Some(json!({
            "user_id": bob_user_id,
            "message": "let's connect"
        })),
    )
    .await;
    assert_eq!(
        send_status,
        StatusCode::OK,
        "send friend request failed: {send_body}"
    );

    let (accept_status, accept_body) = json_request(
        app,
        "POST",
        format!(
            "/_matrix/client/v1/friends/request/{}/accept",
            alice_user_id
        ),
        Some(&bob_token),
        None,
    )
    .await;
    assert_eq!(
        accept_status,
        StatusCode::OK,
        "accept friend request failed: {accept_body}"
    );

    let dm_room_id = accept_body["room_id"]
        .as_str()
        .expect("friend accept should return dm room id")
        .to_string();

    (
        alice_token,
        alice_user_id,
        bob_token,
        bob_user_id,
        dm_room_id,
    )
}

async fn fetch_friend_entry(app: &axum::Router, token: &str, target_user_id: &str) -> Value {
    let (status, body) = json_request(
        app,
        "GET",
        "/_matrix/client/v1/friends?limit=20&offset=0&sort_by=activity".to_string(),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "fetch friends failed: {body}");

    body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .find(|entry| entry["user_id"].as_str() == Some(target_user_id))
        .cloned()
        .unwrap_or_else(|| panic!("friend entry for {} not found in {}", target_user_id, body))
}

#[tokio::test]
async fn test_v1_and_r0_friend_list_routes_work_after_nesting() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, "friend_routes_list").await;

    for path in [
        "/_matrix/client/v1/friends",
        "/_matrix/client/r0/friendships",
    ] {
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "failed path: {path}");
    }
}

#[tokio::test]
async fn test_r0_friendships_alias_keeps_send_friend_request_validation() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "friend_routes_alias").await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/friendships")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": user_id
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
    assert_eq!(json["error"], "Cannot send friend request to yourself");
}

#[tokio::test]
async fn test_v3_friends_keeps_get_only_contract() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "friend_routes_v3").await;

    let get_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/friends")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let post_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/friends")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": user_id
            })
            .to_string(),
        ))
        .unwrap();

    let post_response = ServiceExt::<Request<Body>>::oneshot(app, post_request)
        .await
        .unwrap();
    assert!(
        post_response.status() == StatusCode::METHOD_NOT_ALLOWED
            || post_response.status() == StatusCode::BAD_REQUEST,
        "Expected 405 or 400 for POST to GET-only route, got: {}",
        post_response.status()
    );
}

#[tokio::test]
async fn test_v3_friend_search_supports_exact_mode() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (searcher_token, _) = register_user(&app, "friend_routes_searcher").await;
    let (_, target_user_id) = register_user(&app, "friend_routes_search_target").await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/friends/search?q=friend_routes_search_target&mode=exact&limit=10")
        .header("Authorization", format!("Bearer {}", searcher_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    if status != StatusCode::OK {
        eprintln!(
            "unexpected search response body: {}",
            String::from_utf8_lossy(&body)
        );
    }
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected search response: {}",
        String::from_utf8_lossy(&body)
    );
    let json: Value = serde_json::from_slice(&body).unwrap();
    let results = json["results"].as_array().expect("results array");
    assert!(results
        .iter()
        .any(|entry| entry["user_id"].as_str() == Some(target_user_id.as_str())));
}

#[tokio::test]
async fn test_v1_friend_list_accepts_pagination_and_sort_params() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, _) = register_user(&app, "friend_routes_page").await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/friends?limit=10&offset=0&sort_by=activity")
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
    assert_eq!(json["limit"].as_u64(), Some(10));
    assert_eq!(json["offset"].as_u64(), Some(0));
    assert!(json.get("friends").is_some());
    assert!(json.get("version").is_some());
}

#[tokio::test]
async fn test_friend_dm_leave_updates_friend_list_state() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (alice_token, alice_user_id, bob_token, bob_user_id, dm_room_id) =
        establish_friend_dm(&app, "friend_leave_alice", "friend_leave_bob").await;

    let (leave_status, leave_body) = json_request(
        &app,
        "POST",
        format!("/_matrix/client/r0/rooms/{}/leave", dm_room_id),
        Some(&bob_token),
        None,
    )
    .await;
    assert_eq!(leave_status, StatusCode::OK, "leave failed: {leave_body}");

    let alice_entry = fetch_friend_entry(&app, &alice_token, &bob_user_id).await;
    let bob_entry = fetch_friend_entry(&app, &bob_token, &alice_user_id).await;

    for entry in [&alice_entry, &bob_entry] {
        assert_eq!(entry["dm_room_id"].as_str(), Some(dm_room_id.as_str()));
        assert_eq!(entry["dm_room_state"].as_str(), Some("left"));
        assert_eq!(entry["dm_room_active"].as_bool(), Some(false));
        assert_eq!(
            entry["dm_room_affected_user_id"].as_str(),
            Some(bob_user_id.as_str())
        );
        assert_eq!(
            entry["dm_room_changed_by"].as_str(),
            Some(bob_user_id.as_str())
        );
    }
}

#[tokio::test]
async fn test_friend_dm_kick_updates_friend_list_state() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_admin_token(&app).await;
    let (alice_token, alice_user_id, bob_token, bob_user_id, dm_room_id) =
        establish_friend_dm(&app, "friend_kick_alice", "friend_kick_bob").await;

    let (kick_status, kick_body) = json_request(
        &app,
        "POST",
        format!("/_synapse/admin/v1/rooms/{}/kick", dm_room_id),
        Some(&admin_token),
        Some(json!({
            "user_id": bob_user_id,
            "reason": "moderation kick"
        })),
    )
    .await;
    assert_eq!(kick_status, StatusCode::OK, "kick failed: {kick_body}");

    let alice_entry = fetch_friend_entry(&app, &alice_token, &bob_user_id).await;
    let bob_entry = fetch_friend_entry(&app, &bob_token, &alice_user_id).await;

    for entry in [&alice_entry, &bob_entry] {
        assert_eq!(entry["dm_room_id"].as_str(), Some(dm_room_id.as_str()));
        assert_eq!(entry["dm_room_state"].as_str(), Some("kicked"));
        assert_eq!(entry["dm_room_active"].as_bool(), Some(false));
        assert_eq!(
            entry["dm_room_affected_user_id"].as_str(),
            Some(bob_user_id.as_str())
        );
        assert!(entry["dm_room_changed_by"].as_str().is_some());
        assert_eq!(entry["dm_room_reason"].as_str(), Some("moderation kick"));
    }
    assert_ne!(alice_user_id, bob_user_id);
}

#[tokio::test]
async fn test_friend_dm_ban_updates_friend_list_state() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_admin_token(&app).await;
    let (alice_token, alice_user_id, bob_token, bob_user_id, dm_room_id) =
        establish_friend_dm(&app, "friend_ban_alice", "friend_ban_bob").await;

    let (ban_status, ban_body) = json_request(
        &app,
        "POST",
        format!("/_synapse/admin/v1/rooms/{}/ban", dm_room_id),
        Some(&admin_token),
        Some(json!({
            "user_id": bob_user_id,
            "reason": "moderation ban"
        })),
    )
    .await;
    assert_eq!(ban_status, StatusCode::OK, "ban failed: {ban_body}");

    let alice_entry = fetch_friend_entry(&app, &alice_token, &bob_user_id).await;
    let bob_entry = fetch_friend_entry(&app, &bob_token, &alice_user_id).await;

    for entry in [&alice_entry, &bob_entry] {
        assert_eq!(entry["dm_room_id"].as_str(), Some(dm_room_id.as_str()));
        assert_eq!(entry["dm_room_state"].as_str(), Some("banned"));
        assert_eq!(entry["dm_room_active"].as_bool(), Some(false));
        assert_eq!(
            entry["dm_room_affected_user_id"].as_str(),
            Some(bob_user_id.as_str())
        );
        assert!(entry["dm_room_changed_by"].as_str().is_some());
        assert_eq!(entry["dm_room_reason"].as_str(), Some("moderation ban"));
    }
}
