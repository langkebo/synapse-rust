//! Shared test-isolation fixture: template schema + single-round-trip clone.
//!
//! ## Why this module exists
//!
//! Two crates grew separate schema-per-test fixtures:
//!
//! * `synapse-storage::test_isolation` replayed the whole v11 baseline
//!   statement-by-statement **on every call** (measured: `baseline_replay`
//!   median 4.416s, 99% of fixture cost), then was changed to clone a template.
//! * `synapse-services::test_utils::prepare_isolated_test_pool` created an
//!   *empty* schema and let the runtime `DatabaseInitService` fill it. That
//!   initializer does not create every baseline table (e.g. the retention
//!   tables), so queries silently fell back to the shared `public` schema via
//!   `search_path = <schema>, public` and tests leaked state into each other.
//!
//! One implementation, two callers. The baseline SQL is passed in by the
//! caller because the migration files live at the workspace root and are not
//! reachable from this crate via `include_str!` relative paths.

use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Advisory-lock key guarding shared template creation.
const TEMPLATE_ADVISORY_LOCK_KEY: i64 = 0x5359_4E41_5053_5445;

/// Marker table written into the template only after a complete build.
pub const TEMPLATE_READY_TABLE: &str = "_synapse_test_template_ready";

/// FNV-1a 64-bit fingerprint of the baseline SQL, as 16 hex chars.
pub fn baseline_fingerprint(baseline_sql: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in baseline_sql.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Name of the shared template schema for the given baseline content.
pub fn template_schema_name(baseline_sql: &str) -> String {
    format!("test_isolation_template_{}", baseline_fingerprint(baseline_sql))
}

/// First non-empty line of a statement, for error context.
pub fn first_line(s: &str) -> &str {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
}

/// Removes `COPY ... FROM stdin;` ... `\.` blocks (seed data).
pub fn strip_copy_blocks(sql: &str) -> String {
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
pub fn split_sql_statements(sql: &str) -> Vec<String> {
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

// ============================================================================
// Shared baseline template
// ============================================================================
//
// Before: `IsolatedTestPool::new()` split and executed the v11 baseline (253
// CREATE TABLE + 373 CREATE INDEX + 45 ALTER TABLE + functions/views/triggers)
// as one round trip *per statement* on every call. Measured on the instrumented
// build (`[ISO_TIMING]`, 89-case cohort): `baseline_replay` median 4.416s, 99%
// of fixture setup; under `--test-threads 8` this DDL storm contends and the
// tail crosses the 30s acquire window, surfacing as `Operation timed out`.
//
// After: the baseline is applied exactly once per database into a *template*
// schema, and each test clones it with a single `DO $$` round trip.
//
// Serialization: nextest runs one process per test, so a process-local
// `OnceLock` cannot stop concurrent processes from racing to `CREATE SCHEMA`.
// A session-level advisory lock serializes the build across processes. It must
// outlive individual statements, so it is session-scoped (`pg_advisory_lock`)
// rather than transaction-scoped.

/// Ensure the shared template schema exists and is complete for `baseline_sql`.
///
/// Returns the template schema name (`template_schema_name(baseline_sql)`).
/// Safe to call concurrently from many processes: the build runs under a
/// session-scoped `pg_advisory_lock`, which is always released — including on
/// failure — because a leaked advisory lock deadlocks every later process.
///
/// The baseline SQL is passed in by the caller because the migration files live
/// at the workspace root and are not reachable from this crate via
/// `include_str!` relative paths.
pub async fn ensure_template_schema(db_url: &str, baseline_sql: &str) -> Result<String, String> {
    let template = template_schema_name(baseline_sql);
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(60))
        .connect(db_url)
        .await
        .map_err(|e| format!("failed to connect admin pool for template {template}: {e}"))?;

    // The advisory lock is session-scoped, so it must be taken and released on
    // the same connection that performs the build.
    let mut conn = admin_pool
        .acquire()
        .await
        .map_err(|e| format!("failed to acquire admin connection for template {template}: {e}"))?;

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(TEMPLATE_ADVISORY_LOCK_KEY)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("failed to take the template advisory lock: {e}"))?;

    let result = build_template(&mut conn, &template, baseline_sql).await;

    // Always release, even on failure: a leaked advisory lock deadlocks every
    // later process.
    if let Err(error) =
        sqlx::query("SELECT pg_advisory_unlock($1)").bind(TEMPLATE_ADVISORY_LOCK_KEY).execute(&mut *conn).await
    {
        tracing::error!("failed to release the template advisory lock: {error}");
    }

    result?;
    Ok(template)
}

/// Build (or reuse) the template schema on an already-locked connection.
///
/// A template carrying the readiness marker is complete and returned as-is.
/// Anything else — absent, or a previous build interrupted by timeout / SIGKILL
/// / panic — is dropped and rebuilt from scratch. Cloning from an incomplete
/// template would silently fall back to the shared `public` schema through
/// `search_path`.
async fn build_template(conn: &mut sqlx::PgConnection, template: &str, baseline_sql: &str) -> Result<(), String> {
    let ready: bool = sqlx::query_scalar("SELECT to_regclass(format('%I.%I', $1, $2)) IS NOT NULL")
        .bind(template)
        .bind(TEMPLATE_READY_TABLE)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("failed to read the readiness marker of template {template}: {e}"))?;
    if ready {
        return Ok(());
    }

    // Either absent, or a previous build was interrupted (timeout / SIGKILL /
    // panic) and left a table-less schema. Cloning from an incomplete template
    // would silently fall back to `public` via search_path, so rebuild.
    sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{}" CASCADE"#, template))
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("failed to drop the incomplete template schema {template}: {e}"))?;
    sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, template))
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("failed to create the template schema {template}: {e}"))?;
    sqlx::query(&format!(r#"SET search_path TO "{}", public"#, template))
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("failed to set search_path to the template schema {template}: {e}"))?;

    // Fail loudly: a partially applied baseline leaves the template incomplete,
    // and every clone would then silently read/write the shared `public` schema.
    let baseline_sql = strip_copy_blocks(baseline_sql);
    for stmt in split_sql_statements(&baseline_sql) {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }
        sqlx::query(trimmed).execute(&mut *conn).await.map_err(|error| {
            format!("template baseline statement failed: {error} | stmt head: {}", first_line(trimmed))
        })?;
    }

    // Readiness marker: written only after every statement succeeded, so its
    // presence is a truthful statement about completeness.
    sqlx::query(&format!(
        r#"CREATE TABLE "{}"."{TEMPLATE_READY_TABLE}" (built_at timestamptz NOT NULL DEFAULT now())"#,
        template
    ))
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("failed to write the readiness marker of template {template}: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Database URL for the isolation tests, or `None` for an *explicit* skip.
    ///
    /// A DB-backed test that silently returns when `TEST_DATABASE_URL` is unset
    /// reports `ok` in 0.00s and proves nothing — a false-green hazard in the
    /// very module this plan is hardening. So the DB is required by default:
    /// the operator must either provide `TEST_DATABASE_URL` or opt out loudly
    /// with `ALLOW_SKIP_TEST_DB=1`.
    fn test_database_url() -> Option<String> {
        match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => Some(url),
            Err(_) if std::env::var("ALLOW_SKIP_TEST_DB").ok().as_deref() == Some("1") => {
                eprintln!(
                    "SKIPPING test_isolation DB test: TEST_DATABASE_URL is unset and \
                     ALLOW_SKIP_TEST_DB=1 was explicitly set. This test proves NOTHING without a \
                     database; the run above is not green evidence."
                );
                None
            }
            Err(_) => panic!(
                "TEST_DATABASE_URL is not set. Point it at a throwaway Postgres database (for \
                 example postgresql://synapse:...@host:5432/synapse_test), or set \
                 ALLOW_SKIP_TEST_DB=1 to skip these tests explicitly."
            ),
        }
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        let a = baseline_fingerprint("CREATE TABLE users (id text);");
        let b = baseline_fingerprint("CREATE TABLE users (id text);");
        let c = baseline_fingerprint("CREATE TABLE users (id bigint);");
        assert_eq!(a, b, "same content must yield same fingerprint");
        assert_ne!(a, c, "changed content must yield a different fingerprint");
        assert_eq!(a.len(), 16, "fingerprint must be 16 hex chars");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "fingerprint must be hex, got {a}");
    }

    #[test]
    fn template_name_embeds_the_fingerprint() {
        // Fixed expectation, deliberately NOT recomputed with `baseline_fingerprint`:
        // recomputing would make the assertion near-tautological (both sides would
        // go through the function under test) and would pin nothing about the
        // naming scheme. `56b2fd4971477b87` is the FNV-1a 64 of the SQL below.
        let sql = "CREATE TABLE users (id text);";
        let name = template_schema_name(sql);
        assert_eq!(name, "test_isolation_template_56b2fd4971477b87");
    }

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
        // The users CREATE TABLE must survive the preceding `--` comment chunk.
        assert!(stmts.iter().any(|s| s.trim_start().starts_with("CREATE TABLE IF NOT EXISTS users")));
        // Function body must stay in one piece (no cut at inner `;`, no truncation).
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
        let sql = "CREATE TABLE t (id int);\nCOPY t (id) FROM stdin;\n1\n2\n\\.\nCREATE INDEX i ON t (id);\n";
        let out = strip_copy_blocks(sql);
        assert!(!out.contains("COPY"));
        assert!(!out.contains("FROM stdin"));
        assert!(!out.contains("\n1\n"));
        assert!(out.contains("CREATE INDEX i ON t (id);"));
    }

    /// Requires TEST_DATABASE_URL. Verifies the template carries the readiness
    /// marker and the baseline probe table, and that a second
    /// `ensure_template_schema` call is a genuine no-op: the schema's Postgres
    /// `oid` (and obviously its name) are unchanged, so the template was reused
    /// rather than dropped and rebuilt.
    #[tokio::test]
    async fn template_is_built_complete_and_reused() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = "CREATE TABLE IF NOT EXISTS unify_probe (id bigint PRIMARY KEY);";
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");
        let t1 = ensure_template_schema(&url, baseline).await.expect("first build");
        // `oid` is allocated per schema object and is not reused, so an unchanged
        // oid across the second call is what proves the `if ready { return Ok(()) }`
        // short-circuit was taken. Name equality alone would also hold for a
        // silent DROP + CREATE rebuild.
        let oid_after_first: i64 = sqlx::query_scalar("SELECT oid::bigint FROM pg_namespace WHERE nspname = $1")
            .bind(&t1)
            .fetch_one(&admin)
            .await
            .expect("read oid after first build");

        let t2 = ensure_template_schema(&url, baseline).await.expect("second call");
        assert_eq!(t1, t2, "same baseline content must reuse the same template");
        let oid_after_second: i64 = sqlx::query_scalar("SELECT oid::bigint FROM pg_namespace WHERE nspname = $1")
            .bind(&t1)
            .fetch_one(&admin)
            .await
            .expect("read oid after second call");
        assert_eq!(
            oid_after_first, oid_after_second,
            "template {t1} was dropped and recreated (oid {oid_after_first} -> {oid_after_second}); \
             the second call must reuse it instead of replaying the baseline"
        );

        let ready: bool = sqlx::query_scalar("SELECT to_regclass(format('%I.%I', $1, $2)) IS NOT NULL")
            .bind(&t1)
            .bind(TEMPLATE_READY_TABLE)
            .fetch_one(&admin)
            .await
            .expect("readiness query");
        assert!(ready, "template {t1} must carry the readiness marker");
        let has_table: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE schemaname = $1 AND tablename = 'unify_probe')",
        )
        .bind(&t1)
        .fetch_one(&admin)
        .await
        .expect("table query");
        assert!(has_table, "template must contain the baseline table");

        // Cleanup so repeated runs stay deterministic.
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{}" CASCADE"#, t1)).execute(&admin).await;
    }

    /// A schema that exists but has no readiness marker must be rebuilt, not
    /// reused: cloning from an incomplete template silently falls back to
    /// `public` through search_path.
    #[tokio::test]
    async fn incomplete_template_is_rebuilt() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = "CREATE TABLE IF NOT EXISTS unify_probe2 (id bigint PRIMARY KEY);";
        let template = template_schema_name(baseline);
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{template}""#)).execute(&admin).await.expect("create bare schema");

        let got = ensure_template_schema(&url, baseline).await.expect("rebuild");
        assert_eq!(got, template);
        let has_table: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE schemaname = $1 AND tablename = 'unify_probe2')",
        )
        .bind(&template)
        .fetch_one(&admin)
        .await
        .expect("table query");
        assert!(has_table, "bare schema must have been rebuilt with the baseline");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }
}
