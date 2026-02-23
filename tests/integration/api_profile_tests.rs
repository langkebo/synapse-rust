use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "password123",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    if response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        println!("Register failed: {:?}", body);
        panic!("Register failed");
    }

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_profile_validation_fixes() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "test_profile").await;

    // Test 1: Update displayname with too long string
    let long_displayname = "a".repeat(256);
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/account/profile/{}/displayname", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "displayname": long_displayname
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
    assert_eq!(json["error"], "Displayname too long (max 255 characters)");

    // Test 2: Update avatar_url with too long string
    let long_avatar_url = "http://example.com/".to_string() + &"a".repeat(250); // Total > 255
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/account/profile/{}/avatar_url", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "avatar_url": long_avatar_url
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
    assert_eq!(json["error"], "Avatar URL too long (max 255 characters)");

    // Test 3: Get profile with invalid user_id format
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/account/profile/invalid_user_id")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test 4: Update displayname for non-existent user
    // First, we need an admin token to bypass the "auth_user.user_id != user_id" check
    // Or we can try to update another user's profile with our token and expect Forbidden, 
    // but here we want to test "User not found" which requires bypassing the first check or having a valid token for that user?
    // Actually, if we use our token to update another user, we get Forbidden first.
    // If we are admin, we pass the first check, then we hit user_exists check.
    
    // Let's try update non-existent user with admin rights.
    // We need to create an admin user first. But setup_test_app uses a fresh DB/services.
    // We can register another user and manually promote to admin if we had access to DB, 
    // but here we only have API. 
    // Wait, the `register` endpoint allows creating admin if we know the shared secret, 
    // but `register_user` helper uses standard flow.
    
    // Let's just test "Update another user's profile" -> Forbidden.
    let other_user_id = "@other:localhost";
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/account/profile/{}/displayname", other_user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "displayname": "New Name"
            })
            .to_string(),
        ))
        .unwrap();
    
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Test 5: Get profile for non-existent user (should be Not Found, but after validation passes)
    let non_existent_user = "@nonexistent:localhost";
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/account/profile/{}", non_existent_user))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
