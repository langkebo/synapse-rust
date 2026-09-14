//! Schema isolation for test pools.
//!
//! Provides `TestPool` wrapper that connects to a dedicated test schema,
//! isolating each test from `pg_stat_*` and data pollution from other tests.
//!
//! Usage:
//! ```ignore
//! // Replace:
//! let pool = test_pool().await;
//!
//! // With:
//! let test_pool = IsolatedTestPool::new().await;
//! let pool = test_pool.pool();
//! // Schema is auto-dropped when test_pool is dropped
//! ```
//!
//! The template-schema machinery (fingerprint, naming, advisory lock, build,
//! single-round-trip clone, inventory validation) lives in
//! [`synapse_common::test_isolation`]. This module is the `synapse-storage`
//! adapter: it owns test-DB URL resolution and the per-test pool lifecycle,
//! and delegates the shared work.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;

/// Resolve the test database URL.
///
/// Precedence: `TEST_DATABASE_URL`, then `DATABASE_URL`, then the documented
/// local convention. The env-var path is deliberately zero-probe so the hot
/// path never pays connection-probe latency.
///
/// Storage keeps its own resolver on purpose: the shared module deliberately
/// does not provide one, because its callers do not agree on a fallback chain.
fn test_database_url() -> String {
    if let Ok(url) = std::env::var("TEST_DATABASE_URL") {
        return url;
    }
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return url;
    }
    for candidate in [
        "postgresql://synapse:synapse@localhost:15432/synapse_test",
        "postgresql://synapse:synapse@localhost:15432/synapse",
        "postgresql://synapse:synapse@localhost:5432/synapse_test",
        "postgresql://synapse:synapse@localhost:5432/synapse",
    ] {
        if tcp_reachable(candidate) {
            return candidate.to_string();
        }
    }
    "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
}

/// Cheap synchronous reachability probe for a Postgres URL's host:port.
fn tcp_reachable(url: &str) -> bool {
    let Some(authority) = url.split("://").nth(1).and_then(|rest| rest.split('/').next()) else {
        return false;
    };
    let Some(host_port) = authority.rsplit('@').next() else {
        return false;
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().unwrap_or(5432)),
        None => (host_port, 5432),
    };
    let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(host, port)) else {
        return false;
    };
    addrs.into_iter().any(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok())
}

/// Creates a pool connected to a fresh isolated schema per test.
/// Each schema is cloned from the shared v11 baseline template and dropped on Drop.
pub struct IsolatedTestPool {
    pool: Arc<PgPool>,
    schema: String,
}

