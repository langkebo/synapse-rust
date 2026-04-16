use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn register_user(app: &axum::Router, username: &str) -> String {
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

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
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

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn get_user_id(app: &axum::Router, token: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/account/whoami")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["user_id"].as_str().unwrap().to_string()
}

async fn create_space(app: &axum::Router, token: &str, room_id: &str, body: Value) -> Value {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/spaces")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    json
}

async fn add_child(
    app: &axum::Router,
    token: &str,
    space_room_id: &str,
    child_room_id: &str,
    suggested: bool,
) {
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/spaces/{}/children",
            space_room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": child_room_id,
                "via_servers": ["localhost"],
                "suggested": suggested
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn setup_test_app_with_pool() -> Option<(axum::Router, std::sync::Arc<sqlx::PgPool>)> {
    use synapse_rust::cache::CacheManager;
    use synapse_rust::services::ServiceContainer;
    use synapse_rust::web::routes::state::AppState;

    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool.clone());
    let cache = std::sync::Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);

    Some((synapse_rust::web::create_router(state), pool))
}

#[tokio::test]
async fn test_space_summary_suite_keeps_summary_counts_and_child_projection_verified() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let token = register_user(&app, "space_summary_suite").await;
    let root_room_id = create_room(&app, &token, "Space Summary Root").await;
    let child_room_id = create_room(&app, &token, "Space Summary Child").await;

    let root_space = create_space(
        &app,
        &token,
        &root_room_id,
        json!({
            "room_id": root_room_id,
            "name": "Space Summary Root",
            "topic": "summary route sample",
            "join_rule": "invite",
            "visibility": "private",
            "is_public": false
        }),
    )
    .await;
    let root_space_id = root_space["space_id"].as_str().unwrap().to_string();
    add_child(&app, &token, &root_room_id, &child_room_id, true).await;

    for path in [
        format!("/_matrix/client/v3/spaces/{}/summary", root_space_id),
        format!("/_matrix/client/r0/spaces/{}/summary", root_space_id),
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
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["children_count"], 1);
        assert_eq!(json["member_count"], 1);
    }

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v1/spaces/{}/summary/with_children",
            root_space_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_type"], "m.space");
    assert_eq!(json["num_joined_members"], 1);
    assert_eq!(json["children"][0]["room_id"], child_room_id);
    assert_eq!(json["children_state"][0]["state_key"], child_room_id);
    assert_eq!(json["children_state"][0]["content"]["suggested"], true);
}

