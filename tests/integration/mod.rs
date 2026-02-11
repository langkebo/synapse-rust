mod api_admin_tests;
mod api_device_presence_tests;
mod api_e2ee_tests;
mod api_enhanced_features_tests;
mod api_federation_tests;
mod api_input_validation_tests;
mod api_ip_block_test;
mod api_profile_tests;
mod api_room_tests;
mod transaction_tests;
mod cache_tests;
mod concurrency_tests;
mod metrics_tests;
mod regex_cache_tests;
mod protocol_compliance_tests;
mod voice_routes_tests;

use std::sync::Arc;
use synapse_rust::services::DatabaseInitService;

async fn init_test_database() -> bool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
    {
        Ok(pool) => Arc::new(pool),
        Err(error) => {
            eprintln!(
                "Skipping integration tests because test database is unavailable: {}",
                error
            );
            return false;
        }
    };
    let init_service = DatabaseInitService::new(pool);
    let report = match init_service.initialize().await {
        Ok(report) => report,
        Err(error) => {
            eprintln!(
                "Skipping integration tests because database initialization failed: {}",
                error
            );
            return false;
        }
    };
    if !report.success {
        eprintln!(
            "Skipping integration tests because database initialization errors: {:?}",
            report.errors
        );
        return false;
    }
    true
}
