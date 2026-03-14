use once_cell::sync::Lazy;
use sqlx::{PgPool, Pool, Postgres};
use std::sync::Arc;
use std::time::Duration;

static TEST_POOL: Lazy<Arc<Pool<Postgres>>> = Lazy::new(|| {
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    runtime.block_on(async {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgres://synapse:secret@localhost:5432/synapse_test".to_string());

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(600))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        Arc::new(pool)
    })
});

pub fn get_test_pool() -> Arc<Pool<Postgres>> {
    TEST_POOL.clone()
}

pub async fn setup_test_pool() -> Arc<Pool<Postgres>> {
    get_test_pool()
}

pub async fn cleanup_test_data(pool: &PgPool) {
    let tables = vec![
        "room_memberships",
        "events",
        "rooms",
        "users",
        "devices",
        "access_tokens",
        "refresh_tokens",
        "account_data",
        "presence",
    ];

    for table in tables {
        let _ = sqlx::query(&format!("DELETE FROM {} WHERE user_id LIKE 'test_%'", table))
            .execute(pool)
            .await;
    }
}

pub fn generate_test_user_id() -> String {
    format!("@test_{}:localhost", uuid::Uuid::new_v4())
}

pub fn generate_test_room_id() -> String {
    format!("!test_{}:localhost", uuid::Uuid::new_v4())
}

pub fn generate_test_device_id() -> String {
    format!("DEVICE_{}", uuid::Uuid::new_v4())
}

pub fn generate_test_token() -> String {
    format!("token_{}", uuid::Uuid::new_v4())
}
