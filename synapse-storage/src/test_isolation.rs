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

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;

/// First non-empty line of a statement, for warn-log context.
fn first_line(s: &str) -> &str {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
}

/// Removes `COPY ... FROM stdin;` ... `\.` blocks (test data seeding) from a
/// migration file.  The seed data is not needed for isolated schemas, and the
/// bare data lines would otherwise be executed as (failing) statements.
fn strip_copy_blocks(sql: &str) -> String {
    let mut out = String::new();
    let mut in_copy = false;
    for line in sql.split_inclusive('\n') {
        let t = line.trim_start();
        if !in_copy && t.starts_with("COPY") && t.contains("FROM stdin") {
            in_copy = true;
            continue;
        }
        if in_copy {
            if t.starts_with("\\.") {
                in_copy = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

/// Splits SQL text into individual statements, correctly handling:
/// - `--` line comments and `/* */` block comments (skipped, not emitted)
/// - `'...'` string literals (with `''` escapes)
/// - `"..."` quoted identifiers (with `""` escapes)
/// - `$$...$$` / `$tag$...$tag$` dollar-quoted bodies (functions, DO blocks)
/// - `;` statement terminators outside all of the above
///
/// Unlike a naive `split(';')`, a chunk that begins with a comment line is not
/// dropped together with the statements that follow it.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let n = chars.len();
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut i = 0;

    while i < n {
        let c = chars[i];

        // `--` line comment: skip to end of line.
        if c == '-' && i + 1 < n && chars[i + 1] == '-' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // `/* ... */` block comment.
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }

        // `'...'` string literal with `''` escapes.
        if c == '\'' {
            current.push(c);
            i += 1;
            while i < n {
                if chars[i] == '\'' {
                    if i + 1 < n && chars[i + 1] == '\'' {
                        current.push('\'');
                        current.push('\'');
                        i += 2;
                        continue;
                    }
                    current.push('\'');
                    i += 1;
                    break;
                }
                current.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // `"..."` quoted identifier with `""` escapes.
        if c == '"' {
            current.push(c);
            i += 1;
            while i < n {
                if chars[i] == '"' {
                    if i + 1 < n && chars[i + 1] == '"' {
                        current.push('"');
                        current.push('"');
                        i += 2;
                        continue;
                    }
                    current.push('"');
                    i += 1;
                    break;
                }
                current.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // `$tag$ ... $tag$` dollar-quoted body.
        if c == '$' {
            let mut j = i + 1;
            while j < n && chars[j] != '$' {
                j += 1;
            }
            if j < n {
                let tag: String = chars[i..=j].iter().collect();
                let tag_len = tag.len();
                current.push_str(&tag);
                let body_start = j + 1;
                i = body_start;
                let mut found = false;
                while i + tag_len <= n {
                    if chars[i..i + tag_len].iter().collect::<String>() == tag {
                        // Push the body AND the closing tag, not just the tag.
                        let seg: String = chars[body_start..i + tag_len].iter().collect();
                        current.push_str(&seg);
                        i += tag_len;
                        found = true;
                        break;
                    }
                    i += 1;
                }
                if !found {
                    let seg: String = chars[body_start..].iter().collect();
                    current.push_str(&seg);
                    i = n;
                }
                continue;
            }
            // Lone `$` — keep as ordinary character.
            current.push(c);
            i += 1;
            continue;
        }

        // Statement terminator outside strings/comments/dollar bodies.
        if c == ';' {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(current);
            }
            current = String::new();
            i += 1;
            continue;
        }

        current.push(c);
        i += 1;
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(current);
    }

    statements
}

/// Creates a pool connected to a fresh isolated schema per test.
/// Each schema is created from the v11 baseline and dropped on Drop.
pub struct IsolatedTestPool {
    pool: Arc<PgPool>,
    schema: String,
}

impl IsolatedTestPool {
    /// Create a new isolated test pool with a unique schema.
    pub async fn new() -> Result<Self, sqlx::Error> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse_test".to_string());

        // Admin pool to create/drop schema.  We must NOT share a connection
        // between concurrent IsolatedTestPool::new() callers, because each
        // test sets `search_path` on its connection and parallel baseline
        // application can otherwise leak schema state between tests.
        // max_connections is kept at 2: each call only ever acquires one
        // admin connection (for baseline application); a large cap is
        // over-provisioning that adds parallel pressure on the Postgres
        // `max_connections` limit when many tests initialize schemas at once.
        let admin_pool =
            PgPoolOptions::new().max_connections(2).acquire_timeout(Duration::from_secs(60)).connect(&db_url).await?;

        let schema = format!("test_{}", uuid::Uuid::new_v4().as_simple());

        // Create isolated schema
        sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, schema)).execute(&admin_pool).await?;

        // Clone v11 baseline into the new schema.  Use a dedicated connection
        // (acquired once) so concurrent IsolatedTestPool::new() callers don't
        // stomp on each other's `search_path`.
        let baseline_sql = include_str!("../../migrations/00000000_unified_schema_v11.sql");
        let extensions_sql = include_str!("../../migrations/00000001_extensions_v10.sql");

        let mut admin_conn = admin_pool.acquire().await?;
        let set_path = format!(r#"SET search_path TO "{}", public"#, schema);
        sqlx::query(&set_path).execute(&mut *admin_conn).await?;

        // The v11 baseline contains `$$...$$` function/DO bodies, string
        // literals and comments.  A naive `split(';')` chops function bodies
        // at inner `;` and — worse — a chunk that *starts* with a `--`
        // comment line carries the following statements with it, silently
        // dropping whole tables (e.g. `users`) from the isolated schema.
        // Queries for those tables then fall back to the shared `public`
        // schema via the search_path, polluting it and breaking UNIQUE
        // constraints under parallel execution.  Use a proper splitter.
        let baseline_sql = strip_copy_blocks(baseline_sql);
        for stmt in split_sql_statements(&baseline_sql) {
            let trimmed = stmt.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Err(e) = sqlx::query(trimmed).execute(&mut *admin_conn).await {
                // Do NOT silently swallow: a failing baseline statement leaves
                // the isolated schema incomplete and later queries silently
                // fall back to the shared `public` schema.
                tracing::warn!(schema = %schema, "baseline statement failed: {e} | stmt head: {}", first_line(trimmed));
            }
        }

        for stmt in split_sql_statements(extensions_sql) {
            let trimmed = stmt.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Err(e) = sqlx::query(trimmed).execute(&mut *admin_conn).await {
                tracing::warn!(schema = %schema, "extensions statement failed: {e} | stmt head: {}", first_line(trimmed));
            }
        }
        drop(admin_conn);

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

impl Drop for IsolatedTestPool {
    fn drop(&mut self) {
        let schema = self.schema.clone();
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse_test".to_string());

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");

            rt.block_on(async {
                let pool = match PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(Duration::from_secs(10))
                    .connect(&db_url)
                    .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Failed to connect for schema cleanup: {}", e);
                        return;
                    }
                };

                let drop_sql = format!(r#"DROP SCHEMA "{}" CASCADE"#, schema);
                match sqlx::query(&drop_sql).execute(&pool).await {
                    Ok(_) => tracing::debug!("Dropped test schema {}", schema),
                    Err(e) => tracing::error!("Failed to drop test schema {}: {}", schema, e),
                }
            });
        });
    }
}

#[cfg(test)]
mod search_path_tests {
    use super::*;

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
