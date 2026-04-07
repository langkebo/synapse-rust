use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> axum::Router {
    super::setup_test_app()
        .await
        .unwrap_or_else(|| {
            panic!(
                "Protocol alignment integration tests require isolated schema setup. Start PostgreSQL and apply migrations for local runs."
            )
        })
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let username = format!("{}_{}", username, rand::random::<u32>());
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

    let body = axum::body::to_bytes(response.into_body(), 2048)
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

#[tokio::test]
async fn test_room_summary_sync_populates_members_state_and_stats() {
    let app = setup_test_app().await;

    let (token, user_id) = register_user(&app, "summary_alignment_user").await;
    let room_id = create_room(&app, &token, "Alignment Room").await;

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
    assert!(members
        .iter()
        .any(|member| { member["user_id"] == user_id && member["membership"] == "join" }));

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

    let state_body = axum::body::to_bytes(state_response.into_body(), 8192)
        .await
        .unwrap();
    let state_json: Value = serde_json::from_slice(&state_body).unwrap();
    let state_entries = state_json.as_array().unwrap();
    assert!(!state_entries.is_empty());
    assert!(state_entries.iter().any(|entry| entry["event_type"]
        .as_str()
        .unwrap_or_default()
        .starts_with("m.room.")));

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
    assert!(
        stats_json["total_state_events"]
            .as_i64()
            .unwrap_or_default()
            > 0
    );
}

#[tokio::test]
async fn test_dm_routes_persist_matrix_direct_account_data() {
    let app = setup_test_app().await;

    let (alice_token, alice_id) = register_user(&app, "dm_alignment_alice").await;
    let (_bob_token, bob_id) = register_user(&app, "dm_alignment_bob").await;

    let create_dm_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/create_dm")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": bob_id
            })
            .to_string(),
        ))
        .unwrap();
    let create_dm_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_dm_request)
        .await
        .unwrap();
    assert_eq!(create_dm_response.status(), StatusCode::OK);

    let create_dm_body = axum::body::to_bytes(create_dm_response.into_body(), 2048)
        .await
        .unwrap();
    let create_dm_json: Value = serde_json::from_slice(&create_dm_body).unwrap();
    let room_id = create_dm_json["room_id"].as_str().unwrap().to_string();

    let direct_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/direct")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let direct_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), direct_request)
        .await
        .unwrap();
    assert_eq!(direct_response.status(), StatusCode::OK);

    let direct_body = axum::body::to_bytes(direct_response.into_body(), 4096)
        .await
        .unwrap();
    let direct_json: Value = serde_json::from_slice(&direct_body).unwrap();
    let mapped_rooms = direct_json["rooms"][&bob_id].as_array().unwrap();
    assert!(mapped_rooms
        .iter()
        .any(|room| room == &Value::String(room_id.clone())));

    let account_data_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/user/{}/account_data/m.direct",
            alice_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let account_data_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), account_data_request)
            .await
            .unwrap();
    assert_eq!(account_data_response.status(), StatusCode::OK);

    let account_data_body = axum::body::to_bytes(account_data_response.into_body(), 4096)
        .await
        .unwrap();
    let account_data_json: Value = serde_json::from_slice(&account_data_body).unwrap();
    let account_data_rooms = account_data_json[&bob_id].as_array().unwrap();
    assert!(account_data_rooms
        .iter()
        .any(|room| room == &Value::String(room_id.clone())));

    let dm_check_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/dm", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let dm_check_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), dm_check_request)
        .await
        .unwrap();
    assert_eq!(dm_check_response.status(), StatusCode::OK);

    let dm_partner_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/dm/partner", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let dm_partner_response = ServiceExt::<Request<Body>>::oneshot(app, dm_partner_request)
        .await
        .unwrap();
    assert_eq!(dm_partner_response.status(), StatusCode::OK);

    let dm_partner_body = axum::body::to_bytes(dm_partner_response.into_body(), 4096)
        .await
        .unwrap();
    let dm_partner_json: Value = serde_json::from_slice(&dm_partner_body).unwrap();
    assert_eq!(dm_partner_json["user_id"], bob_id);
}