impl IsolatedTestPool {
    /// Create a new isolated test pool with a unique schema.
    pub async fn new() -> Result<Self, sqlx::Error> {
        let db_url = test_database_url();

        // The template name is a content fingerprint of this string, so the
        // concatenation is load-bearing: `v11 ++ extensions` (no separator)
        // yields `bec240fb79ed438b`, the existing real-baseline template.
        // Inserting a separator or swapping the order silently forks a second
        // template (the shared `ensure_template_schema` would build it from
        // scratch) instead of reusing the one already in the database.
        let baseline_sql = concat!(
            include_str!("../../migrations/00000000_unified_schema_v11.sql"),
            include_str!("../../migrations/00000001_extensions_v10.sql"),
        );
        let template = synapse_common::test_isolation::ensure_template_schema(&db_url, baseline_sql)
            .await
            .map_err(sqlx::Error::Protocol)?;

        let schema = format!("test_{}", uuid::Uuid::new_v4().as_simple());

        // One round trip: every table, index, constraint, function, view and
        // trigger. The shared clone helper does not create the schema, so the
        // caller creates it and puts it first on the session `search_path`.
        let clone_pool =
            PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(60)).connect(&db_url).await?;
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&clone_pool).await?;
        sqlx::query(&format!(r#"SET search_path TO "{schema}", public"#)).execute(&clone_pool).await?;
        synapse_common::test_isolation::clone_schema_from_template(
            &clone_pool,
            &schema,
            &template,
            synapse_common::test_isolation::SeedSource::Everything,
        )
        .await
        .map_err(sqlx::Error::Protocol)?;
        drop(clone_pool);

        // Create test pool with isolated search_path.  Use `connect_lazy` so we
        // can also run a `SET search_path` on the first connection *before* any
        // other query.  `after_connect` only fires for connections acquired
        // from the pool after the initial `connect()` (which would otherwise
        // default to the `public` schema and leak data across parallel tests).
        //
        // Connection budget is deliberately minimal: db_tests issue queries
        // serially, so one connection suffices, and every pool holds its
        // connection open until the test ends.  With dozens of tests creating
        // isolated pools in parallel, an idle connection pool with a large
        // cap / no idle timeout can exceed the Postgres `max_connections`
        // limit (100) and make unrelated `test_pool()` connections fail.
        let pool_schema = schema.clone();
        let set_path_for_pool = format!(r#"SET search_path TO "{}", public"#, schema);
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Some(Duration::from_secs(60)))
            .after_connect(move |conn, _| {
                let schema = pool_schema.clone();
                Box::pin(async move {
                    sqlx::query(&format!(r#"SET search_path TO "{}", public"#, schema)).execute(conn).await?;
                    Ok(())
                })
            })
            .connect_lazy(&db_url)?;

        // Force a connection acquisition and immediately set the search_path.
        // This ensures even the very first connection (which bypasses
        // `after_connect`) lands in the correct schema.  Pre-warm the single
        // pooled connection so no later connection (created while the test is
        // running) can silently default to the `public` schema and leak data
        // across parallel tests.
        let mut conn = pool.acquire().await?;
        sqlx::query(&set_path_for_pool).execute(&mut *conn).await?;
        drop(conn);

        Ok(Self { pool: Arc::new(pool), schema })
    }

    /// Get the underlying pool.
    pub fn pool(&self) -> Arc<PgPool> {
        self.pool.clone()
    }

    /// Get the schema name for debugging.
    pub fn schema_name(&self) -> &str {
        &self.schema
    }
}

// NOTE on cleanup strategy (2026-09-11)
//
// `Drop::drop` is synchronous and cannot await, so schema cleanup must be
// delegated. Three approaches were tried:
//
//   1. `std::thread::spawn` + block_on — **leaked 100%**. Under nextest (one
//      process per test) the process exits before the thread reaches Postgres.
//   2. `LazyLock<Runtime>::spawn` — **also leaked 100%**. Dropping the runtime
//      at process exit *cancels* in-flight async tasks rather than awaiting
//      them, so the `DROP SCHEMA` never ran.
//   3. Spawn a thread and **join it** before `drop` returns — this is the only
//      variant that guarantees the schema is gone before the process exits.
//      It costs a connect + DROP per test, which is the price of not
//      accumulating schemas.
//
// Measured: 24 isolated tests leaked exactly 24 schemas under (1) and (2); the
// local database had accumulated 22,532 `test_*` schemas.

impl Drop for IsolatedTestPool {
    fn drop(&mut self) {
        let schema = self.schema.clone();
        let db_url = test_database_url();

        // Never drop the shared template. `new()` only ever puts a `test_<uuid>`
        // schema in `self.schema`, so this guards a future refactor rather than
        // a reachable path today.
        if schema.starts_with("test_isolation_template_") {
            tracing::error!("refusing to drop the shared isolation template schema {schema}");
            return;
        }

        // Spawn a thread and JOIN it: dropping the schema must complete before
        // this returns, otherwise process exit races the cleanup and leaks the
        // schema (measured 100% leak with both fire-and-forget variants).
        let handle = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                return;
            };
            rt.block_on(async {
                let Ok(pool) = PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(Duration::from_secs(10))
                    .connect(&db_url)
                    .await
                else {
                    return;
                };
                let drop_sql = format!(r#"DROP SCHEMA IF EXISTS "{}" CASCADE"#, schema);
                if let Err(e) = sqlx::query(&drop_sql).execute(&pool).await {
                    tracing::error!("Failed to drop test schema {}: {}", schema, e);
                }
            });
        });

        // If the cleanup thread panicked, do not propagate from `drop`
        // (a panic during unwinding would abort the process).
        let _ = handle.join();
    }
}

#[cfg(test)]
mod search_path_tests {
    use super::*;
    use synapse_common::test_isolation::{first_line, split_sql_statements, strip_copy_blocks};

    // The pure SQL-parsing tests now exercise the shared implementation (the
    // local copies were deleted in this refactor). They stay here rather than
    // being dropped so the storage suite keeps direct coverage of the parsers
    // it feeds its baseline through.

