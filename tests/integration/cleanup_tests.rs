use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::CacheManager;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

async fn setup_test_app_with_admin() -> Option<(axum::Router, String)> {
    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool.clone()).await;
    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state);

    // 1. Register and login as admin
    let username = format!(
        "admin_{}",
        &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]
    );
    let password = "AdminPassword123!";

    let reg_req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": password,
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let reg_res = app.clone().oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::OK);

    let body = axum::body::to_bytes(reg_res.into_body(), 10240)
        .await
        .unwrap();
    let reg_json: Value = serde_json::from_slice(&body).unwrap();
    let user_id = reg_json["user_id"].as_str().unwrap();

    // 2. Make user admin in DB
    sqlx::query("UPDATE users SET is_admin = TRUE, user_type = 'super_admin' WHERE user_id = $1")
        .bind(user_id)
        .execute(&*pool)
        .await
        .unwrap();

    // 3. Login again to get admin token
    let login_req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "type": "m.login.password",
                "user": username,
                "password": password
            })
            .to_string(),
        ))
        .unwrap();

    let login_res = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(login_res.status(), StatusCode::OK);

    let body = axum::body::to_bytes(login_res.into_body(), 10240)
        .await
        .unwrap();
    let login_json: Value = serde_json::from_slice(&body).unwrap();
    let token = login_json["access_token"].as_str().unwrap().to_string();

    Some((app, token))
}

#[tokio::test]
async fn test_cleanup_api() {
    let Some((app, admin_token)) = setup_test_app_with_admin().await else {
        eprintln!("Skipping test because test database is unavailable");
        return;
    };

    // 1. Call cleanup all
    let req = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/cleanup/all")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "min_age_ms": 0 // Cleanup all empty rooms regardless of age for test
            })
            .to_string(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = axum::body::to_bytes(res.into_body(), 10240).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("rooms").is_some());
    assert!(json.get("tokens").is_some());

    let rooms = json["rooms"].as_object().unwrap();
    assert!(rooms.get("deleted_empty_rooms").is_some());
    assert!(rooms.get("deleted_orphan_events").is_some());
}

#[tokio::test]
async fn test_cleanup_api_unauthorized() {
    let Some(pool) = super::get_test_pool().await else {
        eprintln!("Skipping test because test database is unavailable");
        return;
    };
    let container = ServiceContainer::new_test_with_pool(pool).await;
    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/cleanup/all")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
