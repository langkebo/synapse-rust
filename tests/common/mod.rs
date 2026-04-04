#![allow(dead_code)]

use sqlx::{PgPool, Pool, Postgres};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use synapse_rust::services::database_initializer::initialize_database;
use synapse_rust::test_utils::{env_lock_async, EnvGuard};

static TEST_DB_INIT_MUTEX: OnceLock<tokio::sync::Mutex<bool>> = OnceLock::new();

pub fn get_database_url() -> String {
    candidate_database_urls()
        .into_iter()
        .next()
        .unwrap_or_else(|| "postgresql://synapse:synapse@localhost:5432/synapse".to_string())
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

pub fn get_test_pool() -> Arc<Pool<Postgres>> {
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    runtime
        .block_on(get_test_pool_async())
        .expect("Failed to connect to test database")
}

pub async fn get_test_pool_async() -> Result<Arc<Pool<Postgres>>, String> {
    let mut errors = Vec::new();

    for database_url in candidate_database_urls() {
        match sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(600))
            .connect(&database_url)
            .await
        {
            Ok(pool) => match ensure_test_schema(&pool).await {
                Ok(()) => return Ok(Arc::new(pool)),
                Err(error) => {
                    errors.push(format!("{} -> schema init failed: {}", database_url, error))
                }
            },
            Err(e) => errors.push(format!("{} -> {}", database_url, e)),
        }
    }

    Err(format!(
        "Failed to connect to any configured test database: {}",
        errors.join(" | ")
    ))
}

async fn ensure_test_schema(pool: &PgPool) -> Result<(), String> {
    let init_mutex = TEST_DB_INIT_MUTEX.get_or_init(|| tokio::sync::Mutex::new(false));
    let mut initialized = init_mutex.lock().await;
    if *initialized {
        return Ok(());
    }

    let _env_lock = env_lock_async().await;
    let mut env_guard = EnvGuard::new();
    env_guard.set("SYNAPSE_ENABLE_RUNTIME_DB_INIT", "true");
    initialize_database(pool).await?;
    *initialized = true;
    Ok(())
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
        let _ = sqlx::query(&format!(
            "DELETE FROM {} WHERE user_id LIKE 'test_%'",
            table
        ))
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