    #[test]
    fn split_handles_functions_do_blocks_and_comments() {
        let sql = r#"
-- leading comment before the users table
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL,
    CONSTRAINT pk_users PRIMARY KEY (user_id)
);

CREATE OR REPLACE FUNCTION update_updated_ts_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = now();
    RETURN NEW; -- inner semicolon inside dollar body
END;
$$ LANGUAGE plpgsql;

DO $$
BEGIN
    INSERT INTO users (user_id) VALUES ('a;b'); -- semicolon inside a string
END $$;

/* block
   comment */
CREATE TABLE IF NOT EXISTS devices (
    id BIGSERIAL, -- trailing comment
    note TEXT DEFAULT 'it;s;fine'
);
"#;
        let stmts = split_sql_statements(sql);
        let heads: Vec<&str> = stmts.iter().map(|s| first_line(s)).collect();
        assert_eq!(
            heads,
            vec![
                "CREATE TABLE IF NOT EXISTS users (",
                "CREATE OR REPLACE FUNCTION update_updated_ts_column()",
                "DO $$",
                "CREATE TABLE IF NOT EXISTS devices ("
            ]
        );
        // The users CREATE TABLE must survive the preceding comment chunk.
        assert!(stmts.iter().any(|s| s.trim_start().starts_with("CREATE TABLE IF NOT EXISTS users")));
        // Function body must stay in one piece (no cut at inner `;`).
        assert!(stmts.iter().any(|s| s.contains("RETURN NEW") && s.contains("$$ LANGUAGE plpgsql")));
        // String containing semicolons must not split the DO block.
        assert!(stmts.iter().any(|s| s.contains("INSERT INTO users (user_id) VALUES ('a;b')")));
    }

    #[test]
    fn split_handles_quoted_identifiers_and_escapes() {
        let sql = r#"
CREATE TABLE "my;table" (id TEXT);
INSERT INTO t VALUES ('it''s;here');
"#;
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("\"my;table\""));
        assert!(stmts[1].contains("'it''s;here'"));
    }

    #[test]
    fn strip_copy_blocks_removes_seed_data() {
        let sql = "CREATE TABLE t (id INT);\nCOPY t (id) FROM stdin;\n1\n2\n\\.\nCREATE TABLE u (id INT);";
        let cleaned = strip_copy_blocks(sql);
        assert!(!cleaned.contains("COPY"));
        assert!(!cleaned.contains("FROM stdin"));
        assert!(cleaned.contains("CREATE TABLE t"));
        assert!(cleaned.contains("CREATE TABLE u"));
        assert!(split_sql_statements(&cleaned).len() == 2);
    }

    #[tokio::test]
    async fn test_isolated_pool_search_path_is_schema() {
        let iso = IsolatedTestPool::new().await.expect("isolated pool");
        let pool = iso.pool();
        let (schema,): (String,) =
            sqlx::query_as("SELECT current_schema()").fetch_one(&*pool).await.expect("query current_schema");
        assert_eq!(
            schema,
            iso.schema_name(),
            "isolated pool connection should resolve current_schema to the isolated schema, got {schema}"
        );
    }

    /// Regression test: the naive `split(';')` + `starts_with("--")` skip used
    /// to drop the whole `CREATE TABLE users` chunk (it followed a comment
    /// block), so `users` was missing from every isolated schema and queries
    /// silently fell back to the shared `public` schema.  The isolated schema
    /// must contain the core user tables.
    #[tokio::test]
    async fn test_isolated_schema_contains_core_user_tables() {
        let iso = IsolatedTestPool::new().await.expect("isolated pool");
        let pool = iso.pool();
        let schema = iso.schema_name();
        let (users,): (Option<String>,) = sqlx::query_as("SELECT to_regclass($1)::text")
            .bind(format!(r#""{}".users"#, schema))
            .fetch_one(&*pool)
            .await
            .expect("query to_regclass");
        assert!(
            users.is_some(),
            "isolated schema {schema} is missing the `users` table — baseline apply is dropping it"
        );

        // Inserting a user must land in the isolated schema, never `public`.
        let user_id = format!("@iso_{}:example.com", uuid::Uuid::new_v4());
        let username = format!("isouser_{}", uuid::Uuid::new_v4().as_simple());
        let now = 1_700_000_000_000i64;
        let inserted: i64 = sqlx::query_scalar(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) RETURNING 1::bigint",
        )
        .bind(&user_id)
        .bind(&username)
        .bind(now)
        .fetch_one(&*pool)
        .await
        .expect("insert into isolated users table");
        assert_eq!(inserted, 1);

        let (found,): (String,) = sqlx::query_as("SELECT username FROM users WHERE user_id = $1")
            .bind(&user_id)
            .fetch_one(&*pool)
            .await
            .expect("read back inserted user");
        assert_eq!(found, username);
    }
}
