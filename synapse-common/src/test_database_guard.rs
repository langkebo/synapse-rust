//! Cleanup helpers for **database-level** test isolation.
//!
//! The schema-level guard in [`crate::test_schema_guard`] identifies "still in
//! use" with a connection lease (a shared advisory lock). That mechanism does
//! not carry over to databases: PostgreSQL advisory locks are scoped to the
//! database they were taken in, so a lease taken inside `test_…` is invisible
//! from the maintenance connection that has to run `DROP DATABASE`.
//!
//! The equivalent question — "is anything still connected to this database?" —
//! is answered directly by `pg_stat_activity`, which is cluster-wide. That is
//! what [`drop_database_if_unused_blocking`] does, and why it can report
//! [`CleanupOutcome::Retry`] without sharing any state with the test process.
//!
//! Both functions connect to the **maintenance database** (`postgres`), never to
//! the database being dropped: PostgreSQL refuses `DROP DATABASE` on the
//! session's own database, and the test pool is about to be torn down.

use crate::test_schema_guard::{CleanupOutcome, SchemaCleanup};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Cleanup pair for a test **database**: drop it when the last connection is
/// gone, and unconditionally at process exit.
///
/// Registered through the same registry as schema cleanups
/// ([`crate::test_schema_guard::register_schema_cleanup`]) so the janitor, the
/// atexit drain and the `Retry` re-queue work unchanged.
pub fn database_cleanup(database_url: &str, database: &str) -> SchemaCleanup {
    let release_url = database_url.to_string();
    let release_name = database.to_string();
    let exit_url = database_url.to_string();
    let exit_name = database.to_string();
    SchemaCleanup {
        on_release: Box::new(move || drop_database_if_unused_blocking(&release_url, &release_name)),
        on_exit: Box::new(move || {
            drop_database_blocking(&exit_url, &exit_name);
            CleanupOutcome::Done
        }),
    }
}

/// How long a cleanup connection may take to appear before we give up and leave
/// the database to the next run's sweep.
const CLEANUP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Return `database_url` with its database component replaced by `database`.
///
/// The connection string is user-supplied (env var, CI secret), so this only
/// rewrites the path segment and preserves scheme, credentials, host, port and
/// query parameters. A URL without `://` is returned unchanged.
pub fn with_database_name(database_url: &str, database: &str) -> String {
    let Some(scheme_end) = database_url.find("://") else {
        return database_url.to_string();
    };
    let after_scheme = &database_url[scheme_end + 3..];
    let authority_end = after_scheme.find('/').unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    let tail = &after_scheme[authority_end..];
    let query = tail.find('?').map_or("", |index| &tail[index..]);
    format!("{}://{}/{}{}", &database_url[..scheme_end], authority, database, query)
}

/// `DROP DATABASE "{name}" WITH (FORCE)` through a short-lived maintenance
/// connection.
///
/// Used on the exit path, where "someone is still connected" is not a reason to
/// keep the database: those connections belong to a process that is ending.
pub fn drop_database_blocking(database_url: &str, database: &str) {
    let maintenance = with_database_name(database_url, "postgres");
    let database = database.to_string();
    crate::test_schema_guard::run_cleanup_blocking(async move {
        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(CLEANUP_CONNECT_TIMEOUT)
            .connect(&maintenance)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("test database cleanup: could not connect to drop {database}: {error}");
                return;
            }
        };
        if let Err(error) =
            sqlx::query(&format!(r#"DROP DATABASE IF EXISTS "{database}" WITH (FORCE)"#)).execute(&pool).await
        {
            eprintln!("test database cleanup: failed to drop {database}: {error}");
        }
        pool.close().await;
    });
}

/// Drop `database` only when no session is connected to it.
///
/// `Retry` means "this database is still here, someone should try again later".
/// Two different situations produce it:
///
/// * a connection is still open — normally the test's own pool that is being
///   torn down — so a later pass can simply succeed; and
/// * the maintenance connection or the `DROP DATABASE` itself **failed**.
///
/// ## Why failures are `Retry` and not `Done` (2026-10-28)
///
/// This used to return `Done` for both failure paths, justified by "retrying a
/// database outage forever would turn the janitor into a hot loop, and the next
/// run's sweep can still reclaim the database". Two things are wrong with that:
///
/// 1. `Retry` is **not** a hot loop: `run_release_cleanups_requeueing` puts the
///    entry back with `LEASE_RETRY_BACKOFF` before the next pass can claim it;
///    2. "the next run's sweep" does not exist on a one-shot CI runner. The
///    container is destroyed with the job, so a `Done` here is silently the
///    **last** word on a database that never got dropped.
///
/// Falling back to `Retry` costs nothing and buys two things: the in-run
/// janitor keeps trying (which is what actually helps after a transient
/// `out of shared memory` / `too many clients`), and the process-exit drain
/// runs the **unconditional** [`drop_database_blocking`] (`WITH (FORCE)`) as a
/// last resort — `Done` removes the entry from the registry, so that drain never
/// saw it.
pub fn drop_database_if_unused_blocking(database_url: &str, database: &str) -> CleanupOutcome {
    let maintenance = with_database_name(database_url, "postgres");
    let database = database.to_string();
    crate::test_schema_guard::run_cleanup_blocking_outcome(async move {
        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(CLEANUP_CONNECT_TIMEOUT)
            .connect(&maintenance)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("test database cleanup: could not connect to drop {database}: {error}");
                return CleanupOutcome::Retry;
            }
        };
        let in_use: Result<i64, _> =
            sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()")
                .bind(&database)
                .fetch_one(&pool)
                .await;
        let outcome = match in_use {
            Ok(0) => {
                match sqlx::query(&format!(r#"DROP DATABASE IF EXISTS "{database}" WITH (FORCE)"#)).execute(&pool).await
                {
                    Ok(_) => CleanupOutcome::Done,
                    Err(error) => {
                        eprintln!("test database cleanup: failed to drop {database}: {error}");
                        CleanupOutcome::Retry
                    }
                }
            }
            Ok(_) => CleanupOutcome::Retry,
            Err(error) => {
                eprintln!("test database cleanup: could not check sessions on {database}: {error}");
                CleanupOutcome::Retry
            }
        };
        pool.close().await;
        outcome
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_database_name_preserves_everything_but_the_path() {
        assert_eq!(
            with_database_name("postgresql://u:p@h:55432/synapse_test", "postgres"),
            "postgresql://u:p@h:55432/postgres"
        );
        // Query parameters must survive: sslmode and friends change the connection.
        assert_eq!(
            with_database_name("postgresql://u:p@h:55432/synapse_test?sslmode=require", "postgres"),
            "postgresql://u:p@h:55432/postgres?sslmode=require"
        );
        // No database component: the path is still replaced deterministically.
        assert_eq!(with_database_name("postgresql://u:p@h:55432", "postgres"), "postgresql://u:p@h:55432/postgres");
        // A string that is not a URL is passed through rather than mangled.
        assert_eq!(with_database_name("host=localhost dbname=x", "postgres"), "host=localhost dbname=x");
    }
}
