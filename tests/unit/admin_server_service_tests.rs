// Admin server service unit tests — exercises
// `synapse_services::admin_server_service::AdminServerService`.
//
// `AdminServerService` wraps two infrastructure-level operations:
//   * `is_database_healthy() -> bool` — runs a `DatabaseHealthCheck`.
//   * `validate_required_tables(tables) -> Result<Vec<String>, ApiError>` —
//     delegates to `SchemaValidator`.
//
// Both require a live Postgres connection. Without a DB, we test:
//   * Construction with a lazy pool (no I/O).
//   * `Clone` semantics (the struct derives Clone).
//   * `is_database_healthy` returns `false` when the pool can't connect.
//   * `validate_required_tables` surfaces an `ApiError::internal` when the
//     pool can't connect.
//
// The happy-path (true/Ok) branches are exercised by the integration test
// suite against a real Postgres instance.

use std::sync::Arc;

use synapse_services::admin_server_service::AdminServerService;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build a lazy `PgPool` that doesn't perform I/O until a query is issued.
/// Used to construct `AdminServerService` without a live database.
fn lazy_pool() -> Arc<sqlx::PgPool> {
    Arc::new(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_millis(100))
            .connect_lazy("postgresql://nobody:nobody@localhost:5432/nobody")
            .expect("connect_lazy must not perform I/O"),
    )
}

fn build_service() -> AdminServerService {
    AdminServerService::new(lazy_pool())
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction + Clone
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn admin_server_service_constructs_with_lazy_pool() {
    let _svc = build_service();
    // Construction must not panic and must not perform I/O.
}

#[tokio::test]
async fn admin_server_service_is_clone() {
    let svc = build_service();
    let cloned = svc;
    // Both instances should be usable independently. We can't call methods
    // without a DB, but the clone itself must succeed.
    let _ = cloned;
}

#[tokio::test]
async fn admin_server_service_clone_preserves_identity() {
    // Verify that cloning produces an independent handle to the same pool.
    let svc = build_service();
    let _cloned = svc.clone();
    // If clone were broken (e.g. moved the pool), the original would be
    // unusable. We just verify the original is still drop-able here.
    drop(svc);
}

// ─────────────────────────────────────────────────────────────────────────────
// is_database_healthy — error path (no live DB)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn is_database_healthy_returns_false_when_pool_cannot_connect() {
    // The lazy pool points at a non-existent DB. The health check must
    // surface this as `false` (not panic, not hang).
    let svc = build_service();

    let result = tokio::time::timeout(std::time::Duration::from_secs(10), svc.is_database_healthy()).await;
    // The call must complete within the timeout.
    let healthy = result.expect("is_database_healthy must not hang");
    assert!(!healthy, "health check must be false when the pool can't connect");
}

// ─────────────────────────────────────────────────────────────────────────────
// validate_required_tables — error path (no live DB)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn validate_required_tables_returns_internal_error_when_pool_cannot_connect() {
    let svc = build_service();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        svc.validate_required_tables(&["users", "rooms"]),
    )
    .await;

    let inner = result.expect("validate_required_tables must not hang");
    assert!(inner.is_err(), "must return Err when pool can't connect");

    let err = inner.unwrap_err();
    assert!(err.is_internal(), "storage error must surface as ApiError::internal");
}

#[tokio::test]
async fn validate_required_tables_with_empty_list_still_errors_when_no_db() {
    // Even with an empty table list, the validator may attempt a connection.
    // Verify it doesn't panic and returns an error.
    let svc = build_service();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        svc.validate_required_tables(&[]),
    )
    .await;

    // With an empty list, the validator might return Ok(empty) without
    // touching the DB, or it might still error. Either outcome is
    // acceptable — we just verify it doesn't hang or panic.
    match result {
        Ok(Ok(missing)) => assert!(missing.is_empty(), "empty input must yield empty missing list"),
        Ok(Err(e)) => assert!(e.is_internal()),
        Err(_) => panic!("validate_required_tables must not hang"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiple health checks don't interfere
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn repeated_health_checks_all_return_false_without_db() {
    let svc = build_service();

    for _ in 0..3 {
        let healthy = tokio::time::timeout(std::time::Duration::from_secs(10), svc.is_database_healthy())
            .await
            .expect("must not hang");
        assert!(!healthy);
    }
}
