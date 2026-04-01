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
use synapse_rust::services::DatabaseInitService;
use tokio::sync::OnceCell;

static TEST_POOL: OnceCell<Option<Arc<sqlx::PgPool>>> = OnceCell::const_new();
static TEST_DB_READY: OnceCell<bool> = OnceCell::const_new();

fn integration_tests_required() -> bool {
    if let Ok(value) = std::env::var("INTEGRATION_TESTS_REQUIRED") {
        let value = value.trim().to_ascii_lowercase();
        return value == "1" || value == "true" || value == "yes" || value == "required";
    }
    std::env::var("CI").is_ok()
}

fn candidate_database_urls() -> Vec<String> {
    let mut urls = Vec::new();

    for key in ["TEST_DATABASE_URL", "DATABASE_URL"] {
        if let Ok(value) = std::env::var(key) {
            if !urls.iter().any(|existing| existing == &value) {
                urls.push(value);
            }
        }
    }

    for fallback in [
        "postgresql://synapse:synapse@localhost:5432/synapse",
        "postgresql://synapse:synapse@localhost:5432/synapse_test",
        "postgresql://synapse:secret@localhost:5432/synapse_test",
    ] {
        let fallback = fallback.to_string();
        if !urls.iter().any(|existing| existing == &fallback) {
            urls.push(fallback);
        }
    }

    urls
}

async fn get_test_pool() -> Option<Arc<sqlx::PgPool>> {
    let pool = TEST_POOL
        .get_or_init(|| async {
            let mut errors = Vec::new();

            for database_url in candidate_database_urls() {
                match sqlx::postgres::PgPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(std::time::Duration::from_secs(10))
                    .connect(&database_url)
                    .await
                {
                    Ok(pool) => return Some(Arc::new(pool)),
                    Err(error) => {
                        errors.push(format!("{} -> {}", database_url, error));
                    }
                }
            }

            eprintln!(
                "Skipping integration tests because test database is unavailable: {}",
                errors.join(" | ")
            );
            if integration_tests_required() {
                panic!(
                    "Integration tests require a working PostgreSQL test database, but none of the candidate URLs connected successfully. Errors: {}",
                    errors.join(" | ")
                );
            }
            None
        })
        .await;

    let Some(pool) = pool else {
        return None;
    };

    let ready = TEST_DB_READY
        .get_or_init(|| async {
            let init_service = DatabaseInitService::new(pool.clone());
            match init_service.initialize().await {
                Ok(report) if report.success => true,
                Ok(report) => {
                    eprintln!(
                        "Skipping integration tests because database initialization errors: {:?}",
                        report.errors
                    );
                    if integration_tests_required() {
                        panic!(
                            "Integration tests require database initialization to succeed, but it reported errors: {:?}",
                            report.errors
                        );
                    }
                    false
                }
                Err(error) => {
                    eprintln!(
                        "Skipping integration tests because database initialization failed: {}",
                        error
                    );
                    if integration_tests_required() {
                        panic!(
                            "Integration tests require database initialization to succeed, but initialization failed: {}",
                            error
                        );
                    }
                    false
                }
            }
        })
        .await;

    if !*ready {
        return None;
    }

    Some(pool.clone())
}

async fn init_test_database() -> bool {
    get_test_pool().await.is_some()
}
