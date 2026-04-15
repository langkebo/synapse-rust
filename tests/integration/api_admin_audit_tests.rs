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

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool.clone());
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some((create_router(state), pool))
}

async fn setup_test_app() -> Option<axum::Router> {
    let (app, _) = setup_test_app_with_pool().await?;
    Some(app)
}

async fn get_admin_token(app: &axum::Router) -> (String, String) {
    let (token, _) = super::get_admin_token(app).await;
    let request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
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
    (
        token,
        json["user_id"].as_str().unwrap().to_string(),
    )
}

async fn promote_admin_role(pool: &sqlx::PgPool, username: &str, role: &str) {
    sqlx::query("UPDATE users SET user_type = $2 WHERE username = $1")
        .bind(username)
        .bind(role)
        .execute(pool)
        .await
        .expect("failed to update admin test user role");
}

async fn create_test_user(app: &axum::Router) -> (String, String) {
    let username = format!("audit_user_{}", rand::random::<u32>());
    let password = "Password123!";

    let request = Request::builder()
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_admin_audit_write_query_and_trace() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };
    let (admin_token, admin_user_id) = get_admin_token(&app).await;
    let admin_username = admin_user_id
        .trim_start_matches('@')
        .split(':')
        .next()
        .unwrap_or(&admin_user_id)
        .to_string();
    promote_admin_role(&pool, &admin_username, "super_admin").await;
    let unique = rand::random::<u32>();
    let action = format!("admin.audit.manual_{}", unique);
    let request_id = format!("req-audit-manual-{}", unique);

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/audit/events")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "actor_id": format!("@manual_admin_{}:localhost", unique),
                "action": action,
                "resource_type": "user",
                "resource_id": format!("@manual_user_{}:localhost", unique),
                "result": "success",
                "request_id": request_id,
                "details": { "source": "integration_test" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let created_event: Value = serde_json::from_slice(&body).unwrap();
    let event_id = created_event["event_id"].as_str().unwrap().to_string();
    assert_eq!(created_event["action"], action);

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/audit/events?action={}", action))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let list_response: Value = serde_json::from_slice(&body).unwrap();
    let events = list_response["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["event_id"] == event_id));

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/audit/events/{}", event_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let detail_response: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail_response["request_id"], created_event["request_id"]);
    assert_eq!(detail_response["details"]["source"], "integration_test");
}

#[tokio::test]
async fn test_shadow_ban_writes_audit_event() {
    let Some((app, pool)) = setup_test_app_with_pool().await else {
        return;
    };
    let (admin_token, admin_user_id) = get_admin_token(&app).await;
    let admin_username = admin_user_id
        .trim_start_matches('@')
        .split(':')
        .next()
        .unwrap_or(&admin_user_id)
        .to_string();
    promote_admin_role(&pool, &admin_username, "super_admin").await;
    let (_, user_id) = create_test_user(&app).await;
    let request_id = format!("req-shadow-ban-{}", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/users/{}/shadow_ban", user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("x-request-id", &request_id)
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events?action=admin.user.shadow_ban")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let list_response: Value = serde_json::from_slice(&body).unwrap();
    let events = list_response["events"].as_array().unwrap();
    let audit_event = events
        .iter()
        .find(|event| {
            event["request_id"] == request_id
                && event["resource_id"] == user_id
                && event["actor_id"] == admin_user_id
        })
        .expect("shadow ban audit event should exist");

    let event_id = audit_event["event_id"].as_str().unwrap();

    let request = Request::builder()
        .uri(format!("/_synapse/admin/v1/audit/events/{}", event_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let detail_response: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail_response["details"]["is_shadow_banned"], true);
}

#[tokio::test]
async fn test_audit_query_requires_admin() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (user_token, _) = create_test_user(&app).await;

    let request = Request::builder()
        .uri("/_synapse/admin/v1/audit/events")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
