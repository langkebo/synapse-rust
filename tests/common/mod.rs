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
    // Fail closed under CI: a wrong TEST_DATABASE_URL must not be papered over by the
    // hard-coded localhost fallback. See synapse_common::test_isolation::test_db_fallback_allowed.
    if !synapse_common::test_isolation::test_db_fallback_allowed() {
        return urls;
    }
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

/// Installs the schema janitor's process-exit drain for this test binary (B').
///
/// `libc::atexit` is `unsafe`, so the registration lives in the *test target*
/// rather than in `synapse-common`'s production code: cargo-geiger's production
/// scan then sees no `unsafe` there, while its `--include-tests` scan does (the
/// unit lands in the test-only delta). Runtime behaviour is unchanged — the drain
/// still runs at process exit, so no schema is left behind.
///
/// Every test binary that can register schemas must reach the registration: a
/// `#[cfg(test)]` item in a dependency is invisible to the dependent's test
/// build. The body is therefore `#[path]`-included from one shared file rather
/// than copied per test target.
/// Guard: `tests/unit/ci_test_scope_tests.rs` →
/// `every_db_test_binary_registers_the_exit_drain`.
#[path = "../../synapse-common/tests-support/exit_hook_impl.rs"]
mod exit_hook;

pub fn ensure_schema_exit_hook() {
    exit_hook::ensure()
}

/// 有界条件轮询：条件成立即返回，超时返回带「等什么 / 等多久」的错误。
///
/// 为什么要有它（NEW-10）：此前 4 处需要"等后台任务跑完"的地方各写一遍
/// `timeout(3s, loop { if cond { break } sleep(50ms) })` 或手搓 deadline，
/// 于是同一件事有 4 种超时文案 —— 失败时只能从中读出"某个东西没等到"，
/// 读不出**等的是什么、等了多久**，只能翻代码。这里把这两个事实变成必填参数。
///
/// 间隔固定 25ms：原先是 50ms / 20ms 混用，对"等后台任务"这类场景没有区别，
/// 统一之后站点从 4 处降到 1 处（`check_sleep_ratchet.py` 的测试域计数随之下降）。
pub async fn wait_until<F, Fut>(what: &str, budget: Duration, mut condition: F) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    const POLL: Duration = Duration::from_millis(25);
    let deadline = tokio::time::Instant::now() + budget;

    loop {
        if condition().await {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("等待「{what}」超时（{budget:?}）：条件始终不成立"));
        }
        // sleep-reason: 条件轮询的间隔（上方已有 deadline 与条件判断）
        tokio::time::sleep(POLL).await;
    }
}

pub async fn get_test_pool_async() -> Result<Arc<Pool<Postgres>>, String> {
    ensure_schema_exit_hook();
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
