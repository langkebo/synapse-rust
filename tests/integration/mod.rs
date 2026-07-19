#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod api_account_data_routes_tests;
mod api_admin_audit_tests;
mod api_admin_federation_tests;
mod api_admin_room_lifecycle_tests;
mod api_admin_user_lifecycle_tests;
mod api_appservice_p1_tests;
mod api_appservice_tests;
mod api_auth_routes_tests;
mod api_beacon_location_tests;
mod api_device_presence_tests;
mod api_device_routes_tests;
mod api_e2ee_advanced_tests;
mod api_enhanced_features_tests;
mod api_error_compliance_tests;
mod api_feature_flags_tests;
mod api_federation_join_key_fetch_priority_tests;
mod api_federation_key_fetch_limits_tests;
mod api_federation_tests;
mod api_federation_transaction_tests;
mod api_input_validation_tests;
mod api_invite_blocklist_routes_tests;
mod api_key_backup_route_table_tests;
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
mod api_route_ledger_tests;
mod api_route_snapshots_tests;
mod api_search_thread_tests;
mod api_security_headers_tests;
mod api_space_routes_tests;
mod api_sticky_event_tests;
mod api_sync_filter_tests;
mod api_sync_isolation_rate_limit_tests;
mod api_telemetry_alerts_tests;
mod api_typing_routes_tests;
mod api_widget_tests;
mod api_worker_replication_auth_tests;
mod cache_tests;
mod cleanup_tests;
mod concurrency_tests;
mod database_integrity_tests;
mod federation_error_tests;
mod metrics_tests;
mod password_hash_pool_tests;
mod protocol_compliance_tests;
mod regex_cache_tests;
mod rtc_transports_tests;
mod transaction_tests;
mod voice_routes_tests;
mod worker_task_recovery_tests;

#[cfg(feature = "beacons")]
mod beacon_storage_tests_migrated;
mod cross_signing_storage_tests_migrated;
mod device_storage_tests_migrated;
mod event_storage_tests_migrated;
mod feature_flags_storage_tests_migrated;
mod federation_blacklist_storage_tests_migrated;
mod filter_storage_tests_migrated;
mod friend_room_storage_tests_migrated;
mod key_backup_recovery_tests;
mod key_backup_storage_tests_migrated;
mod megolm_dual_write_storage_tests_migrated;
mod membership_storage_tests_migrated;
mod openid_token_storage_tests_migrated;
mod permission_escalation_tests;
mod presence_storage_tests_migrated;
mod receipt_storage_tests_migrated;
mod refresh_token_storage_tests_migrated;
mod retention_storage_tests_migrated;
mod room_summary_storage_tests_migrated;
mod room_tag_storage_tests_migrated;
mod sliding_sync_storage_tests_migrated;
mod state_groups_storage_tests_migrated;
mod thread_storage_tests_migrated;
mod threepid_storage_tests_migrated;
mod token_storage_tests_migrated;
mod user_storage_tests_migrated;

// Service tests migrated from tests/unit/
mod admin_registration_service_tests_migrated;
mod auth_service_coverage_tests;
mod auth_service_tests_migrated;
mod captcha_tests_migrated;
mod exception_tests_migrated;
mod feature_flag_service_tests_migrated;
mod federation_service_tests_migrated;
mod invite_blocklist_tests_migrated;
#[cfg(feature = "voip-tracking")]
mod matrixrtc_tests_migrated;
mod registration_service_tests_migrated;
mod relations_service_tests_migrated;
mod room_service_tests_migrated;
mod sliding_sync_service_tests_migrated;
mod sync_service_tests_migrated;
mod to_device_sync_tests_migrated;
mod uia_service_tests_migrated;

// Schema contract tests migrated from tests/unit/
mod db_schema_smoke_tests_migrated;
mod schema_contract_p0_tests_migrated;
mod schema_contract_room_summary_queue_driver_tests_migrated;

mod nullable_decode_tests;

#[cfg(test)]
mod coverage_tests;

use std::sync::Arc;
use std::time::{Duration, Instant};

static TEST_POOL: tokio::sync::OnceCell<Option<Arc<sqlx::PgPool>>> = tokio::sync::OnceCell::const_new();

pub fn with_local_connect_info(mut request: hyper::Request<axum::body::Body>) -> hyper::Request<axum::body::Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    let local_addr: SocketAddr = "127.0.0.1:65530".parse().expect("valid loopback socket addr");
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