#[tokio::test]
async fn test_dm_update_accepts_users_array_and_legacy_content_shorthand() {
    let app = setup_test_app().await;

    let (alice_token, alice_id) = register_user(&app, "dm_update_alice").await;
    let (_bob_token, bob_id) = register_user(&app, "dm_update_bob").await;

    let create_dm_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/create_dm")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": bob_id
            })
            .to_string(),
        ))
        .unwrap();
    let create_dm_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_dm_request)
        .await
        .unwrap();
    assert_eq!(create_dm_response.status(), StatusCode::OK);

    let create_dm_body = axum::body::to_bytes(create_dm_response.into_body(), 2048)
        .await
        .unwrap();
    let create_dm_json: Value = serde_json::from_slice(&create_dm_body).unwrap();
    let room_id = create_dm_json["room_id"].as_str().unwrap().to_string();

    let update_array_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/direct/{}", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": [bob_id]
            })
            .to_string(),
        ))
        .unwrap();
    let update_array_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), update_array_request)
            .await
            .unwrap();
    assert_eq!(update_array_response.status(), StatusCode::OK);

    let update_legacy_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/direct/{}", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "content": {
                    "user_id": bob_id
                }
            })
            .to_string(),
        ))
        .unwrap();
    let update_legacy_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), update_legacy_request)
            .await
            .unwrap();
    assert_eq!(update_legacy_response.status(), StatusCode::OK);

    let account_data_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/user/{}/account_data/m.direct",
            alice_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let account_data_response = ServiceExt::<Request<Body>>::oneshot(app, account_data_request)
        .await
        .unwrap();
    assert_eq!(account_data_response.status(), StatusCode::OK);

    let account_data_body = axum::body::to_bytes(account_data_response.into_body(), 4096)
        .await
        .unwrap();
    let account_data_json: Value = serde_json::from_slice(&account_data_body).unwrap();
    let account_data_rooms = account_data_json[&bob_id].as_array().unwrap();
    let expected_room_id = Value::String(room_id);
    assert!(account_data_rooms
        .iter()
        .any(|room| room == &expected_room_id));
}

#[tokio::test]
async fn test_admin_room_search_enforces_matrix_forbidden_and_handles_special_terms() {
    let app = setup_test_app().await;

    let (user_token, _) = register_user(&app, "room_search_non_admin").await;
    let forbidden_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/rooms/search")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_term": "anything"
            })
            .to_string(),
        ))
        .unwrap();
    let forbidden_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_request)
        .await
        .unwrap();
    assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);

    let forbidden_body = axum::body::to_bytes(forbidden_response.into_body(), 4096)
        .await
        .unwrap();
    let forbidden_json: Value = serde_json::from_slice(&forbidden_body).unwrap();
    assert_eq!(forbidden_json["errcode"], "M_FORBIDDEN");

    let (admin_token, _) = super::get_admin_token(&app).await;
    let (creator_token, _) = register_user(&app, "room_search_creator").await;
    let _room_id = create_room(&app, &creator_token, "Searchable Room").await;

    let special_term_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/rooms/search")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "search_term": "Searchable' OR 1=1 --",
                "is_public": false,
                "is_encrypted": false
            })
            .to_string(),
        ))
        .unwrap();
    let special_term_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), special_term_request)
            .await
            .unwrap();
    assert_eq!(special_term_response.status(), StatusCode::OK);

    let special_term_body = axum::body::to_bytes(special_term_response.into_body(), 4096)
        .await
        .unwrap();
    let special_term_json: Value = serde_json::from_slice(&special_term_body).unwrap();
    assert!(special_term_json["results"].is_array());
    assert_eq!(
        special_term_json["count"].as_u64().unwrap(),
        special_term_json["results"].as_array().unwrap().len() as u64
    );
}

