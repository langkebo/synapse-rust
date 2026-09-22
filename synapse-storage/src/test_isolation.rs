//! Schema isolation for test pools.
//!
//! Storage's entry point to the shared per-test isolated pool. The pool
//! lifecycle (struct, construction, janitor-registered cleanup) lives in
//! [`synapse_common::test_isolation::IsolatedTestPool`] because that module is
//! compiled unconditionally and is therefore reachable from every sibling
//! crate's `#[cfg(test)]` fixtures — a `#[cfg(test)]` module here is not.
//!
//! Usage:
//! ```ignore
//! // Replace:
//! let pool = test_pool().await;
//!
//! // With:
//! let test_pool = isolated_test_pool().await.unwrap();
//! let pool = test_pool.pool();
//! // Schema is auto-dropped when test_pool is dropped
//! ```
//!
//! The template-schema machinery (fingerprint, naming, advisory lock, build,
//! single-round-trip clone, inventory validation) also lives in
//! [`synapse_common::test_isolation`]. This module is the `synapse-storage`
//! adapter: it owns the storage-local copy of the baseline SQL and supplies it
//! to the shared pool.

pub use synapse_common::test_isolation::IsolatedTestPool;

/// Baseline SQL for isolated schemas, handed to the shared pool constructor.
///
/// The exact bytes are load-bearing: the template name is a content
/// fingerprint of this string, so any edit forks a second template (the shared
/// `ensure_template_schema` rebuilds from scratch) instead of reusing the one
/// already in the database.
///
/// The v12 baseline already contains every object the former
/// `00000001_extensions_v10.sql` defined (14 tables + 1 index, all
/// `IF NOT EXISTS`), so that file was a byte-for-byte no-op duplicate and was
/// deleted — see migrations/README.md §"为什么只有一个 baseline".
///
/// It stays here, rather than being embedded by `synapse-common`, because that
/// crate must not compile the workspace migrations into its production build.
fn isolated_baseline_sql() -> &'static str {
    include_str!("../../migrations/00000000_unified_schema_v12.sql")
}

/// Create a pool backed by a fresh schema cloned from the shared v12 template.
///
/// Every DB test in this crate should start here: the schema is dropped on
/// `Drop`, so tests cannot leak state into each other or into `public`.
pub async fn isolated_test_pool() -> Result<IsolatedTestPool, sqlx::Error> {
    #[cfg(test)]
    crate::test_exit_hook::ensure();
    IsolatedTestPool::new(isolated_baseline_sql()).await
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
        let iso = isolated_test_pool().await.expect("isolated pool");
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
        let iso = isolated_test_pool().await.expect("isolated pool");
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
