#![allow(dead_code)]
pub mod http_mock;
pub mod snapshots;
use sqlx::{PgPool, Pool, Postgres};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use synapse_services::database_initializer::initialize_database;
use synapse_test_utils::{env_lock_async, EnvGuard};

static TEST_DB_INIT_MUTEX: OnceLock<tokio::sync::Mutex<bool>> = OnceLock::new();

fn candidate_database_urls() -> Vec<String> {
    let mut urls = Vec::new();
    for key in ["TEST_DATABASE_URL", "DATABASE_URL"] {
        if let Ok(value) = std::env::var(key) {
            if !urls.iter().any(|existing| existing == &value) {
                urls.push(value);
            }
        }
    }

    if urls.is_empty() && !integration_tests_required() {
        return urls;
    }

    // Test-DB fallback convention — asserted by
    // `tests/unit/test_db_url_convention_tests.rs`, so keep every copy in sync:
    //   * port `5432`: what CI exports, what the dev compose override publishes
    //     (`${DB_EXPOSE_PORT:-5432}:5432`) and what `init_test_public_schema.sh`
    //     defaults to;
    //   * `synapse_test` only, never the application database — a harness that
    //     silently falls back to the database under test turns a configuration
    //     mistake into data loss;
    //   * no `15432`: a dead host-forward from an older compose file. Nothing
    //     listens there, so probing it first cost a connect timeout in every
    //     DB-backed test process before falling through (H-12).
    for fallback in [
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

pub async fn get_test_pool_async() -> Result<Arc<Pool<Postgres>>, String> {
    let mut errors = Vec::new();
    let connect_timeout = synapse_test_utils::configured_test_pool_connect_timeout();

    for database_url in candidate_database_urls() {
        let connect_future = sqlx::postgres::PgPoolOptions::new()
            .max_connections(synapse_test_utils::configured_test_pool_max_connections())
            .min_connections(synapse_test_utils::configured_test_pool_min_connections())
            .acquire_timeout(synapse_test_utils::configured_test_pool_acquire_timeout())
            .idle_timeout(synapse_test_utils::configured_test_pool_idle_timeout())
            .max_lifetime(Some(synapse_test_utils::configured_test_pool_max_lifetime()))
            .connect(&database_url);

        match tokio::time::timeout(connect_timeout, connect_future).await {
            Err(_) => errors.push(format!("{database_url} -> connect timed out after {connect_timeout:?}")),
            Ok(Ok(pool)) => match ensure_test_schema(&pool).await {
                Ok(()) => return Ok(Arc::new(pool)),
                Err(error) => errors.push(format!("{database_url} -> schema init failed: {error}")),
            },
            Ok(Err(e)) => errors.push(format!("{database_url} -> {e}")),
        }
    }

    let message = format!("Failed to connect to any configured test database: {}", errors.join(" | "));
    if integration_tests_required() {
        panic!("{message}");
    }
    Err(message)
}

/// Single source of truth for "this run must not silently skip DB tests".
///
/// It lives here, in the module that both `--test unit` and `--test integration`
/// compile (`#[path = "../common/mod.rs"]`), which is why
/// `tests/integration/mod.rs` re-exports it rather than keeping its own copy.
/// That copy (`db_tests_required`) was a near-duplicate of the integration
/// helper; two implementations of an infrastructure decision is precisely the
/// defect AGENTS.md iron rule 2 names.
///
/// Semantics: the first of `INTEGRATION_TESTS_REQUIRED` / `DB_TESTS_REQUIRED`
/// that is set decides the answer (`1`/`true`/`yes`/`required` ⇒ required;
/// anything else ⇒ explicitly not required, even under `CI`). When neither is
/// set, a present `CI` means required.
pub(crate) fn integration_tests_required() -> bool {
    for key in ["INTEGRATION_TESTS_REQUIRED", "DB_TESTS_REQUIRED"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim().to_ascii_lowercase();
            return value == "1" || value == "true" || value == "yes" || value == "required";
        }
    }
    std::env::var("CI").is_ok()
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
    tokio::time::timeout(Duration::from_secs(120), initialize_database(pool))
        .await
        .map_err(|_| "schema init timed out after 120 seconds".to_string())??;
    *initialized = true;
    Ok(())
}
