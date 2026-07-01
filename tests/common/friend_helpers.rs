//! Shared helpers for friend-module integration tests.
//!
//! Provides both service-layer helpers (operating on `ServiceContainer` directly)
//! and HTTP-layer helpers (operating on an `axum::Router` via `tower::ServiceExt`).

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_cache::{CacheConfig, CacheManager};
use synapse_services::ServiceContainer;

/// Monotonic counter used to generate unique usernames across parallel tests.
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Returns a unique numeric suffix to avoid username collisions between tests.
pub fn unique_suffix() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Builds a fresh `ServiceContainer` backed by an isolated test schema.
///
/// Returns `None` when the test database is unavailable (tests then early-return).
pub async fn setup_fresh_container() -> Option<ServiceContainer> {
    let pool = synapse_rust::test_utils::prepare_isolated_test_pool().await.ok()?;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    Some(ServiceContainer::new_test_with_pool_and_cache(pool, cache).await)
}

/// Registers a user via the service layer and returns its `user_id`.
pub async fn register_user(container: &ServiceContainer, username: &str, display_name: &str) -> String {
    let (user, _, _, _) = container
        .core
        .auth_service
        .register(username, "Test@123", false, Some(display_name))
        .await
        .expect("register test user");
    user.user_id
}

/// Registers two users and establishes a bidirectional friendship between them.
///
/// Returns `(alice_user_id, bob_user_id)`.
pub async fn establish_friendship(
    container: &ServiceContainer,
    alice_username: &str,
    bob_username: &str,
) -> (String, String) {
    let alice = register_user(container, alice_username, "Alice").await;
    let bob = register_user(container, bob_username, "Bob").await;
    establish_friendship_between(container, &alice, &bob).await;
    (alice, bob)
}

/// Establishes a bidirectional friendship between two already-registered users.
pub async fn establish_friendship_between(container: &ServiceContainer, alice_user_id: &str, bob_user_id: &str) {
    container
        .extensions
        .friend_room_service
        .send_friend_request("test-request-id", alice_user_id, bob_user_id, Some("hello"))
        .await
        .expect("send friend request");
    container
        .extensions
        .friend_room_service
        .accept_friend_request("test-request-id", bob_user_id, alice_user_id)
        .await
        .expect("accept friend request");
}

// ---------------------------------------------------------------------------
// HTTP-layer helpers (used by Phase C route tests)
// ---------------------------------------------------------------------------

/// Builds a fresh axum router backed by an isolated test schema.
pub async fn setup_fresh_app() -> Option<axum::Router> {
    use synapse_rust::web::routes::state::AppState;
    let pool = synapse_rust::test_utils::prepare_isolated_test_pool().await.ok()?;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool, cache.clone()).await;
    let state = AppState::new(container, cache);
    Some(synapse_rust::web::create_router(state))
}

/// Registers a user via HTTP and returns its access token.
pub async fn http_register_user(app: &axum::Router, username: &str) -> String {
    use axum::body::Body;
    use hyper::Request;
    use tower::ServiceExt;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": username,
                "password": "Test@123",
                "device_id": format!("DEV_{username}"),
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "user registration failed with status {status}: {}",
        String::from_utf8_lossy(&body)
    );
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

/// Sends a friend request via HTTP and returns the response.
pub async fn http_send_friend_request(
    app: &axum::Router,
    token: &str,
    target_user_id: &str,
) -> (axum::http::StatusCode, serde_json::Value) {
    use axum::body::Body;
    use hyper::Request;
    use tower::ServiceExt;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/user/friend/request")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({ "target_user_id": target_user_id }).to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}