#[tokio::test]
async fn test_space_children_hierarchy_suite_keeps_nested_chain_verified() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let token = register_user(&app, "space_hierarchy_suite").await;
    let root_room_id = create_room(&app, &token, "Hierarchy Root Space").await;
    let child_space_room_id = create_room(&app, &token, "Hierarchy Child Space").await;
    let leaf_room_id = create_room(&app, &token, "Hierarchy Leaf Room").await;

    let root_space = create_space(
        &app,
        &token,
        &root_room_id,
        json!({
            "room_id": root_room_id,
            "name": "Hierarchy Root Space",
            "join_rule": "invite",
            "visibility": "private",
            "is_public": false
        }),
    )
    .await;
    let root_space_id = root_space["space_id"].as_str().unwrap().to_string();
    let child_space = create_space(
        &app,
        &token,
        &child_space_room_id,
        json!({
            "room_id": child_space_room_id,
            "name": "Hierarchy Child Space",
            "join_rule": "public",
            "visibility": "public",
            "is_public": true
        }),
    )
    .await;
    let _child_space_id = child_space["space_id"].as_str().unwrap().to_string();
    add_child(&app, &token, &root_room_id, &child_space_room_id, true).await;
    add_child(&app, &token, &child_space_room_id, &leaf_room_id, false).await;

    let children_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/spaces/{}/children",
            root_space_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let children_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), children_request)
        .await
        .unwrap();
    assert_eq!(children_response.status(), StatusCode::OK);

    let children_body = axum::body::to_bytes(children_response.into_body(), 8192)
        .await
        .unwrap();
    let children_json: Value = serde_json::from_slice(&children_body).unwrap();
    assert_eq!(children_json[0]["space_id"], root_space_id);
    assert_eq!(children_json[0]["room_id"], child_space_room_id);
    assert_eq!(children_json[0]["is_suggested"], true);

    let hierarchy_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v1/spaces/{}/hierarchy/v1?max_depth=3&limit=1",
            root_space_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let hierarchy_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), hierarchy_request)
        .await
        .unwrap();
    assert_eq!(hierarchy_response.status(), StatusCode::OK);

    let hierarchy_body = axum::body::to_bytes(hierarchy_response.into_body(), 16384)
        .await
        .unwrap();
    let hierarchy_json: Value = serde_json::from_slice(&hierarchy_body).unwrap();
    assert_eq!(hierarchy_json["rooms"][0]["room_id"], child_space_room_id);
    assert_eq!(hierarchy_json["rooms"][0]["room_type"], "m.space");
    assert_eq!(hierarchy_json["rooms"][0]["world_readable"], true);
    assert_eq!(hierarchy_json["rooms"][0]["guest_can_join"], true);
    assert_eq!(
        hierarchy_json["rooms"][0]["children_state"][0]["state_key"],
        leaf_room_id
    );
    assert_eq!(hierarchy_json["next_batch"], leaf_room_id);

    let parents_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/spaces/room/{}/parents",
            leaf_room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let parents_response = ServiceExt::<Request<Body>>::oneshot(app, parents_request)
        .await
        .unwrap();
    assert_eq!(parents_response.status(), StatusCode::OK);

    let parents_body = axum::body::to_bytes(parents_response.into_body(), 8192)
        .await
        .unwrap();
    let parents_json: Value = serde_json::from_slice(&parents_body).unwrap();
    assert_eq!(parents_json[0]["room_id"], child_space_room_id);
}