fn integration_test_setup_timeout() -> Duration {
    let default_secs = if integration_tests_required() { 600 } else { 120 };
    let minimum_secs = synapse_rust::test_utils::configured_test_db_init_timeout().as_secs().saturating_add(60);
    let secs = std::env::var("INTEGRATION_TEST_SETUP_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| default_secs.max(minimum_secs));
    Duration::from_secs(secs)
}

fn describe_integration_test_setup(mode: &str, elapsed: Duration) -> String {
    let database_url_source = if std::env::var_os("TEST_DATABASE_URL").is_some() {
        "TEST_DATABASE_URL"
    } else if std::env::var_os("DATABASE_URL").is_some() {
        "DATABASE_URL"
    } else {
        "built-in localhost fallbacks"
    };

    format!(
        "mode={mode}, elapsed={elapsed:?}, database_url_source={database_url_source}, \
         TEST_ISOLATED_SCHEMAS={}, TEST_DB_CONNECT_TIMEOUT_SECS={}, TEST_DB_INIT_TIMEOUT_SECS={}",
        std::env::var("TEST_ISOLATED_SCHEMAS").unwrap_or_else(|_| "<unset>".to_string()),
        std::env::var("TEST_DB_CONNECT_TIMEOUT_SECS").unwrap_or_else(|_| "<default>".to_string()),
        std::env::var("TEST_DB_INIT_TIMEOUT_SECS").unwrap_or_else(|_| "<default>".to_string()),
    )
}

pub async fn get_test_pool() -> Option<Arc<sqlx::PgPool>> {
    TEST_POOL
        .get_or_init(|| async {
            let use_isolated = std::env::var("TEST_ISOLATED_SCHEMAS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let mode = if use_isolated { "isolated-schema" } else { "shared-template-schema" };
            let setup_timeout = integration_test_setup_timeout();
            let started = Instant::now();

            eprintln!("Preparing integration test database schema ({mode}; timeout {setup_timeout:?})");

            let setup = async {
                if use_isolated {
                    synapse_rust::test_utils::prepare_isolated_test_pool().await
                } else {
                    match synapse_rust::test_utils::prepare_shared_test_pool().await {
                        Ok(pool) => Ok(pool),
                        Err(error) if should_fallback_to_isolated_pool(&error) => {
                            eprintln!(
                                "Shared test schema clone failed ({error}); retrying with isolated schema initialization"
                            );
                            synapse_rust::test_utils::prepare_isolated_test_pool().await
                        }
                        Err(error) => Err(error),
                    }
                }
            };

            let result = match tokio::time::timeout(setup_timeout, setup).await {
                Ok(result) => result,
                Err(_) => Err(format!(
                    "integration test database setup timed out after {setup_timeout:?}. \
                     Set INTEGRATION_TEST_SETUP_TIMEOUT_SECS to override, or INTEGRATION_TESTS_REQUIRED=1/CI=1 to fail hard.",
                )),
            };

            match result {
                Ok(pool) => {
                    eprintln!(
                        "Integration test database schema ready: {}",
                        describe_integration_test_setup(mode, started.elapsed())
                    );
                    Some(pool)
                }
                Err(error) => {
                    eprintln!(
                        "Skipping integration tests because schema setup failed: {}; {}",
                        error,
                        describe_integration_test_setup(mode, started.elapsed())
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
        })
        .await
        .clone()
}

fn should_fallback_to_isolated_pool(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("out of shared memory")
        || error.contains("failed to clone template")
        || error.contains("template schema initialization")
}

/// Returns an isolated schema pool for each call, bypassing the shared
/// `TEST_POOL` OnceCell cache.
///
/// Previously this returned the shared `TEST_POOL`, which caused two problems:
/// 1. PoolTimedOut: `#[tokio::test]` creates isolated runtimes; sqlx pool
///    connections from other runtimes become isolated (project memory known issue).
/// 2. Data interference: parallel tests shared the same schema and data.
///
/// Now each call returns a fresh schema cloned from the template (fast —
/// ~100x faster than re-running migrations), providing per-test isolation.
pub async fn require_test_pool() -> Arc<sqlx::PgPool> {
    synapse_rust::test_utils::prepare_shared_test_pool().await.unwrap_or_else(|error| {
        panic!(
            "Integration test requires database setup. For local runs, start PostgreSQL and apply migrations first; in CI this must already succeed. Error: {error}"
        )
    })
}

pub fn clear_test_cache() {}

static DEFAULT_APP: tokio::sync::OnceCell<Option<(axum::Router, synapse_rust::web::routes::state::AppState)>> =
    tokio::sync::OnceCell::const_new();

static FEDERATION_APP: tokio::sync::OnceCell<Option<(axum::Router, synapse_rust::web::routes::state::AppState)>> =
    tokio::sync::OnceCell::const_new();

pub async fn setup_test_app() -> Option<axum::Router> {
    let cached = DEFAULT_APP.get_or_init(|| async { build_test_app(|_| {}).await }).await;
    cached.as_ref().map(|(app, _)| app.clone())
}

pub async fn setup_test_app_with_config<F>(
    configure: F,
) -> Option<(axum::Router, synapse_rust::web::routes::state::AppState)>
where
    F: FnOnce(&mut synapse_services::ServiceContainer),
{
    build_test_app(configure).await
}

pub async fn setup_test_app_with_state() -> Option<(axum::Router, synapse_rust::web::routes::state::AppState)> {
    let cached = FEDERATION_APP
        .get_or_init(|| async {
            build_test_app(|container| {
                container.core.config.federation.allow_ingress = true;
            })
            .await
        })
        .await;
    cached.as_ref().map(|(app, state)| (app.clone(), state.clone()))
}

async fn build_test_app<F>(configure: F) -> Option<(axum::Router, synapse_rust::web::routes::state::AppState)>
where
    F: FnOnce(&mut synapse_services::ServiceContainer),
{
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_rust::web::routes::state::AppState;
    use synapse_services::ServiceContainer;

    let pool = get_test_pool().await?;
    let cache = std::sync::Arc::new(CacheManager::new(&CacheConfig::default()));
    let mut container = ServiceContainer::new_test_with_pool_and_cache(pool, cache.clone()).await;
    configure(&mut container);
    let state = AppState::new(container, cache);

    let app = synapse_rust::web::create_router(state.clone());
    Some((app, state))
}

pub async fn setup_test_app_with_pool(
) -> Option<(axum::Router, Arc<sqlx::PgPool>, Arc<synapse_rust::cache::CacheManager>)> {
    let cached = DEFAULT_APP.get_or_init(|| async { build_test_app(|_| {}).await }).await;
    cached.as_ref().map(|(app, state)| {
        let pool = state.services.account.user_storage.pool().clone();
        let cache = state.cache.clone();
        (app.clone(), pool, cache)
    })
}

// ============================================================================
// DB Isolation Infrastructure (Phase 4 — TestContext + setup_fresh_test_app)
// ============================================================================
//
// Project memory mandates:
// - "Integration tests must use TestContext for isolated database environments"
// - "Test database connections must use acquire_with_retry with exponential backoff"
// - "Destructive tests (e.g., DROP TABLE) must use SerialGuard for mutual exclusion"
// - "Test setup must use setup_fresh_test_app() instead of setup_test_app()"
//
// These utilities provide per-test isolated schema pools (via
// prepare_shared_test_pool / prepare_isolated_test_pool), bypassing the
// OnceCell-cached DEFAULT_APP that causes parallel test data interference.

/// Per-test isolated context: owns a fresh app + state + pool, each backed by
/// a unique schema cloned from the template. Dropping the context closes the
/// pool; schema orphaning is acceptable (PostgreSQL reclaims via namespace +
/// PID naming in `next_test_schema_name`).
///
/// Usage:
/// ```ignore
/// #[tokio::test]
/// async fn my_test() {
///     let Some(ctx) = TestContext::new().await else { return };
///     let app = &ctx.app;
///     // ... test body ...
/// }
/// ```
pub struct TestContext {
    pub app: axum::Router,
    pub state: synapse_rust::web::routes::state::AppState,
    pub pool: Arc<sqlx::PgPool>,
}

impl TestContext {
    /// Create a new isolated context using the shared template clone path
    /// (fast — ~100x faster than re-running migrations).
    pub async fn new() -> Option<Self> {
        Self::build(false).await
    }

    /// Create a new isolated context using the isolated migration path (slow,
    /// runs all migrations from scratch). Use for tests that modify schema.
    pub async fn new_isolated() -> Option<Self> {
        Self::build(true).await
    }

    async fn build(isolated: bool) -> Option<Self> {
        let pool = if isolated {
            synapse_rust::test_utils::prepare_isolated_test_pool().await.ok()?
        } else {
            synapse_rust::test_utils::prepare_shared_test_pool().await.ok()?
        };
        let cache = Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
        let container =
            synapse_services::ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone()).await;
        let state = synapse_rust::web::routes::state::AppState::new(container, cache);
        let app = synapse_rust::web::create_router(state.clone());
        Some(Self { app, state, pool })
    }
}

/// Build a fresh test app with an isolated schema pool, bypassing the OnceCell
/// cache. Each call returns a NEW app + state + pool, so tests do not share data.
///
/// This is the mandated replacement for `setup_test_app()` per project memory:
/// "Test setup must use setup_fresh_test_app() instead of setup_test_app() to
/// ensure isolated AppState and CacheManager".
pub async fn setup_fresh_test_app() -> Option<axum::Router> {
    TestContext::new().await.map(|ctx| ctx.app)
}

/// Build a fresh test app with state, bypassing the OnceCell cache.
pub async fn setup_fresh_test_app_with_state() -> Option<(axum::Router, synapse_rust::web::routes::state::AppState)> {
    TestContext::new().await.map(|ctx| (ctx.app, ctx.state))
}

/// Build a fresh test app with an isolated schema (runs migrations from scratch).
/// Use for destructive tests that modify schema (DROP TABLE, ALTER, etc.).
pub async fn setup_fresh_isolated_test_app() -> Option<axum::Router> {
    TestContext::new_isolated().await.map(|ctx| ctx.app)
}

/// Build a fresh test app with pool + cache, bypassing the OnceCell cache.
/// Drop-in replacement for `setup_test_app_with_pool()` — same return shape
/// `(Router, Arc<PgPool>, Arc<CacheManager>)` but each call gets an isolated schema.
pub async fn setup_fresh_test_app_with_pool(
) -> Option<(axum::Router, Arc<sqlx::PgPool>, Arc<synapse_rust::cache::CacheManager>)> {
    let ctx = TestContext::new().await?;
    let cache = ctx.state.cache.clone();
    Some((ctx.app, ctx.pool, cache))
}

/// Build a fresh test app with custom container configuration, bypassing the
/// OnceCell cache. Drop-in replacement for `setup_test_app_with_config()` —
/// same signature but uses an isolated schema pool instead of `get_test_pool()`.
pub async fn setup_fresh_test_app_with_config<F>(
    configure: F,
) -> Option<(axum::Router, synapse_rust::web::routes::state::AppState)>
where
    F: FnOnce(&mut synapse_services::ServiceContainer),
{
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_rust::web::routes::state::AppState;
    use synapse_services::ServiceContainer;

    let pool = synapse_rust::test_utils::prepare_shared_test_pool().await.ok()?;
    let cache = std::sync::Arc::new(CacheManager::new(&CacheConfig::default()));
    let mut container = ServiceContainer::new_test_with_pool_and_cache(pool, cache.clone()).await;
    configure(&mut container);
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state.clone());
    Some((app, state))
}

/// Retry a pool-acquiring operation with exponential backoff. Handles
/// PoolTimedOut and PoolClosed errors by retrying up to 5 times.
///
/// Project memory: "Test database connections must use acquire_with_retry
/// with exponential backoff to handle connection pool contention".
///
/// Usage:
/// ```ignore
/// let pool = ctx.pool.clone();
/// let result: sqlx::Result<i64> = acquire_with_retry(|| async move {
///     sqlx::query_scalar("SELECT 1").fetch_one(&*pool).await
/// }).await;
/// ```
pub async fn acquire_with_retry<F, Fut, T>(op: F) -> Result<T, sqlx::Error>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    let mut delay = std::time::Duration::from_millis(100);
    let max_attempts = 5;
    for attempt in 0..max_attempts {
        match op().await {
            Ok(result) => return Ok(result),
            Err(ref e) if attempt < max_attempts - 1 && is_retryable_pool_error(e) => {
                eprintln!("acquire_with_retry: attempt {} failed with {}, retrying after {:?}", attempt + 1, e, delay);
                tokio::time::sleep(delay).await;
                delay = delay.saturating_mul(2);
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

fn is_retryable_pool_error(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed) || error.to_string().contains("connection")
}

/// Mutual exclusion guard for destructive tests (DROP TABLE, TRUNCATE, etc.)
/// that cannot run in parallel with other tests.
///
/// Project memory: "Destructive tests (e.g., DROP TABLE) must use SerialGuard
/// for mutual exclusion".
///
/// Usage:
/// ```ignore
/// #[tokio::test]
/// async fn destructive_test() {
///     let _guard = serial_guard().await;
///     // ... destructive operations ...
/// }
/// ```
static SERIAL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct SerialGuard {
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

pub async fn serial_guard() -> SerialGuard {
    SerialGuard { _guard: SERIAL_TEST_LOCK.lock().await }
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
    let nonce_request =
        Request::builder().method("GET").uri("/_synapse/admin/v1/register/nonce").body(Body::empty()).unwrap();

    let nonce_response = app.clone().oneshot(with_local_connect_info(nonce_request)).await.unwrap();
    let nonce_body = axum::body::to_bytes(nonce_response.into_body(), 1024).await.unwrap();
    let nonce_json: serde_json::Value = serde_json::from_slice(&nonce_body).unwrap();
    let nonce = nonce_json["nonce"].as_str().unwrap();

    // Step 2: Calculate HMAC
    let shared_secret =
        std::env::var("REGISTRATION_SHARED_SECRET").unwrap_or_else(|_| "test_shared_secret".to_string());

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

    let response = app.clone().oneshot(with_local_connect_info(register_request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
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
    use uuid::Uuid;

    let suffix = Uuid::new_v4().simple().to_string();

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": format!("user_{}", suffix),
                "password": "UserTest@123",
                "device_id": format!("DEV{}", &suffix[..12]),
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "test user registration failed with status {}: {}",
        status,
        String::from_utf8_lossy(&body)
    );
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}