#[tokio::test]
async fn test_space_state_and_children_form_a_matrix_style_closure() {
    let app = setup_test_app().await;

    let (token, _user_id) = register_user(&app, "space_alignment_owner").await;
    let parent_room_id = create_room(&app, &token, "Parent Space Room").await;
    let child_room_id = create_room(&app, &token, "Child Space Room").await;

    let create_space_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/spaces")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": parent_room_id,
                "name": "Knowledge Space",
                "topic": "Aligned state",
                "join_rule": "invite",
                "visibility": "private",
                "is_public": false
            })
            .to_string(),
        ))
        .unwrap();
    let create_space_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_space_request)
            .await
            .unwrap();
    assert_eq!(create_space_response.status(), StatusCode::CREATED);

    let create_space_body = axum::body::to_bytes(create_space_response.into_body(), 4096)
        .await
        .unwrap();
    let create_space_json: Value = serde_json::from_slice(&create_space_body).unwrap();
    let space_id = create_space_json["space_id"].as_str().unwrap().to_string();

    let add_child_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/children", space_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": child_room_id,
                "via_servers": ["localhost"],
                "suggested": true
            })
            .to_string(),
        ))
        .unwrap();
    let add_child_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), add_child_request)
        .await
        .unwrap();
    if add_child_response.status() != StatusCode::CREATED {
        let body = axum::body::to_bytes(add_child_response.into_body(), 8192)
            .await
            .unwrap();
        panic!("add_child failed: {:?}", String::from_utf8_lossy(&body));
    }

    let update_space_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/spaces/{}", space_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Knowledge Space Updated",
                "topic": "Aligned state updated"
            })
            .to_string(),
        ))
        .unwrap();
    let update_space_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), update_space_request)
            .await
            .unwrap();
    assert_eq!(update_space_response.status(), StatusCode::OK);

    let state_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/spaces/{}/state", space_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let state_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), state_request)
        .await
        .unwrap();
    assert_eq!(state_response.status(), StatusCode::OK);

    let state_body = axum::body::to_bytes(state_response.into_body(), 8192)
        .await
        .unwrap();
    let state_json: Value = serde_json::from_slice(&state_body).unwrap();
    let state_entries = state_json.as_array().unwrap();
    assert!(state_entries.iter().any(|entry| {
        entry["type"] == "m.room.name" && entry["content"]["name"] == "Knowledge Space Updated"
    }));
    assert!(state_entries
        .iter()
        .any(|entry| { entry["type"] == "m.space.child" && entry["state_key"] == child_room_id }));

    let children_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/spaces/{}/children", space_id))
        .body(Body::empty())
        .unwrap();
    let children_response = ServiceExt::<Request<Body>>::oneshot(app, children_request)
        .await
        .unwrap();
    assert_eq!(children_response.status(), StatusCode::OK);

    let children_body = axum::body::to_bytes(children_response.into_body(), 4096)
        .await
        .unwrap();
    let children_json: Value = serde_json::from_slice(&children_body).unwrap();
    let children = children_json.as_array().unwrap();
    assert!(children
        .iter()
        .any(|entry| entry["room_id"] == child_room_id));
}

#[tokio::test]
async fn test_space_search_accepts_query_and_search_term_alias() {
    let app = setup_test_app().await;

    let (token, _user_id) = register_user(&app, "space_search_owner").await;
    let parent_room_id = create_room(&app, &token, "Search Alias Parent Room").await;

    let create_space_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/spaces")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": parent_room_id,
                "name": "Alias Search Space",
                "topic": "Search alias coverage",
                "join_rule": "invite",
                "visibility": "private",
                "is_public": false
            })
            .to_string(),
        ))
        .unwrap();
    let create_space_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_space_request)
            .await
            .unwrap();
    assert_eq!(create_space_response.status(), StatusCode::CREATED);

    let query_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/spaces/search?query=Alias")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let query_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), query_request)
        .await
        .unwrap();
    assert_eq!(query_response.status(), StatusCode::OK);

    let query_body = axum::body::to_bytes(query_response.into_body(), 4096)
        .await
        .unwrap();
    let query_json: Value = serde_json::from_slice(&query_body).unwrap();
    let query_entries = query_json.as_array().unwrap();
    assert!(query_entries
        .iter()
        .any(|entry| entry["name"] == "Alias Search Space"));

    let alias_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/spaces/search?search_term=Alias")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let alias_response = ServiceExt::<Request<Body>>::oneshot(app, alias_request)
        .await
        .unwrap();
    assert_eq!(alias_response.status(), StatusCode::OK);

    let alias_body = axum::body::to_bytes(alias_response.into_body(), 4096)
        .await
        .unwrap();
    let alias_json: Value = serde_json::from_slice(&alias_body).unwrap();
    let alias_entries = alias_json.as_array().unwrap();
    assert!(alias_entries
        .iter()
        .any(|entry| entry["name"] == "Alias Search Space"));
}

