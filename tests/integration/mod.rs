mod api_account_data_routes_tests;
mod api_admin_audit_tests;
mod api_admin_federation_tests;
mod api_admin_regression_tests;
mod api_admin_room_lifecycle_tests;
mod api_admin_tests;
mod api_admin_user_lifecycle_tests;
mod api_appservice_basic_tests;
mod api_appservice_p1_tests;
mod api_appservice_tests;
mod api_auth_routes_tests;
mod api_device_presence_tests;
mod api_device_routes_tests;
mod api_e2ee_advanced_tests;
mod api_e2ee_tests;
mod api_enhanced_features_tests;
mod api_feature_flags_tests;
mod api_federation_signature_auth_tests;
mod api_federation_tests;
mod api_friend_room_routes_tests;
mod api_input_validation_tests;
mod api_ip_block_test;
mod api_media_routes_tests;
mod api_profile_tests;
mod api_protocol_alignment_tests;
mod api_room_placeholder_contract_tests;
mod api_room_summary_routes_tests;
mod api_room_sync_tests;
mod api_room_tests;
mod api_search_thread_tests;
mod api_shell_route_fixes_p1_tests;
mod api_shell_route_fixes_p2_friend_tests;
mod api_shell_route_fixes_p2_misc_tests;
mod api_shell_route_fixes_p2_push_tests;
mod api_telemetry_alerts_tests;
mod api_widget_tests;
mod cache_tests;
mod concurrency_tests;
mod database_integrity_tests;
mod federation_error_tests;
mod metrics_tests;
mod missing_features_tests;
mod password_hash_pool_tests;
mod protocol_compliance_tests;
mod regex_cache_tests;
mod transaction_tests;
mod voice_routes_tests;

#[cfg(test)]
mod coverage_tests;

#[cfg(test)]
mod schema_validation_tests;

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
    match synapse_rust::test_utils::prepare_isolated_test_pool().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping integration tests because isolated schema setup failed: {}",
                error
            );
            if integration_tests_required() {
                panic!(
                    "Integration tests require strict migration initialization to succeed, but isolated schema setup failed: {}",
                    error
                );
            }
            None
        }
    }
}

async fn init_test_database() -> bool {
    match get_test_pool().await {
        Some(pool) => {
            synapse_rust::test_utils::enqueue_prepared_test_pool(pool);
            true
        }
        None => false,
    }
}

pub fn clear_test_cache() {}

pub async fn setup_test_app() -> Option<axum::Router> {
    use synapse_rust::cache::CacheManager;
    use synapse_rust::services::ServiceContainer;
    use synapse_rust::web::routes::state::AppState;

    let pool = get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool);
    let cache = std::sync::Arc::new(CacheManager::new(Default::default()));
    let state = AppState::new(container, cache);

    Some(synapse_rust::web::create_router(state))
}

pub async fn get_admin_token(app: &axum::Router) -> (String, String) {
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

    let mac_content = format!("{}\0{}\0{}\0admin", nonce, username, "AdminTest@123");
    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes()).unwrap();
    mac.update(mac_content.as_bytes());
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
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
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