#[tokio::test]
async fn test_space_membership_state_suite_keeps_invite_join_leave_closure_verified() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner_token = register_user(&app, "space_membership_owner").await;
    let guest_token = register_user(&app, "space_membership_guest").await;
    let guest_user_id = get_user_id(&app, &guest_token).await;
    let root_room_id = create_room(&app, &owner_token, "Membership Root Space").await;
    let child_room_id = create_room(&app, &owner_token, "Membership Child Room").await;

    let root_space = create_space(
        &app,
        &owner_token,
        &root_room_id,
        json!({
            "room_id": root_room_id,
            "name": "Membership Root Space",
            "topic": "membership closure",
            "join_rule": "invite",
            "visibility": "private",
            "is_public": false
        }),
    )
    .await;
    let root_space_id = root_space["space_id"].as_str().unwrap().to_string();
    add_child(&app, &owner_token, &root_room_id, &child_room_id, true).await;

    let state_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/spaces/{}/state", root_space_id))
        .header("Authorization", format!("Bearer {}", owner_token))
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
        entry["type"] == "m.room.create" && entry["content"]["room_type"] == "m.space"
    }));
    assert!(state_entries
        .iter()
        .any(|entry| { entry["type"] == "m.space.child" && entry["state_key"] == child_room_id }));

    let members_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/spaces/{}/members",
            root_space_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let members_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), members_request)
        .await
        .unwrap();
    assert_eq!(members_response.status(), StatusCode::OK);

    let members_body = axum::body::to_bytes(members_response.into_body(), 8192)
        .await
        .unwrap();
    let members_json: Value = serde_json::from_slice(&members_body).unwrap();
    let members = members_json.as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["membership"], "join");

    let rooms_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/spaces/{}/rooms", root_space_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let rooms_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), rooms_request)
        .await
        .unwrap();
    assert_eq!(rooms_response.status(), StatusCode::OK);

    let rooms_body = axum::body::to_bytes(rooms_response.into_body(), 8192)
        .await
        .unwrap();
    let rooms_json: Value = serde_json::from_slice(&rooms_body).unwrap();
    assert_eq!(rooms_json["space_id"], root_space_id);
    assert_eq!(rooms_json["rooms"][0], child_room_id);

    let forbidden_join_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/join", root_space_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let forbidden_join_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), forbidden_join_request)
            .await
            .unwrap();
    assert_eq!(forbidden_join_response.status(), StatusCode::FORBIDDEN);

    let invite_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/invite", root_space_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": guest_user_id
            })
            .to_string(),
        ))
        .unwrap();
    let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request)
        .await
        .unwrap();
    assert_eq!(invite_response.status(), StatusCode::CREATED);

    let join_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/join", root_space_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let join_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), join_request)
        .await
        .unwrap();
    assert_eq!(join_response.status(), StatusCode::OK);

    let joined_members_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/spaces/{}/members",
            root_space_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let joined_members_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), joined_members_request)
            .await
            .unwrap();
    assert_eq!(joined_members_response.status(), StatusCode::OK);

    let joined_members_body = axum::body::to_bytes(joined_members_response.into_body(), 8192)
        .await
        .unwrap();
    let joined_members_json: Value = serde_json::from_slice(&joined_members_body).unwrap();
    let joined_members = joined_members_json.as_array().unwrap();
    assert_eq!(joined_members.len(), 2);
    assert!(joined_members
        .iter()
        .any(|member| member["user_id"] == guest_user_id && member["membership"] == "join"));

    let leave_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v1/spaces/{}/leave", root_space_id))
        .header("Authorization", format!("Bearer {}", guest_token))
        .body(Body::empty())
        .unwrap();
    let leave_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), leave_request)
        .await
        .unwrap();
    assert_eq!(leave_response.status(), StatusCode::NO_CONTENT);

    let members_after_leave_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/spaces/{}/members",
            root_space_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let members_after_leave_response =
        ServiceExt::<Request<Body>>::oneshot(app, members_after_leave_request)
            .await
            .unwrap();
    assert_eq!(members_after_leave_response.status(), StatusCode::OK);

    let members_after_leave_body =
        axum::body::to_bytes(members_after_leave_response.into_body(), 8192)
            .await
            .unwrap();
    let members_after_leave_json: Value =
        serde_json::from_slice(&members_after_leave_body).unwrap();
    let members_after_leave = members_after_leave_json.as_array().unwrap();
    assert_eq!(members_after_leave.len(), 1);
    assert!(!members_after_leave
        .iter()
        .any(|member| member["user_id"] == guest_user_id));
}

