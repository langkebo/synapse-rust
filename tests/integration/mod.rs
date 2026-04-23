mod api_account_data_routes_tests;
mod api_admin_audit_tests;
mod api_admin_federation_tests;
mod api_admin_regression_tests;
mod api_admin_room_lifecycle_tests;
mod api_admin_tests;
mod api_admin_user_lifecycle_tests;
mod api_appservice_p1_tests;
mod api_appservice_tests;
mod api_auth_routes_tests;
mod api_beacon_location_tests;
mod api_device_presence_tests;
mod api_device_routes_tests;
mod api_e2ee_advanced_tests;
mod api_e2ee_tests;
mod api_enhanced_features_tests;
mod api_feature_flags_tests;
mod api_federation_join_key_fetch_priority_tests;
mod api_federation_key_fetch_limits_tests;
mod api_federation_signature_auth_tests;
mod api_federation_tests;
mod api_friend_room_routes_tests;
mod api_input_validation_tests;
mod api_invite_blocklist_routes_tests;
mod api_media_routes_tests;
#[cfg(feature = "openclaw-routes")]
mod api_openclaw_routes_tests;
mod api_placeholder_contract_p0_tests;
mod api_placeholder_contract_p1p2_tests;
mod api_profile_tests;
mod api_protocol_alignment_tests;
mod api_rate_limit_contract_tests;
mod api_rendezvous_routes_tests;
mod api_room_summary_routes_tests;
mod api_room_sync_tests;
mod api_room_tests;
mod api_search_thread_tests;
mod api_sliding_sync_contract_tests;
mod api_space_routes_tests;
mod api_sticky_event_tests;
mod api_sync_filter_tests;
mod api_sync_isolation_rate_limit_tests;
mod api_telemetry_alerts_tests;
mod api_typing_routes_tests;
mod api_widget_tests;
mod api_worker_replication_auth_tests;
mod cache_tests;
mod concurrency_tests;
mod database_integrity_tests;
mod federation_error_tests;
mod metrics_tests;
mod password_hash_pool_tests;
mod protocol_compliance_tests;
mod regex_cache_tests;
mod transaction_tests;
mod voice_routes_tests;

mod permission_escalation_tests;

#[cfg(test)]
mod coverage_tests;

use std::sync::Arc;

pub fn with_local_connect_info(
    mut request: hyper::Request<axum::body::Body>,
) -> hyper::Request<axum::body::Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    let local_addr: SocketAddr = "127.0.0.1:65530"
        .parse()
        .expect("valid loopback socket addr");
    request.extensions_mut().insert(ConnectInfo(local_addr));
    request
}

fn integration_tests_required() -> bool {
    if let Ok(value) = std::env::var("INTEGRATION_TESTS_REQUIRED") {
        let value = value.trim().to_ascii_lowercase();
        return value == "1" || value == "true" || value == "yes" || value == "required";
    }
    std::env::var("CI").is_ok()
}

pub async fn get_test_pool() -> Option<Arc<sqlx::PgPool>> {
    let use_isolated = std::env::var("TEST_ISOLATED_SCHEMAS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let result = if use_isolated {
        synapse_rust::test_utils::prepare_isolated_test_pool().await
    } else {
        synapse_rust::test_utils::prepare_shared_test_pool().await
    };

    match result {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping integration tests because schema setup failed: {}",
                error
            );
            if integration_tests_required() {
                panic!(
                    "Integration tests require strict migration initialization to succeed, but schema setup failed: {}",
                    error
                );
            }
            None
        }
    }
}

pub async fn require_test_pool() -> Arc<sqlx::PgPool> {
    get_test_pool().await.unwrap_or_else(|| {
        panic!(
            "Integration test requires database setup. For local runs, start PostgreSQL and apply migrations first; in CI this must already succeed."
        )
    })
}

pub fn clear_test_cache() {}

pub async fn setup_test_app() -> Option<axum::Router> {
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_rust::services::ServiceContainer;
    use synapse_rust::web::routes::state::AppState;

    let pool = get_test_pool().await?;
    let cache = std::sync::Arc::new(CacheManager::new(CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool, cache.clone());
    let state = AppState::new(container, cache);

    Some(synapse_rust::web::create_router(state))
}

pub async fn get_admin_token(app: &axum::Router) -> (String, String) {
    register_admin_token(app, None).await
}

pub async fn get_super_admin_token(app: &axum::Router) -> (String, String) {
    register_admin_token(app, Some("super_admin")).await
}

async fn register_admin_token(app: &axum::Router, user_type: Option<&str>) -> (String, String) {
    use axum::body::Body;
    use hyper::Request;
    use tower::ServiceExt;

    let username = format!("admin_{}", rand::random::<u32>());

    // Step 1: Get nonce
    let nonce_request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/register/nonce")
        .body(Body::empty())
        .unwrap();

    let nonce_response = app
        .clone()
        .oneshot(with_local_connect_info(nonce_request))
        .await
        .unwrap();
    let nonce_body = axum::body::to_bytes(nonce_response.into_body(), 1024)
        .await
        .unwrap();
    let nonce_json: serde_json::Value = serde_json::from_slice(&nonce_body).unwrap();
    let nonce = nonce_json["nonce"].as_str().unwrap();

    // Step 2: Calculate HMAC
    let shared_secret = std::env::var("REGISTRATION_SHARED_SECRET")
        .unwrap_or_else(|_| "test_shared_secret".to_string());

    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut message = Vec::new();
    message.extend(nonce.as_bytes());
    message.push(b'\x00');
    message.extend(username.as_bytes());
    message.push(b'\x00');
    message.extend("AdminTest@123".as_bytes());
    message.push(b'\x00');
    message.extend(b"admin\x00\x00\x00");
    if let Some(user_type) = user_type {
        message.push(b'\x00');
        message.extend(user_type.as_bytes());
    }

    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes()).unwrap();
    mac.update(&message);
    let mac_result = mac.finalize();
    let mac_hex = hex::encode(mac_result.into_bytes());

    // Step 3: Register admin user
    let register_request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "nonce": nonce,
                "username": &username,
                "password": "AdminTest@123",
                "admin": true,
                "user_type": user_type,
                "mac": mac_hex
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(with_local_connect_info(register_request))
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "admin registration failed with status {}: {}",
        status,
        String::from_utf8_lossy(&body)
    );
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token = json["access_token"].as_str().unwrap().to_string();
    (token, username)
}

pub async fn create_test_user(app: &axum::Router) -> String {
    use axum::body::Body;
    use hyper::Request;
    use tower::ServiceExt;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": format!("user_{}", rand::random::<u32>()),
                "password": "UserTest@123",
                "device_id": "TESTDEVICE"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}
