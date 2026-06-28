use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::web::routes::state::AppState;
use synapse_services::ServiceContainer;
use tower::ServiceExt;

async fn setup_test_app_with_voip() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    let pool = super::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool.clone()).await;

    // Rebuild the RTC infra service after mutating config because the service
    // captures `config.voip` during ServiceContainer construction.
    container.core.config.voip.turn_uris = vec!["turn:turn.example.org?transport=udp".to_string()];
    container.core.config.voip.stun_uris = vec!["stun:stun.example.org".to_string()];
    container.core.config.voip.turn_shared_secret = Some("test_secret".to_string());
    let voip_config = container.core.config.voip.clone();
    let rtc_domain_service = Arc::make_mut(&mut container.extensions.rtc_domain_service);
    rtc_domain_service.infra = Arc::new(synapse_services::rtc::RtcInfraService::new(Arc::new(voip_config)));

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some((synapse_rust::web::create_router(state), pool))
}

#[tokio::test]
async fn test_get_rtc_transports_authenticated() {
    let Some((app, _pool)) = setup_test_app_with_voip().await else {
        eprintln!("Skipping test because test database is unavailable");
        return;
    };

    // 1. Register and login to get token
    let username = format!("alice_{}", &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]);
    let password = "Password123!";

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
    let status = reg_res.status();
    let body = axum::body::to_bytes(reg_res.into_body(), 10240).await.unwrap();
    if status != StatusCode::OK {
        panic!("Registration failed with status {}: {}", status, String::from_utf8_lossy(&body));
    }
    let reg_json: Value = serde_json::from_slice(&body).unwrap();
    let token = reg_json["access_token"].as_str().unwrap();

    // 2. Call RTC transports
    let req = Request::builder()
        .method("GET")
        .uri("/_matrix/client/unstable/org.matrix.msc4143/rtc/transports")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = axum::body::to_bytes(res.into_body(), 10240).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // 3. Verify response
    let transports = json["transports"].as_array().expect("transports should be an array");
    assert!(!transports.is_empty());

    let transport = &transports[0];
    assert_eq!(transport["type"], "org.matrix.msc4403.ice-server-transport");

    let ice_servers = transport["ice_servers"].as_array().expect("ice_servers should be an array");
    assert_eq!(ice_servers.len(), 2); // 1 STUN, 1 TURN

    // Check STUN
    let stun = ice_servers.iter().find(|s| s["urls"].as_array().unwrap()[0] == "stun:stun.example.org").unwrap();
    assert!(stun["username"].is_null());

    // Check TURN
    let turn =
        ice_servers.iter().find(|s| s["urls"].as_array().unwrap()[0] == "turn:turn.example.org?transport=udp").unwrap();
    assert!(turn["username"].is_string());
    assert!(turn["credential"].is_string());
}

#[tokio::test]
async fn test_get_rtc_transports_unauthenticated() {
    let Some((app, _pool)) = setup_test_app_with_voip().await else {
        eprintln!("Skipping test because test database is unavailable");
        return;
    };

    let req = Request::builder()
        .method("GET")
        .uri("/_matrix/client/unstable/org.matrix.msc4143/rtc/transports")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    // Should be unauthorized because AuthenticatedUser extractor fails
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
