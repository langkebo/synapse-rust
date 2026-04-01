mod api_account_data_routes_tests;
mod api_admin_audit_tests;
mod api_admin_federation_tests;
mod api_admin_regression_tests;
mod api_admin_tests;
mod api_auth_routes_tests;
mod api_device_presence_tests;
mod api_device_routes_tests;
mod api_e2ee_tests;
mod api_enhanced_features_tests;
mod api_feature_flags_tests;
mod api_federation_tests;
mod api_friend_room_routes_tests;
mod api_input_validation_tests;
mod api_ip_block_test;
mod api_media_routes_tests;
mod api_profile_tests;
mod api_protocol_alignment_tests;
mod api_room_summary_routes_tests;
mod api_room_tests;
mod api_search_thread_tests;
mod api_telemetry_alerts_tests;
mod api_widget_tests;
mod cache_tests;
mod concurrency_tests;
mod metrics_tests;
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

fn integration_tests_required() -> bool {
    if let Ok(value) = std::env::var("INTEGRATION_TESTS_REQUIRED") {
        let value = value.trim().to_ascii_lowercase();
        return value == "1" || value == "true" || value == "yes" || value == "required";
    }
    std::env::var("CI").is_ok()
}

async fn get_test_pool() -> Option<Arc<sqlx::PgPool>> {
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

    let admin_secret = std::env::var("ADMIN_REGISTRATION_SECRET")
        .unwrap_or_else(|_| "admin_secret_key".to_string());

    let username = format!("admin_{}", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/register_admin")
        .header("Content-Type", "application/json")
        .header("X-Admin-Secret", &admin_secret)
        .body(Body::from(
            serde_json::json!({
                "username": &username,
                "password": "AdminTest@123"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
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