#[tokio::test]
async fn test_admin_pusher_query_requires_existing_user_and_returns_created_pushers() {
    let app = setup_test_app().await;

    let (token, user_id) = register_user(&app, "pusher_alignment_user").await;
    let (admin_token, _) = super::get_admin_token(&app).await;

    let set_pusher_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/pushers/set")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "pushkey": "pushkey-alignment",
                "kind": "http",
                "app_id": "com.example.alignment",
                "app_display_name": "Alignment Push",
                "device_display_name": "Alignment Device",
                "lang": "en",
                "data": {
                    "url": "https://push.example.test/_matrix/push/v1/notify"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let set_pusher_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), set_pusher_request)
        .await
        .unwrap();
    assert_eq!(set_pusher_response.status(), StatusCode::OK);

    let get_pushers_request = Request::builder()
        .method("GET")
        .uri(format!("/_synapse/admin/v1/users/{}/pushers", user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let get_pushers_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), get_pushers_request)
            .await
            .unwrap();
    assert_eq!(get_pushers_response.status(), StatusCode::OK);

    let get_pushers_body = axum::body::to_bytes(get_pushers_response.into_body(), 4096)
        .await
        .unwrap();
    let get_pushers_json: Value = serde_json::from_slice(&get_pushers_body).unwrap();
    assert_eq!(get_pushers_json["total"], 1);
    assert_eq!(
        get_pushers_json["pushers"][0]["pushkey"],
        "pushkey-alignment"
    );

    let missing_user_request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/users/@missing:localhost/pushers")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let missing_user_response = ServiceExt::<Request<Body>>::oneshot(app, missing_user_request)
        .await
        .unwrap();
    assert_eq!(missing_user_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_send_server_notice_persists_notice_for_target_user() {
    let app = setup_test_app().await;

    let (_, user_id) = register_user(&app, "server_notice_target").await;
    let (admin_token, _) = super::get_admin_token(&app).await;

    let send_notice_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/send_server_notice")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": user_id,
                "content": {
                    "msgtype": "m.text",
                    "body": "Alignment notice"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let send_notice_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), send_notice_request)
            .await
            .unwrap();
    assert_eq!(send_notice_response.status(), StatusCode::OK);

    let send_notice_body = axum::body::to_bytes(send_notice_response.into_body(), 4096)
        .await
        .unwrap();
    let send_notice_json: Value = serde_json::from_slice(&send_notice_body).unwrap();
    assert!(send_notice_json["event_id"].as_str().is_some());
    assert!(send_notice_json["room_id"].as_str().is_some());
    let notice_id = send_notice_json["notice_id"].as_i64().unwrap();

    let list_notices_request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/server_notices")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let list_notices_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), list_notices_request)
            .await
            .unwrap();
    assert_eq!(list_notices_response.status(), StatusCode::OK);

    let list_notices_body = axum::body::to_bytes(list_notices_response.into_body(), 4096)
        .await
        .unwrap();
    let list_notices_json: Value = serde_json::from_slice(&list_notices_body).unwrap();
    assert!(list_notices_json["total"].as_u64().unwrap() >= 1);
    assert!(list_notices_json["notices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|notice| {
            notice["id"] == notice_id
                && notice["user_id"] == user_id
                && notice["event_id"] == send_notice_json["event_id"]
        }));
}