#[tokio::test]
async fn test_space_lifecycle_query_suite_keeps_create_update_lookup_and_delete_verified() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let token = register_user(&app, "space_lifecycle_owner").await;
    let room_id = create_room(&app, &token, "Lifecycle Space Room").await;

    let create_json = create_space(
        &app,
        &token,
        &room_id,
        json!({
            "room_id": room_id,
            "name": "Lifecycle Space",
            "topic": "lifecycle query sample",
            "join_rule": "public",
            "visibility": "public",
            "is_public": true
        }),
    )
    .await;
    let space_id = create_json["space_id"].as_str().unwrap().to_string();
    assert_ne!(space_id, room_id);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/spaces/{}", space_id))
        .body(Body::empty())
        .unwrap();
    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let get_body = axum::body::to_bytes(get_response.into_body(), 8192)
        .await
        .unwrap();
    let get_json: Value = serde_json::from_slice(&get_body).unwrap();
    assert_eq!(get_json["space_id"], space_id);
    assert_eq!(get_json["room_id"], room_id);
    assert_eq!(get_json["name"], "Lifecycle Space");
    assert_eq!(get_json["is_public"], true);

    let by_room_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/spaces/room/{}", room_id))
        .body(Body::empty())
        .unwrap();
    let by_room_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), by_room_request)
        .await
        .unwrap();
    assert_eq!(by_room_response.status(), StatusCode::OK);

    let by_room_body = axum::body::to_bytes(by_room_response.into_body(), 8192)
        .await
        .unwrap();
    let by_room_json: Value = serde_json::from_slice(&by_room_body).unwrap();
    assert_eq!(by_room_json["space_id"], space_id);
    assert_eq!(by_room_json["room_id"], room_id);

    let update_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/spaces/{}", space_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Lifecycle Space Updated",
                "topic": "updated lifecycle query sample"
            })
            .to_string(),
        ))
        .unwrap();
    let update_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), update_request)
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let update_body = axum::body::to_bytes(update_response.into_body(), 8192)
        .await
        .unwrap();
    let update_json: Value = serde_json::from_slice(&update_body).unwrap();
    assert_eq!(update_json["space_id"], space_id);
    assert_eq!(update_json["name"], "Lifecycle Space Updated");
    assert_eq!(update_json["topic"], "updated lifecycle query sample");

    let user_spaces_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/spaces/user")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let user_spaces_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), user_spaces_request)
            .await
            .unwrap();
    assert_eq!(user_spaces_response.status(), StatusCode::OK);

    let user_spaces_body = axum::body::to_bytes(user_spaces_response.into_body(), 8192)
        .await
        .unwrap();
    let user_spaces_json: Value = serde_json::from_slice(&user_spaces_body).unwrap();
    let user_spaces = user_spaces_json.as_array().unwrap();
    assert!(user_spaces.iter().any(|space| {
        space["room_id"] == room_id && space["name"] == "Lifecycle Space Updated"
    }));

    let public_spaces_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v1/spaces/public?limit=20&offset=0")
        .body(Body::empty())
        .unwrap();
    let public_spaces_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), public_spaces_request)
            .await
            .unwrap();
    assert_eq!(public_spaces_response.status(), StatusCode::OK);

    let public_spaces_body = axum::body::to_bytes(public_spaces_response.into_body(), 8192)
        .await
        .unwrap();
    let public_spaces_json: Value = serde_json::from_slice(&public_spaces_body).unwrap();
    let public_spaces = public_spaces_json.as_array().unwrap();
    assert!(public_spaces
        .iter()
        .any(|space| space["room_id"] == room_id && space["is_public"] == true));

    let search_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/spaces/search?query=Lifecycle%20Space%20Updated")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let search_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), search_request)
        .await
        .unwrap();
    assert_eq!(search_response.status(), StatusCode::OK);

    let search_body = axum::body::to_bytes(search_response.into_body(), 8192)
        .await
        .unwrap();
    let search_json: Value = serde_json::from_slice(&search_body).unwrap();
    let search_results = search_json.as_array().unwrap();
    assert!(search_results
        .iter()
        .any(|space| space["room_id"] == room_id && space["name"] == "Lifecycle Space Updated"));

    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/client/v3/spaces/{}", space_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);
    let get_after_delete_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/spaces/{}", space_id))
        .body(Body::empty())
        .unwrap();
    let get_after_delete_response =
        ServiceExt::<Request<Body>>::oneshot(app, get_after_delete_request)
            .await
            .unwrap();
    assert_eq!(get_after_delete_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_private_space_read_routes_reject_anonymous_and_non_members() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner_token = register_user(&app, "space_read_guard_owner").await;
    let outsider_token = register_user(&app, "space_read_guard_outsider").await;
    let root_room_id = create_room(&app, &owner_token, "Private Read Guard Space").await;

    let root_space = create_space(
        &app,
        &owner_token,
        &root_room_id,
        json!({
            "room_id": root_room_id,
            "name": "Private Read Guard Space",
            "topic": "visibility guard",
            "join_rule": "invite",
            "visibility": "private",
            "is_public": false
        }),
    )
    .await;
    let root_space_id = root_space["space_id"].as_str().unwrap().to_string();

    for path in [
        format!("/_matrix/client/v1/spaces/{}", root_space_id),
        format!(
            "/_matrix/client/v1/spaces/{}/summary/with_children",
            root_space_id
        ),
        format!("/_matrix/client/v1/spaces/{}/members", root_space_id),
        format!("/_matrix/client/v1/spaces/{}/hierarchy/v1", root_space_id),
    ] {
        let anonymous_request = Request::builder()
            .method("GET")
            .uri(&path)
            .body(Body::empty())
            .unwrap();
        let anonymous_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), anonymous_request)
                .await
                .unwrap();
        assert_eq!(anonymous_response.status(), StatusCode::UNAUTHORIZED);

        let outsider_request = Request::builder()
            .method("GET")
            .uri(&path)
            .header("Authorization", format!("Bearer {}", outsider_token))
            .body(Body::empty())
            .unwrap();
        let outsider_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), outsider_request)
            .await
            .unwrap();
        assert_eq!(outsider_response.status(), StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn test_space_statistics_only_returns_visible_spaces() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };

    let owner_token = register_user(&app, "space_stats_owner").await;
    let outsider_token = register_user(&app, "space_stats_outsider").await;
    let public_room_id = create_room(&app, &owner_token, "Stats Public Space").await;
    let private_room_id = create_room(&app, &owner_token, "Stats Private Space").await;

    create_space(
        &app,
        &owner_token,
        &public_room_id,
        json!({
            "room_id": public_room_id,
            "name": "Stats Public Space",
            "join_rule": "public",
            "visibility": "public",
            "is_public": true
        }),
    )
    .await;
    create_space(
        &app,
        &owner_token,
        &private_room_id,
        json!({
            "room_id": private_room_id,
            "name": "Stats Private Space",
            "join_rule": "invite",
            "visibility": "private",
            "is_public": false
        }),
    )
    .await;

    let public_space_id: String = sqlx::query_scalar("SELECT space_id FROM spaces WHERE room_id = $1")
        .bind(&public_room_id)
        .fetch_one(&*pool)
        .await
        .unwrap();
    let private_space_id: String =
        sqlx::query_scalar("SELECT space_id FROM spaces WHERE room_id = $1")
            .bind(&private_room_id)
            .fetch_one(&*pool)
            .await
            .unwrap();

    for (space_id, name, is_public) in [
        (public_space_id.as_str(), "Stats Public Space", true),
        (private_space_id.as_str(), "Stats Private Space", false),
    ] {
        sqlx::query(
            r#"
            INSERT INTO space_statistics (space_id, name, is_public, child_room_count, member_count, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (space_id) DO UPDATE SET
                name = EXCLUDED.name,
                is_public = EXCLUDED.is_public,
                child_room_count = EXCLUDED.child_room_count,
                member_count = EXCLUDED.member_count,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(space_id)
        .bind(name)
        .bind(is_public)
        .bind(0_i64)
        .bind(1_i64)
        .bind(0_i64)
        .bind(100_i64)
        .execute(&*pool)
        .await
        .unwrap();
    }

    let anonymous_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/spaces/statistics")
        .body(Body::empty())
        .unwrap();
    let anonymous_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), anonymous_request)
        .await
        .unwrap();
    assert_eq!(anonymous_response.status(), StatusCode::OK);

    let anonymous_body = axum::body::to_bytes(anonymous_response.into_body(), 8192)
        .await
        .unwrap();
    let anonymous_json: Value = serde_json::from_slice(&anonymous_body).unwrap();
    let anonymous_stats = anonymous_json.as_array().unwrap();
    assert!(anonymous_stats
        .iter()
        .any(|entry| entry["name"] == "Stats Public Space"));
    assert!(!anonymous_stats
        .iter()
        .any(|entry| entry["name"] == "Stats Private Space"));

    let outsider_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/spaces/statistics")
        .header("Authorization", format!("Bearer {}", outsider_token))
        .body(Body::empty())
        .unwrap();
    let outsider_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), outsider_request)
        .await
        .unwrap();
    assert_eq!(outsider_response.status(), StatusCode::OK);

    let outsider_body = axum::body::to_bytes(outsider_response.into_body(), 8192)
        .await
        .unwrap();
    let outsider_json: Value = serde_json::from_slice(&outsider_body).unwrap();
    let outsider_stats = outsider_json.as_array().unwrap();
    assert!(outsider_stats
        .iter()
        .any(|entry| entry["name"] == "Stats Public Space"));
    assert!(!outsider_stats
        .iter()
        .any(|entry| entry["name"] == "Stats Private Space"));

    let owner_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/spaces/statistics")
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let owner_response = ServiceExt::<Request<Body>>::oneshot(app, owner_request)
        .await
        .unwrap();
    assert_eq!(owner_response.status(), StatusCode::OK);

    let owner_body = axum::body::to_bytes(owner_response.into_body(), 8192)
        .await
        .unwrap();
    let owner_json: Value = serde_json::from_slice(&owner_body).unwrap();
    let owner_stats = owner_json.as_array().unwrap();
    assert!(owner_stats
        .iter()
        .any(|entry| entry["name"] == "Stats Public Space"));
    assert!(owner_stats
        .iter()
        .any(|entry| entry["name"] == "Stats Private Space"));
}

