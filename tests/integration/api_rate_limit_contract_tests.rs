use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;
use synapse_rust::cache::CacheManager;
use synapse_rust::common::config::{RateLimitEndpointRule, RateLimitMatchType, RateLimitRule};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::state::AppState;
use tower::ServiceExt;

async fn setup_test_app_with_rate_limit_rule(rule: RateLimitEndpointRule) -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.rate_limit.enabled = true;
    container.config.rate_limit.exempt_paths =
        vec!["/".to_string(), "/_matrix/client/versions".to_string()];
    container.config.rate_limit.exempt_path_prefixes = Vec::new();
    container.config.rate_limit.endpoints = vec![rule];

    let cache = Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);
    Some(synapse_rust::web::create_router(state))
}

async fn register_user_and_get_token(app: &axum::Router) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": format!("user_{}", rand::random::<u32>()),
                "password": "UserTest@123",
                "device_id": "TESTDEVICE"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 16)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_sync_rate_limited_returns_retry_after_ms() {
    let rule = RateLimitEndpointRule {
        path: "/_matrix/client/v3/sync".to_string(),
        match_type: RateLimitMatchType::Exact,
        rule: RateLimitRule {
            per_second: 1,
            burst_size: 1,
        },
    };

    let Some(app) = setup_test_app_with_rate_limit_rule(rule).await else {
        return;
    };
    let token = register_user_and_get_token(&app).await;

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert!(response.status().is_success());

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 16)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_LIMIT_EXCEEDED");
    assert!(json
        .get("retry_after_ms")
        .and_then(|v| v.as_u64())
        .is_some());
}

#[tokio::test]
async fn test_sliding_sync_rate_limited_returns_retry_after_ms() {
    let rule = RateLimitEndpointRule {
        path: "/_matrix/client/v3/sync".to_string(),
        match_type: RateLimitMatchType::Exact,
        rule: RateLimitRule {
            per_second: 1,
            burst_size: 1,
        },
    };

    let Some(app) = setup_test_app_with_rate_limit_rule(rule).await else {
        return;
    };
    let token = register_user_and_get_token(&app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/sync")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert!(response.status().is_success());

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/sync")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = app
        .clone()
        .oneshot(super::with_local_connect_info(request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 16)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_LIMIT_EXCEEDED");
    assert!(json
        .get("retry_after_ms")
        .and_then(|v| v.as_u64())
        .is_some());
}