#[tokio::test]
async fn test_create_space_route_rejects_non_creator_for_foreign_room() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner_token = register_user(&app, "space_foreign_room_owner").await;
    let outsider_token = register_user(&app, "space_foreign_room_outsider").await;
    let room_id = create_room(&app, &owner_token, "Foreign Room Space").await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/spaces")
        .header("Authorization", format!("Bearer {}", outsider_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": room_id,
                "name": "Unauthorized Space",
                "join_rule": "public",
                "visibility": "public",
                "is_public": true
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
async fn test_space_shared_write_routes_reject_joined_non_creator() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };

    let owner_token = register_user(&app, "space_write_owner").await;
    let member_token = register_user(&app, "space_write_member").await;
    let target_token = register_user(&app, "space_write_target").await;
    let target_user_id = get_user_id(&app, &target_token).await;
    let room_id = create_room(&app, &owner_token, "Shared Write Space").await;
    let child_room_id = create_room(&app, &owner_token, "Shared Write Child").await;

    let space = create_space(
        &app,
        &owner_token,
        &room_id,
        json!({
            "room_id": room_id,
            "name": "Shared Write Space",
            "join_rule": "public",
            "visibility": "public",
            "is_public": true
        }),
    )
    .await;
    let space_id = space["space_id"].as_str().unwrap().to_string();

    let join_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/join", space_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let join_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), join_request)
        .await
        .unwrap();
    assert_eq!(join_response.status(), StatusCode::OK);

    add_child(&app, &owner_token, &room_id, &child_room_id, true).await;

    let update_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/spaces/{}", space_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Member Mutated Space"
            })
            .to_string(),
        ))
        .unwrap();
    let update_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), update_request)
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::FORBIDDEN);

    let add_child_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/children", space_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": child_room_id,
                "via_servers": ["localhost"],
                "suggested": false
            })
            .to_string(),
        ))
        .unwrap();
    let add_child_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), add_child_request)
        .await
        .unwrap();
    assert_eq!(add_child_response.status(), StatusCode::FORBIDDEN);

    let remove_child_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_matrix/client/v3/spaces/{}/children/{}",
            space_id, child_room_id
        ))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let remove_child_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), remove_child_request)
            .await
            .unwrap();
    assert_eq!(remove_child_response.status(), StatusCode::FORBIDDEN);

    let invite_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/invite", space_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": target_user_id
            })
            .to_string(),
        ))
        .unwrap();
    let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request)
        .await
        .unwrap();
    assert_eq!(invite_response.status(), StatusCode::FORBIDDEN);

    let owner_update_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/spaces/{}", space_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Owner Mutated Space"
            })
            .to_string(),
        ))
        .unwrap();
    let owner_update_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), owner_update_request)
            .await
            .unwrap();
    assert_eq!(owner_update_response.status(), StatusCode::OK);
}
