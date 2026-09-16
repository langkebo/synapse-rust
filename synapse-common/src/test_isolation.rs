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
//! One implementation, two callers. The baseline SQL is passed in by the caller
//! as a deliberate branch constraint: **`synapse-common` must not embed
//! migration files**. The `include_str!("../../migrations/…")` path is
//! mechanically available — this crate sits at the same directory depth as
//! `synapse-storage` and `synapse-services`, both of which use it — so the reason
//! is a design rule, not a path limitation. Keeping `migrations/` out of this
//! crate's build keeps the workspace-root SQL the single source of truth.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// Advisory-lock key guarding shared template creation.
const TEMPLATE_ADVISORY_LOCK_KEY: i64 = 0x5359_4E41_5053_5445;

/// Upper bound on waiting for [`TEMPLATE_ADVISORY_LOCK_KEY`].
///
/// The build it guards is a one-time, seconds-long DDL replay. A holder that has
/// not released within this window is hung (killed mid-build, stuck statement,
/// leaked lock), and an unbounded wait would stall every later test process.
const TEMPLATE_LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

/// Interval between `pg_try_advisory_lock` polls while waiting for the lock.
const TEMPLATE_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Marker table written into the template only after a complete build.
pub const TEMPLATE_READY_TABLE: &str = "_synapse_test_template_ready";

/// The template rows the **baseline migrations** seed, and therefore the only
/// tables [`SeedSource::Only`] ever has anything to copy for.
///
/// Measured against the two inlined baseline files: `00000000_unified_schema_v12.sql`
/// has exactly three `INSERT INTO` statements (lines 4554, 4563, 4568) and
/// `00000001_extensions_v10.sql` has none. The template's other 250-odd tables
/// are empty, so this allowlist and [`SeedSource::Everything`] currently produce
/// row-identical clones.
///
/// That equality is not self-maintaining: it is pinned by
/// `seed_reference_tables_match_baseline`, which parses the inlined migrations
/// and goes red the moment a future migration seeds a fourth table. Without that
/// guard, a new seed would silently appear in `Everything` clones and silently
/// be missing from `Only` clones — a fork with no compiler or test signal.
///
/// Note the `users` table is deliberately absent, and the reason is no longer
/// "the `@admin:localhost` seed is skipped": that hardcoded `INSERT` was removed
/// from the v11 baseline (DB-04) and only an `UPDATE ... WHERE username = 'admin'`
/// over zero rows remains. A freshly migrated database has no users row.
pub const SEED_REFERENCE_TABLES: &[&str] = &["server_media_quota", "server_retention_policy", "sync_stream_id"];

/// Which template rows a clone starts with.
///
/// Both variants read the same template schema; they differ only in which tables
/// phase 1b of [`clone_statement`] copies. They are deliberately distinct types
/// rather than a bare `&[&str]`: `Everything` copies whatever the template
/// happens to hold, while `Only` is a written-down contract that fails the clone
/// loudly if a named table has vanished from the template — a silently missing
/// config row surfaces much later as an unrelated test failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedSource<'a> {
    /// Phase 1b copies every baseline table. This is what every production call
    /// site uses, because none of them depends on a subset.
    Everything,
    /// Phase 1b copies only these tables. Any name absent from the template
    /// aborts the clone with `42P01` from the `INSERT` (fail loudly, never
    /// skip). An empty slice is legal and yields a structure-only clone.
    Only(&'a [&'a str]),
}

/// Build the phase 1b `WHERE` clause selecting which template tables phase 1b
/// copies the rows of.
///
/// Names are interpolated into a dollar-quoted `DO` block, so they are validated
/// as bare unquoted identifiers first. They all come from a `const &[&str]`, but
/// the check is what keeps a future caller from turning this into an injection
/// point. The readiness marker is always excluded: it exists only in the
/// template, so naming it would make the `INSERT` fail with `42P01`.
fn seed_where_clause(seeds: SeedSource<'_>) -> Result<String, String> {
    let tail = format!("tablename <> '{TEMPLATE_READY_TABLE}'");
    Ok(match seeds {
        SeedSource::Everything => tail,
        SeedSource::Only(names) => {
            for name in names {
                let ok =
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
                if !ok {
                    return Err(format!("refusing to build a seed list from the non-identifier table name {name:?}"));
                }
            }
            if names.is_empty() {
                // A structure-only clone: no table matches, so phase 1b copies
                // nothing while phase 1 has already created every table.
                "1 = 0".to_string()
            } else {
                let list = names.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
                format!("tablename IN ({list}) AND {tail}")
            }
        }
    })
}

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
/// The baseline SQL is passed in by the caller as a deliberate branch
/// constraint: `synapse-common` must not embed migration files, even though
/// `include_str!("../../migrations/…")` is mechanically available from this
/// crate's directory depth (see the module docs).
pub async fn ensure_template_schema(db_url: &str, baseline_sql: &str) -> Result<String, String> {
    ensure_template_schema_with_lock_timeout(db_url, baseline_sql, TEMPLATE_LOCK_WAIT_TIMEOUT).await
}

/// [`ensure_template_schema`], with the advisory-lock wait bound injected.
///
/// The timeout is a parameter (rather than an environment read) so the
/// timeout path is unit-testable without holding the shared lock for the full
/// 120s default. Every production caller uses [`ensure_template_schema`].
async fn ensure_template_schema_with_lock_timeout(
    db_url: &str,
    baseline_sql: &str,
    lock_wait: Duration,
) -> Result<String, String> {
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

    acquire_template_lock(&mut conn, lock_wait).await?;

    let result = build_template(&mut conn, &template, baseline_sql).await;

    // Housekeeping (§15.2): drop superseded `test_isolation_template_*`
    // fingerprints while still holding the advisory lock, and only after the
    // current template is verified present (the audit's "confirm the
    // replacement exists before deleting" safety order). Best-effort: a
    // working template must never fail because cleanup stumbled.
    if result.is_ok() {
        match prune_isolation_templates(&mut conn, &template).await {
            Ok(dropped) if !dropped.is_empty() => {
                tracing::info!(count = dropped.len(), keep = %template, "pruned superseded test isolation template schemas");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "failed to prune superseded test isolation template schemas");
            }
        }
    }

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

/// Take the session-scoped template advisory lock, bounded by `lock_wait`.
///
/// `pg_advisory_lock` blocks forever. A holder that never releases — a process
/// killed mid-build, a stuck statement, a leaked lock — therefore stalls every
/// later test process, and the pool's `acquire_timeout` does not cover a lock
/// wait inside an already-acquired connection. Polling `pg_try_advisory_lock`
/// against a deadline bounds the wait and surfaces a descriptive error naming
/// the key and the elapsed wait. It also leaves the connection's own settings
/// untouched, unlike `SET lock_timeout`/`statement_timeout`, which would leak
/// into the baseline DDL executed after the lock is taken.
async fn acquire_template_lock(conn: &mut sqlx::PgConnection, lock_wait: Duration) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + lock_wait;
    loop {
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(TEMPLATE_ADVISORY_LOCK_KEY)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| format!("failed to try the template advisory lock: {e}"))?;
        if acquired {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "timed out after {lock_wait:?} waiting for the template advisory lock \
                 (key {:#x}); another test process is building the shared template and has not \
                 released it. Waiting forever would block every later process, so the wait is \
                 bounded and reported instead.",
                TEMPLATE_ADVISORY_LOCK_KEY
            ));
        }
        tokio::time::sleep(TEMPLATE_LOCK_POLL_INTERVAL).await;
    }
}

/// Grace window protecting a template that other in-flight test processes may
/// still be cloning from. A candidate is dropped only when its readiness
/// marker is older than this window (see [`prune_isolation_templates`]).
const TEMPLATE_PRUNE_GRACE: Duration = Duration::from_secs(6 * 60 * 60);

/// Drop superseded `test_isolation_template_<fingerprint>` schemas.
///
/// ## Why this exists (§15.2)
///
/// [`template_schema_name`] is an FNV-1a hash of the baseline SQL, so **every
/// baseline edit mints a new template and orphans the old one** — and the
/// shared module's own lib tests mint a fresh fingerprint per distinct
/// `let baseline = ...` string (13 of them). Before this function nothing ever
/// deleted those orphans: on a long-lived dev database each full template is
/// 254 tables / ~1,197 objects, and the audit measured 2,457 leaked objects
/// across 7 templates. The root fixture has always pruned its own
/// `test_template_v<rev>_<hex>` family (`src/test_utils.rs::prune_stale_template_schemas`);
/// this closes the same gap for the shared module's family.
///
/// ## Why it is not a plain "delete everything except keep"
///
/// The root function can prune every non-`keep` match because the root
/// template name is derived from a single migration fingerprint shared by one
/// crate. This family is **cross-crate and test-local**: `synapse-storage` and
/// `synapse-services` share the real baseline fingerprint, while lib tests mint
/// their own. Two nextest processes running concurrently legitimately use
/// different fingerprints — `DROP ... CASCADE` on a template another session
/// is mid-`clone_schema_from_template` from would fail that unrelated test.
///
/// So the rule is age-based rather than "!= keep": drop a candidate only once
/// its readiness marker has gone stale beyond [`TEMPLATE_PRUNE_GRACE`].
/// [`build_template`] refreshes that marker on every path — including the
/// already-ready fast path — so "recently used" is measured by *last use*, not
/// build time, and an actively-cloned template keeps itself alive. A template
/// left incomplete by a crashed build has no marker at all and is dropped
/// immediately: holding the advisory lock means no other builder is mid-build,
/// and a clone against an incomplete template is already unusable. A marker
/// table with zero rows (a template built before the marker-row change) is
/// spared too — it is treated as legacy-and-possibly-in-use until its next
/// `ensure_template_schema` backfills a row.
///
/// The anchor `^test_isolation_template_[0-9a-f]{16}$` cannot match the
/// root crate's `test_template_v<rev>_<hex>`, the services'
/// `test_template_<pid>`, per-test clones (`test_<uuid>`,
/// `tstest_*`), or the `test_*` residue from §9.
///
/// [`keep`] is verified present before anything is dropped, so a failure in
/// the current build can never leave the database template-less. Returns the
/// names dropped.
pub async fn prune_stale_isolation_templates(admin_pool: &PgPool, keep: &str) -> Result<Vec<String>, String> {
    let mut conn = admin_pool
        .acquire()
        .await
        .map_err(|error| format!("failed to acquire connection to prune isolation templates: {error}"))?;
    prune_isolation_templates(&mut conn, keep).await
}

/// [`prune_stale_isolation_templates`] over a borrowable connection, so the
/// caller can run it while still holding the advisory lock.
async fn prune_isolation_templates(conn: &mut sqlx::PgConnection, keep: &str) -> Result<Vec<String>, String> {
    // Safety order (same as the root fixture): never prune before the
    // replacement template is confirmed present.
    let keep_exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
        .bind(keep)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| format!("failed to check template {keep}: {error}"))?;
    if !keep_exists {
        return Err(format!("refusing to prune isolation templates: current template {keep} does not exist"));
    }

    // List all `test_isolation_template_<16hex>` candidates except `keep`.
    let candidates: Vec<String> = sqlx::query_scalar(
        "SELECT nspname FROM pg_namespace
         WHERE nspname ~ '^test_isolation_template_[0-9a-f]{16}$'
           AND nspname <> $1
         ORDER BY nspname",
    )
    .bind(keep)
    .fetch_all(&mut *conn)
    .await
    .map_err(|error| format!("failed to list stale isolation templates: {error}"))?;

    let mut dropped = Vec::new();
    for schema in candidates {
        // Query the marker's max built_at for this schema.
        let full_table = format!("{schema}.{TEMPLATE_READY_TABLE}");
        let table_exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(&full_table)
            .fetch_one(&mut *conn)
            .await
            .map_err(|error| format!("failed to check readiness marker for {schema}: {error}"))?;

        let eligible_to_drop = if !table_exists {
            // No marker = incomplete build, safe to drop while holding the lock
            true
        } else {
            // Query the marker's row count and age. A legacy template (created
            // before this code) has an empty marker table; treat it as "used"
            // until it gets backfilled by the next build (which does DELETE+INSERT).
            let row_count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {full_table}"))
                .fetch_one(&mut *conn)
                .await
                .map_err(|error| format!("failed to count marker rows for {schema}: {error}"))?;

            if row_count == 0 {
                // Legacy template: marker table exists but has no rows, so its
                // age is unknowable — and it may be mid-clone by a pre-upgrade
                // process. Backfill a fresh timestamp (starting its age clock)
                // and spare it this pass; if it is a dead orphan it ages out
                // after the grace window, and if it is in use it keeps getting
                // refreshed by its own ensure calls.
                if let Err(error) =
                    sqlx::query(&format!("INSERT INTO {full_table} DEFAULT VALUES")).execute(&mut *conn).await
                {
                    tracing::warn!(schema = %schema, %error, "failed to backfill a legacy template readiness marker");
                }
                false
            } else {
                // Fresh marker with at least one row — check age
                let stale_secs: Option<i64> = sqlx::query_scalar(&format!(
                    "SELECT EXTRACT(EPOCH FROM age(now(), max(built_at)))::BIGINT FROM {full_table}"
                ))
                .fetch_one(&mut *conn)
                .await
                .map_err(|error| format!("failed to read marker age for {schema}: {error}"))?;

                stale_secs.map(|secs| secs > TEMPLATE_PRUNE_GRACE.as_secs() as i64).unwrap_or(false)
            }
        };

        if !eligible_to_drop {
            continue;
        }

        match sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&mut *conn).await {
            Ok(_) => dropped.push(schema),
            Err(error) => {
                tracing::warn!(schema = %schema, %error, "failed to drop stale test isolation template schema");
            }
        }
    }
    Ok(dropped)
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
        // Refresh the marker so an actively-used template stays inside the
        // prune age gate — `built_at` means "last time this fingerprint was
        // requested", not original build time (§15.2). DELETE + INSERT keeps
        // exactly one row at now() and also **backfills** a legacy template
        // whose marker table predates the row-seed change (it has a table but
        // zero rows, so `max(built_at)` would otherwise read NULL). Best-effort:
        // a failed refresh only matters once the multi-hour grace window has
        // passed, and must never fail an otherwise healthy template.
        let marker = format!("\"{template}\".\"{TEMPLATE_READY_TABLE}\"");
        if let Err(error) = sqlx::query(&format!("DELETE FROM {marker}")).execute(&mut *conn).await {
            tracing::warn!(%error, template = %template, "failed to delete rows from the template readiness marker table");
        }
        if let Err(error) = sqlx::query(&format!("INSERT INTO {marker} DEFAULT VALUES")).execute(&mut *conn).await {
            tracing::warn!(%error, template = %template, "failed to insert a row into the template readiness marker table");
        }
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

    // Seed the marker with a row so `max(built_at)` is non-NULL.  Without this
    // row the readiness table is empty, `max` is NULL, and the age-based
    // prune would treat the fresh template as stale and immediately drop it.
    sqlx::query(&format!(r#"INSERT INTO "{}"."{TEMPLATE_READY_TABLE}" DEFAULT VALUES"#, template))
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("failed to seed the readiness marker of template {template}: {e}"))?;

    Ok(())
}

/// Build the single-round-trip clone statement for `schema` from `template`.
///
/// Four steps, deliberately:
///
/// * Phase 1 (`search_path` unchanged): `CREATE TABLE ... (LIKE ... INCLUDING
///   ALL)`. This carries columns, defaults, generated expressions, identity,
///   indexes and PRIMARY KEY / UNIQUE / CHECK constraints. It does **not**
///   carry FOREIGN KEYs (measured: 0/127 survived the copy), so those are
///   replayed explicitly in phase 2. It does **not** carry row data either, so
///   phase 1b copies the rows.
///
/// * Phase 1b (`search_path` unchanged, fully-qualified): `INSERT INTO
///   <clone>.<t> SELECT * FROM <template>.<t>` for every baseline table.
///   `LIKE` copies structure only, which silently dropped the v11 baseline's
///   singleton seeds (`sync_stream_id`, `server_retention_policy`,
///   `server_media_quota`) from every clone. This runs after phase 1 (the
///   tables must exist) and before the materialized views of phase 2, because
///   a matview is populated at creation time and would otherwise be stale at 0
///   rows. It also runs before the FOREIGN KEYs are replayed, so the arbitrary
///   `ORDER BY tablename` copy order cannot trip a not-yet-satisfied FK.
///
/// * Phase 1c (fully-qualified): create a clone-owned copy of every template
///   sequence and rebind each serial column's default to it, then advance each
///   clone sequence past the rows copied in phase 1b. `LIKE ... INCLUDING ALL`
///   copies a serial column's DEFAULT *expression*, which still names the
///   **template's** sequence, and creates no sequence in the clone — so without
///   this phase every clone drew ids from one shared template sequence and owned
///   none of its own. That is a real isolation regression: the pre-unification
///   fixture gave each schema fresh sequences, and a copied row `id = 1` plus a
///   template sequence still at `last_value = 1, is_called = false` made an
///   `INSERT` that omits `id` fail with `duplicate key value violates unique
///   constraint`.
///
/// * Phase 2 (`search_path` = clone, then the caller's remaining entries):
///   replay functions, views, materialized views, foreign keys and triggers,
///   which `LIKE` cannot copy. The `search_path` matters for *correctness*
///   because views, materialized views and FK constraint definitions are
///   **parsed and OID-bound at creation time**: their definitions are replayed
///   with the template qualifier stripped, so `search_path` must already name
///   the clone or the unqualified references bind to the template (or, when the
///   template lacks them, to `public`). PL/pgSQL bodies are the opposite case —
///   they resolve unqualified names at **execution** time through the calling
///   session's `search_path` — and every caller's session path starts with the
///   clone, so the clone's functions read and write the clone's tables. (The
///   baseline's six functions are all PL/pgSQL; a `LANGUAGE sql` body *is* parsed
///   at creation time and would depend on this switch as well.) The caller's tail
///   (everything after the clone) is preserved rather than replaced with a
///   literal `public`, so a caller path such as `<clone>, public, extensions`
///   keeps its `extensions` entry.
///
/// This statement deliberately does **not** create `schema`: the caller
/// guarantees it already exists (and that its session `search_path` already
/// begins with it). A `CREATE SCHEMA` here would fail with `42P06
/// duplicate_schema` for every caller. A caller that never set a path (fresh
/// session, `"$user", public`) still works: phase 2 explicitly puts the clone
/// first and appends the effective tail.
///
/// The template's own bookkeeping table ([`TEMPLATE_READY_TABLE`]) is skipped:
/// it is fixture metadata, not baseline inventory, and a clone is expected to
/// reproduce exactly the baseline objects (so `clone_matches_template_inventory`
/// sees 2 tables for a 2-table baseline). [`validate_clone`] excludes the same
/// table from both sides of its comparison.
fn clone_statement(schema: &str, template: &str, seeds: SeedSource<'_>) -> Result<String, String> {
    let seed_where = seed_where_clause(seeds)?;
    Ok(format!(
        r#"
        DO $do$
        DECLARE
            r RECORD;
            def TEXT;
            rest TEXT;
            seq_q TEXT;
            max_id BIGINT;
        BEGIN
            -- Phase 1: every baseline table, with indexes / defaults / CHECK / PK.
            -- The readiness marker is template bookkeeping, not baseline content.
            FOR r IN
                SELECT tablename FROM pg_tables
                WHERE schemaname = '{template}' AND tablename <> '{TEMPLATE_READY_TABLE}'
                ORDER BY tablename
            LOOP
                EXECUTE format(
                    'CREATE TABLE %I.%I (LIKE %I.%I INCLUDING ALL)',
                    '{schema}', r.tablename, '{template}', r.tablename
                );
            END LOOP;

            -- Phase 1b: copy the template's row data. `LIKE ... INCLUDING ALL`
            -- copies structure only, so the singleton rows the v11 baseline
            -- seeds (`sync_stream_id`, `server_retention_policy`,
            -- `server_media_quota`) were silently missing from every clone even
            -- though the previous statement-by-statement fixture had them.
            -- Positional `SELECT *` matches because `LIKE` preserves column
            -- order. The copy runs HERE, before the materialized views in phase
            -- 2: a matview is populated at creation time, so creating it over an
            -- empty table and filling the base table afterwards would leave it
            -- permanently stale at 0 rows. It also runs before the foreign keys
            -- are replayed, so the arbitrary `ORDER BY tablename` order cannot
            -- trip a not-yet-satisfied FK. The readiness marker is excluded for
            -- the same reason as phase 1 (and it must be, or the `INSERT` would
            -- fail with `42P01`: the clone has no such table).
            --
            -- Which tables are copied is the caller's choice:
            -- `SeedSource::Everything` (the historical behaviour, and what every
            -- production call site passes) copies all of them, while
            -- `SeedSource::Only` restricts the set to the baseline's seeded
            -- reference tables. The clause below is built by
            -- `seed_where_clause`, never by a caller-supplied string.
            FOR r IN
                SELECT tablename FROM pg_tables
                WHERE schemaname = '{template}' AND {seed_where}
                ORDER BY tablename
            LOOP
                EXECUTE format(
                    'INSERT INTO %I.%I SELECT * FROM %I.%I',
                    '{schema}', r.tablename, '{template}', r.tablename
                );
            END LOOP;

            -- Phase 1c: clone-owned sequences. `LIKE ... INCLUDING ALL` copies a
            -- serial column's DEFAULT *expression* — still
            -- `nextval('<template>.<seq>'::regclass)` — but creates no sequence
            -- in the clone. Every clone therefore drew ids from ONE shared
            -- template sequence and owned ZERO sequences of its own. Copying a
            -- row with an explicit `id = 1` leaves the template sequence at
            -- `last_value = 1, is_called = false`, so an `INSERT` that omits
            -- `id` on the clone fails with a duplicate-key error. The
            -- pre-unification fixture gave every schema fresh sequences, so this
            -- is a regression in isolation semantics, not just a latent hazard.
            --
            -- Create every template sequence in the clone — including the two
            -- the baseline never binds to a column (`to_device_stream_id_seq`,
            -- `sliding_sync_pos_seq`), which an unqualified `nextval` would
            -- otherwise fail to find — copying its data type. Then rebind each
            -- serial default to the clone's own sequence and advance it past the
            -- rows phase 1b copied, so a later default-id insert cannot collide.
            -- The sequence is derived from the catalog (`pg_attrdef` -> the
            -- `pg_depend` edge to a `relkind = 'S'` relation), never by slicing
            -- the default expression text.
            FOR r IN
                SELECT c.relname AS seq_rel,
                       COALESCE(sq.data_type, 'bigint') AS seq_type
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                LEFT JOIN pg_sequences sq
                       ON sq.schemaname = n.nspname AND sq.sequencename = c.relname
                WHERE n.nspname = '{template}' AND c.relkind = 'S'
                ORDER BY c.relname
            LOOP
                EXECUTE format(
                    'CREATE SEQUENCE IF NOT EXISTS %I.%I AS %s',
                    '{schema}', r.seq_rel, r.seq_type
                );
            END LOOP;

            FOR r IN
                SELECT t.relname AS tbl,
                       a.attname AS col,
                       s.relname AS seq_rel
                FROM pg_attrdef ad
                JOIN pg_class t ON t.oid = ad.adrelid
                JOIN pg_namespace tn ON tn.oid = t.relnamespace
                JOIN pg_attribute a ON a.attrelid = ad.adrelid AND a.attnum = ad.adnum
                JOIN pg_depend d ON d.classid = 'pg_attrdef'::regclass AND d.objid = ad.oid
                JOIN pg_class s ON s.oid = d.refobjid AND s.relkind = 'S'
                WHERE tn.nspname = '{template}' AND a.attnum > 0 AND NOT a.attisdropped
                ORDER BY t.relname, a.attname
            LOOP
                seq_q := format('%I.%I', '{schema}', r.seq_rel);
                -- `OWNED BY` matters and is easy to leave out: a plain
                -- `CREATE SEQUENCE` plus `SET DEFAULT nextval(...)` leaves the
                -- sequence with no ownership edge, so `pg_get_serial_sequence`
                -- returns NULL for the column and `TRUNCATE ... RESTART IDENTITY`
                -- **silently does not reset it** (measured: an unowned sequence
                -- stayed at 42 while an owned one went to 1). Fixtures that reuse
                -- a schema depend on that reset to return an emptied schema to a
                -- known state, so the clone's sequence must be owned by the
                -- clone's column exactly like the template's `bigserial` is.
                -- `OWNED BY` also makes PostgreSQL drop the sequence with its
                -- column, which is what keeps a dropped clone schema from
                -- leaking sequences.
                EXECUTE format(
                    'ALTER SEQUENCE %I.%I OWNED BY %I.%I.%I',
                    '{schema}', r.seq_rel, '{schema}', r.tbl, r.col
                );
                EXECUTE format(
                    'ALTER TABLE %I.%I ALTER COLUMN %I SET DEFAULT nextval(%L::regclass)',
                    '{schema}', r.tbl, r.col, seq_q
                );
                -- `pg_sequence_last_value` is the clone sequence's own position
                -- (NULL until first called), so this never lowers a sequence a
                -- previous iteration already advanced, even if two columns share
                -- one sequence. An empty clone table leaves the fresh sequence
                -- at its start, so its first `nextval` is 1 with no row to
                -- collide with.
                EXECUTE format(
                    'SELECT GREATEST(COALESCE(max(%I), 0), COALESCE(pg_sequence_last_value(%L::regclass), 0)) FROM %I.%I',
                    r.col, seq_q, '{schema}', r.tbl
                ) INTO max_id;
                IF max_id > 0 THEN
                    EXECUTE format('SELECT setval(%L::regclass, %s, true)', seq_q, max_id);
                END IF;
            END LOOP;

            -- Phase 1d: restore index and UNIQUE-constraint NAMES.
            --
            -- `LIKE ... INCLUDING ALL` copies indexes but PostgreSQL assigns
            -- auto-generated names (a template's `idx_t_v_named` becomes
            -- `t_v_idx`), and it renames UNIQUE constraints
            -- (`uq_c_pid_named` -> `c_pid_key`). PRIMARY KEY names survive.
            -- Tests assert on those names (`has_index_named` has 24 call sites
            -- in tests/integration/schema_contract_p0_tests_migrated.rs), and
            -- `validate_clone` compares only COUNTS, so a rename is invisible
            -- to it. Measured on a two-table probe: template
            -- `idx_t_v_named,uq_c_pid_named` -> clone `t_v_idx,c_pid_key`.
            --
            -- An index that backs a constraint cannot be dropped or renamed
            -- independently of it, so those are renamed in place; plain indexes
            -- are dropped and rebuilt from the template's own definition (which
            -- preserves expression / partial / opclass details that
            -- reconstructing the DDL by hand would lose).
            FOR r IN
                -- `FOR r IN` requires a SELECT: a top-level WITH is a syntax
                -- error in PL/pgSQL, so the CTEs are wrapped in a subquery.
                SELECT * FROM (
                -- Pair by ORDINALITY within the table: `LIKE` copies the
                -- template's indexes in that order, so `row_number()` over the
                -- same ordering lines them up. Pairing on names alone cannot
                -- work (the clone's names are auto-generated, which is the very
                -- thing being fixed). Verified: the paired definitions match
                -- modulo the name (`users_email_idx` <-> `idx_users_email`, both
                -- `USING btree (email)`).
                WITH clone_idx AS (
                    SELECT ci.indexrelid,
                           ct.relname AS tbl_name,
                           cidx.relname AS idx_name,
                           row_number() OVER (PARTITION BY ct.relname ORDER BY cidx.relname) AS rn
                    FROM pg_index ci
                    JOIN pg_class cidx ON cidx.oid = ci.indexrelid
                    JOIN pg_class ct ON ct.oid = ci.indrelid
                    JOIN pg_namespace cn ON cn.oid = ct.relnamespace
                    WHERE cn.nspname = '{schema}'
                      AND NOT EXISTS (
                          SELECT 1 FROM pg_constraint cc
                          WHERE cc.conindid = ci.indexrelid
                      )
                ),
                tmpl_idx AS (
                    SELECT ti.indexrelid,
                           tt.relname AS tbl_name,
                           tidx.relname AS tmpl_idx_name,
                           ti.indisunique AS is_unique,
                           pg_get_indexdef(tidx.oid) AS idx_def,
                           -- Everything after `ON <schema>.<table>` — i.e. the
                           -- USING clause and any WHERE/INCLUDE tail.
                           format('%I.%I', tn.nspname, tt.relname) AS tmpl_tbl,
                           row_number() OVER (PARTITION BY tt.relname ORDER BY tidx.relname) AS rn
                    FROM pg_index ti
                    JOIN pg_class tidx ON tidx.oid = ti.indexrelid
                    JOIN pg_class tt ON tt.oid = ti.indrelid
                    JOIN pg_namespace tn ON tn.oid = tt.relnamespace
                    WHERE tn.nspname = '{template}'
                      -- Constraint-backed indexes (PK/UNIQUE) are handled by
                      -- the constraints themselves: `LIKE` already preserves
                      -- the PRIMARY KEY name, and renaming a constraint's
                      -- index is a needless risk. Only plain indexes are
                      -- normalised here.
                      AND NOT EXISTS (
                          SELECT 1 FROM pg_constraint cc
                          WHERE cc.conindid = ti.indexrelid
                      )
                )
                SELECT c.tbl_name,
                       c.idx_name,
                       t.tmpl_idx_name,
                       t.is_unique,
                       -- Drop the leading `CREATE [UNIQUE] INDEX <name> ON <tbl>`
                       -- so the tail can be re-prefixed with the canonical name;
                       -- `pg_get_indexdef` already returns a complete statement,
                       -- so appending to it would produce `... ON tbl CREATE INDEX`.
                       split_part(t.idx_def, t.tmpl_tbl, 2) AS idx_tail
                FROM clone_idx c
                JOIN tmpl_idx t ON t.tbl_name = c.tbl_name AND t.rn = c.rn
                WHERE c.idx_name <> t.tmpl_idx_name
                ) AS paired
            LOOP
                EXECUTE format('DROP INDEX %I.%I', '{schema}', r.idx_name);
                EXECUTE format(
                    'CREATE %s INDEX %I ON %I.%I %s',
                    CASE WHEN r.is_unique THEN 'UNIQUE' ELSE '' END,
                    r.tmpl_idx_name, '{schema}', r.tbl_name, r.idx_tail
                );
            END LOOP;

            -- Phase 2: non-table objects must bind to the clone, not the template.
            -- Rebuild the path as the clone followed by the caller's remaining
            -- entries. The documented precondition only guarantees the caller's
            -- path *begins* with the clone, so hard-coding `public` here would
            -- silently drop e.g. an `extensions` entry. `current_schemas(false)`
            -- yields the effective path (existing schemas only) and, with the
            -- clone filtered out, leaves the caller's tail intact. This also
            -- covers callers that never set a path (fresh session: `"$user",
            -- public`), which still end up with `<clone>, public`.
            SELECT string_agg(quote_ident(s), ', ') INTO rest
            FROM unnest(current_schemas(false)) AS s
            WHERE s <> '{schema}';
            IF rest IS NULL THEN
                EXECUTE format('SET search_path TO %I', '{schema}');
            ELSE
                EXECUTE format('SET search_path TO %I, %s', '{schema}', rest);
            END IF;

            -- Functions. `pg_get_functiondef` renders the name template-qualified;
            -- strip the qualifier so it is created inside the clone.
            FOR r IN
                SELECT p.proname AS name,
                       pg_get_function_identity_arguments(p.oid) AS args,
                       pg_get_functiondef(p.oid) AS def
                FROM pg_proc p
                JOIN pg_namespace n ON n.oid = p.pronamespace
                WHERE n.nspname = '{template}' AND p.prokind = 'f'
            LOOP
                def := replace(r.def, '{template}.', '');
                def := replace(def, '"{template}".', '');
                EXECUTE def;
            END LOOP;

            -- Views / materialized views. `depth` counts an object's transitive
            -- dependencies (the recursion walks *below* an object), so the
            -- deepest objects are the leaves and must be created first:
            -- descending depth. Ascending created a view before the view or
            -- materialized view it reads, which bound the stripped,
            -- unqualified reference to `public` (or failed with `relation ...
            -- does not exist` when `public` lacked it). Measured on the real
            -- v11 template: `public_room_directory` (depth 0) reads
            -- `rooms_summaries_mv` (depth 1), and ascending left all 13 of its
            -- references pointing at `public.rooms_summaries_mv`.
            FOR r IN
                WITH RECURSIVE deps AS (
                    SELECT c.oid, 0 AS depth
                    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
                    WHERE n.nspname = '{template}' AND c.relkind IN ('v','m')
                    UNION ALL
                    SELECT d.refobjid, deps.depth + 1
                    FROM deps
                    JOIN pg_rewrite w ON w.ev_class = deps.oid
                    JOIN pg_depend d ON d.objid = w.oid AND d.refobjid <> deps.oid
                    JOIN pg_class rc ON rc.oid = d.refobjid
                    WHERE rc.relkind IN ('v','m')
                )
                SELECT c.relname AS name,
                       c.relkind AS kind,
                       pg_get_viewdef(c.oid) AS def,
                       max(deps.depth) AS depth
                FROM deps
                JOIN pg_class c ON c.oid = deps.oid
                GROUP BY c.relname, c.relkind, c.oid
                ORDER BY max(deps.depth) DESC, c.relname
            LOOP
                -- `pg_get_viewdef` renders referenced tables template-qualified
                -- (`FROM test_isolation_template_x.workers`). Left as-is the clone's
                -- views would read the *template's* rows, so strip the qualifier
                -- and let `search_path` (now the clone) resolve them. Without the
                -- strip the DDL is also rejected: `42601 syntax error at end of input`.
                def := replace(r.def, '{template}.', '');
                def := replace(def, '"{template}".', '');
                IF r.kind = 'm' THEN
                    EXECUTE format('CREATE MATERIALIZED VIEW %I.%I AS %s', '{schema}', r.name, def);
                ELSE
                    EXECUTE format('CREATE VIEW %I.%I AS %s', '{schema}', r.name, def);
                END IF;
            END LOOP;

            -- Foreign keys. `LIKE ... INCLUDING ALL` copies PRIMARY KEY /
            -- UNIQUE / CHECK but NOT FOREIGN KEYs (measured: 0/127 carried
            -- over), so replay them. The existence test is computed in the
            -- query and returned as a column, so the `IF` has one simple
            -- condition; a duplicate means the FK is already present, which is
            -- the desired end state.
            FOR r IN
                SELECT fkrel.relname AS tbl,
                       con.conname AS name,
                       replace(pg_get_constraintdef(con.oid), format('%I.', tn.nspname), '') AS def,
                       EXISTS (
                           SELECT 1
                           FROM pg_constraint cc
                           JOIN pg_namespace cn ON cn.oid = cc.connamespace
                           JOIN pg_class crel ON crel.oid = cc.conrelid
                           WHERE cc.contype = 'f'
                             AND cc.conname = con.conname
                             AND cn.nspname = '{schema}'
                             AND crel.relname = fkrel.relname
                       ) AS already_cloned
                FROM pg_constraint con
                JOIN pg_namespace tn ON tn.oid = con.connamespace
                JOIN pg_class fkrel ON fkrel.oid = con.conrelid
                WHERE con.contype = 'f' AND tn.nspname = '{template}'
            LOOP
                IF NOT r.already_cloned THEN
                    EXECUTE format('ALTER TABLE %I.%I ADD CONSTRAINT %I %s',
                                   '{schema}', r.tbl, r.name, r.def);
                END IF;
            END LOOP;

            -- Triggers. Both the `ON` table and the executed function must be
            -- re-pointed at the clone: `pg_get_triggerdef` renders the function
            -- template-qualified (`EXECUTE FUNCTION {template}.f()`), and
            -- leaving that in place gives every clone a real dependency on the
            -- template schema. `ensure_template_schema` drops an incomplete
            -- template with `CASCADE`, which would then cascade into every
            -- clone's triggers. Runtime row routing kept working anyway because
            -- the PL/pgSQL body resolves unqualified names via the session
            -- `search_path`, which is why the dependency was easy to miss.
            FOR r IN
                SELECT c.relname AS tbl, t.tgname AS name, pg_get_triggerdef(t.oid) AS def
                FROM pg_trigger t
                JOIN pg_class c ON c.oid = t.tgrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = '{template}' AND NOT t.tgisinternal
            LOOP
                def := replace(r.def, ' ON {template}.', ' ON {schema}.');
                def := replace(def, ' ON "{template}".', ' ON "{schema}".');
                def := replace(def, 'EXECUTE FUNCTION {template}.', 'EXECUTE FUNCTION {schema}.');
                def := replace(def, 'EXECUTE FUNCTION "{template}".', 'EXECUTE FUNCTION "{schema}".');
                EXECUTE def;
            END LOOP;
        END
        $do$;
        "#
    ))
}

/// Verify the clone reproduces the template's object inventory.
///
/// A clone that silently lacks tables/functions/views makes every query for the
/// missing objects resolve against the shared `public` schema via the
/// `search_path`. That is the failure mode behind the order-dependent
/// `media::tests` and `*::db_tests` breakage, so it must be an immediate error.
/// The comparison also covers **sequences**, because a clone that owns none
/// while its serial defaults still point at the template's sequences has given
/// up clone-local id allocation — the failure mode behind the duplicate-key
/// error on a default-id insert (see [`clone_statement`] phase 1c).
///
/// The template-only bookkeeping table ([`TEMPLATE_READY_TABLE`]) is excluded
/// from the table count on both sides, matching [`clone_statement`], which does
/// not copy it.
async fn validate_clone(pool: &PgPool, schema: &str, template: &str) -> Result<(), String> {
    // One query against a fixed set of relations, aggregating per schema.
    /// Object inventory for one schema. Named fields rather than an 8-tuple:
    /// positional access to eight `i64`s is exactly the kind of thing that
    /// silently swaps two counts.
    #[derive(sqlx::FromRow)]
    struct Inventory {
        nsp: String,
        tbls: i64,
        fks: i64,
        funcs: i64,
        views: i64,
        mviews: i64,
        triggers: i64,
        seqs: i64,
    }

    let inventory: Vec<Inventory> = sqlx::query_as(
        r#"
        WITH target AS (
            -- Catalog-sourced, deliberately: `SELECT unnest(ARRAY[$1, $2])`
            -- fabricated a row for every name whether or not the schema
            -- existed, which made the `find` guards below dead code and turned
            -- a missing template into a vacuous 0/0 `Ok`.
            SELECT nspname AS nsp FROM pg_namespace WHERE nspname = ANY(ARRAY[$1, $2]::text[])
        )
        SELECT t.nsp,
            (SELECT count(*) FROM pg_tables tb
              WHERE tb.schemaname = t.nsp AND tb.tablename <> $3) AS tbls,
            (SELECT count(*) FROM pg_constraint c
               JOIN pg_class r ON r.oid = c.conrelid
               JOIN pg_namespace n ON n.oid = r.relnamespace
              WHERE n.nspname = t.nsp AND c.contype = 'f') AS fks,
            (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
              WHERE n.nspname = t.nsp AND p.prokind = 'f') AS funcs,
            (SELECT count(*) FROM pg_views v WHERE v.schemaname = t.nsp) AS views,
            (SELECT count(*) FROM pg_matviews m WHERE m.schemaname = t.nsp) AS mviews,
            (SELECT count(*) FROM pg_trigger tr
               JOIN pg_class c ON c.oid = tr.tgrelid
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = t.nsp AND NOT tr.tgisinternal) AS triggers,
            -- Sequences are counted too: `LIKE ... INCLUDING ALL` creates none,
            -- so a clone whose phase 1c failed would own 0 against the
            -- template's ~202 while still belonging to one shared template
            -- sequence. That is the isolation regression, and it must fail here
            -- rather than as a duplicate-key error in a later insert.
            (SELECT count(*) FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = t.nsp AND c.relkind = 'S') AS seqs
        FROM target t
        "#,
    )
    .bind(schema)
    .bind(template)
    .bind(TEMPLATE_READY_TABLE)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("clone inventory query for {schema} vs template {template} failed: {e}"))?;

    let Some(clone) = inventory.iter().find(|row| row.nsp == schema) else {
        return Err(format!(
            "clone schema {schema} is not visible after cloning from template {template} \
             (the clone may not have been created at all)"
        ));
    };
    let Some(tmpl) = inventory.iter().find(|row| row.nsp == template) else {
        return Err(format!("template schema {template} is not visible while validating clone {schema}"));
    };

    if clone.tbls != tmpl.tbls
        || clone.fks != tmpl.fks
        || clone.funcs != tmpl.funcs
        || clone.views != tmpl.views
        || clone.mviews != tmpl.mviews
        || clone.triggers != tmpl.triggers
        || clone.seqs != tmpl.seqs
    {
        return Err(format!(
            "isolated schema {schema} is incomplete vs template {template}: \
             tables {0}/{1}, fks {2}/{3}, functions {4}/{5}, views {6}/{7}, \
             matviews {8}/{9}, triggers {10}/{11}, sequences {12}/{13}. An incomplete clone \
             silently falls back to `public` via search_path.",
            clone.tbls,
            tmpl.tbls,
            clone.fks,
            tmpl.fks,
            clone.funcs,
            tmpl.funcs,
            clone.views,
            tmpl.views,
            clone.mviews,
            tmpl.mviews,
            clone.triggers,
            tmpl.triggers,
            clone.seqs,
            tmpl.seqs
        ));
    }

    Ok(())
}

/// Clone the complete `template` schema into `schema` in one round trip.
///
/// `seeds` chooses which template tables phase 1b copies the rows of: pass
/// [`SeedSource::Everything`] to reproduce a freshly migrated database in full,
/// or [`SeedSource::Only`] to start from an explicit allowlist such as
/// [`SEED_REFERENCE_TABLES`]. Structure (tables, indexes, constraints,
/// sequences, functions, views, triggers) is copied identically either way.
///
/// **Precondition (the caller guarantees it):** `schema` already exists and the
/// connection's `search_path` begins with `schema`. The function does **not**
/// `CREATE SCHEMA` — callers that already created it would otherwise fail with
/// `42P06 duplicate_schema`. Any entries the caller had *after* `schema` are
/// preserved (see `clone_statement`), not replaced with a hard-coded `public`.
///
/// The `DO` block embeds `pg_get_functiondef` output, so it is executed with
/// [`sqlx::raw_sql`] (simple protocol) rather than `sqlx::query` (extended
/// protocol): the extended protocol's statement description mangles the
/// dollar-quoted bodies into a truncated statement
/// (`42601 syntax error at end of input`). The block has no bind parameters, so
/// the simple protocol is both correct and cheaper.
///
/// Finally the clone's object inventory is compared against the template's and
/// a shortfall is returned as an error: an incomplete clone silently falls back
/// to the shared `public` schema through `search_path`, which surfaces much
/// later as bizarre, order-dependent test failures.
pub async fn clone_schema_from_template(
    pool: &sqlx::PgPool,
    schema: &str,
    template: &str,
    seeds: SeedSource<'_>,
) -> Result<(), String> {
    sqlx::raw_sql(&clone_statement(schema, template, seeds)?)
        .execute(pool)
        .await
        .map_err(|e| format!("clone of {schema} from {template} failed: {e}"))?;
    validate_clone(pool, schema, template).await
}

/// Advance `schema`'s sequences past the rows its tables already hold.
///
/// Phase 1c of [`clone_statement`] does this as part of a clone. A fixture that
/// **reuses** a schema has to do it too, and the reason is easy to miss:
/// `TRUNCATE ... RESTART IDENTITY` resets every sequence to 1 *without* touching
/// `is_called`, and the rows a fixture then copies back usually carry explicit
/// ids. Re-seeding a table whose row has `id = 1` while the sequence sits at
/// `1, is_called = false` makes the very next default-id insert return 1 and
/// collide — measured on the pooled-schema path:
///
/// ```text
/// ERROR: duplicate key value violates unique constraint "server_media_quota_pkey"
/// DETAIL: Key (id)=(1) already exists.
/// ```
///
/// This is the same rule phase 1c applies, in the same order (raise only ever
/// moves a sequence forward, via `GREATEST` with its current position), so a
/// reused schema and a fresh clone end up in the same state.
///
/// Sequence discovery reuses phase 1c's catalog path — `pg_attrdef` -> the
/// `pg_depend` edge to a `relkind = 'S'` relation — rather than slicing
/// `nextval('...')` default text, and is not limited to serial columns: any
/// column whose default draws from a sequence is covered.
pub async fn advance_schema_sequences(pool: &sqlx::PgPool, schema: &str) -> Result<(), String> {
    if schema.is_empty() || !schema.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(format!("refusing to advance sequences for the non-identifier schema name {schema:?}"));
    }
    sqlx::raw_sql(&format!(
        r#"
        DO $do$
        DECLARE
            r RECORD;
            seq_q TEXT;
            max_id BIGINT;
        BEGIN
            FOR r IN
                SELECT t.relname AS tbl,
                       a.attname AS col,
                       s.relname AS seq_rel
                FROM pg_attrdef ad
                JOIN pg_class t ON t.oid = ad.adrelid
                JOIN pg_namespace tn ON tn.oid = t.relnamespace
                JOIN pg_attribute a ON a.attrelid = ad.adrelid AND a.attnum = ad.adnum
                JOIN pg_depend d ON d.classid = 'pg_attrdef'::regclass AND d.objid = ad.oid
                JOIN pg_class s ON s.oid = d.refobjid AND s.relkind = 'S'
                WHERE tn.nspname = '{schema}' AND a.attnum > 0 AND NOT a.attisdropped
                ORDER BY t.relname, a.attname
            LOOP
                seq_q := format('%I.%I', '{schema}', r.seq_rel);
                EXECUTE format(
                    'SELECT GREATEST(COALESCE(max(%I), 0), COALESCE(pg_sequence_last_value(%L::regclass), 0)) FROM %I.%I',
                    r.col, seq_q, '{schema}', r.tbl
                ) INTO max_id;
                IF max_id > 0 THEN
                    EXECUTE format('SELECT setval(%L::regclass, %s, true)', seq_q, max_id);
                END IF;
            END LOOP;
        END
        $do$;
        "#
    ))
    .execute(pool)
    .await
    .map_err(|e| format!("failed to advance the sequences of {schema}: {e}"))?;
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

    /// `LIKE ... INCLUDING ALL` copies indexes but RENAMES them
    /// (`idx_t_v_named` -> `t_v_idx`) and renames UNIQUE constraints
    /// (`uq_c_pid_named` -> `c_pid_key`), while keeping PRIMARY KEY names.
    /// Tests assert on those names (`has_index_named` has 24 call sites in
    /// `tests/integration/schema_contract_p0_tests_migrated.rs`), and
    /// `validate_clone` only compares counts, so a rename is invisible to it.
    /// Phase 1d must restore the template's names exactly.
    #[tokio::test]
    async fn clone_preserves_index_and_unique_constraint_names() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_named (id bigint PRIMARY KEY, v text, w text);
CREATE INDEX idx_unify_named_v ON unify_named (v);
CREATE UNIQUE INDEX uq_unify_named_w ON unify_named (w);
CREATE UNIQUE INDEX uq_unify_named_vw ON unify_named (v, w);
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let schema = format!("unify_names_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&pool).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{schema}", public"#)).execute(&pool).await.expect("set path");

        clone_schema_from_template(&pool, &schema, &template, SeedSource::Everything).await.expect("clone");

        // Every index NAME in the template for this table must exist verbatim
        // in the clone.
        let template_names: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT ci.relname
            FROM pg_index i
            JOIN pg_class ci ON ci.oid = i.indexrelid
            JOIN pg_class t ON t.oid = i.indrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = $1 AND t.relname = 'unify_named'
            ORDER BY ci.relname
            "#,
        )
        .bind(&template)
        .fetch_all(&pool)
        .await
        .expect("template index names");
        let clone_names: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT ci.relname
            FROM pg_index i
            JOIN pg_class ci ON ci.oid = i.indexrelid
            JOIN pg_class t ON t.oid = i.indrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = $1 AND t.relname = 'unify_named'
            ORDER BY ci.relname
            "#,
        )
        .bind(&schema)
        .fetch_all(&pool)
        .await
        .expect("clone index names");

        assert_eq!(
            clone_names, template_names,
            "clone index names must match the template exactly; \
             `LIKE ... INCLUDING ALL` renames them (idx_x -> x_idx) and phase 1d must undo that"
        );
        assert!(
            template_names.iter().any(|n| n == "idx_unify_named_v"),
            "sanity: the template must actually carry the named index"
        );

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
    }

    /// The clone must reproduce the template's inventory exactly. A shortfall
    /// is what makes queries fall back to `public`.
    /// The `WHERE` clause phase 1b is built with, and the validation that keeps
    /// a table name from escaping its string literal.
    #[test]
    fn seed_where_clause_selects_the_requested_tables() {
        let all = seed_where_clause(SeedSource::Everything).expect("everything");
        assert_eq!(all, format!("tablename <> '{TEMPLATE_READY_TABLE}'"));
        assert!(!all.contains("IN ("), "the default must not restrict the copied set");

        let some = seed_where_clause(SeedSource::Only(&["a_one", "b_two"])).expect("only");
        assert!(some.contains("tablename IN ('a_one', 'b_two')"), "got {some}");
        assert!(some.contains(&format!("tablename <> '{TEMPLATE_READY_TABLE}'")), "marker must stay excluded");

        let none = seed_where_clause(SeedSource::Only(&[])).expect("empty allowlist");
        assert_eq!(none, "1 = 0", "an empty allowlist must select no table at all");

        let rejected = seed_where_clause(SeedSource::Only(&["users; DROP SCHEMA public CASCADE"]));
        assert!(rejected.is_err(), "a non-identifier table name must be refused, got {rejected:?}");
    }

    /// `SeedSource::Only` must differ from `Everything` in exactly one way: the
    /// rows phase 1b copies. This drives both modes over one template whose
    /// baseline has a seeded table, an empty table, and a seeded table that is
    /// deliberately left out of the allowlist — the three cases that matter —
    /// and compares row counts per table.
    ///
    /// The allowlist clone must contain the allowlisted seed, must still have
    /// the structure (and zero rows) of the empty table, and must NOT contain
    /// the seed of the excluded table. That last case is what fails if the
    /// parameter is ignored and phase 1b copies everything regardless.
    #[tokio::test]
    async fn allowlist_clone_copies_only_the_allowlisted_rows() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_seed_in_allowlist (id bigint PRIMARY KEY, label text NOT NULL);
INSERT INTO unify_seed_in_allowlist (id, label) VALUES (1, 'seeded');
CREATE TABLE IF NOT EXISTS unify_seed_outside_allowlist (id bigint PRIMARY KEY, label text NOT NULL);
INSERT INTO unify_seed_outside_allowlist (id, label) VALUES (1, 'seeded');
CREATE TABLE IF NOT EXISTS unify_seed_empty (id bigint PRIMARY KEY, label text NOT NULL);
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        const ALLOWLIST: &[&str] = &["unify_seed_in_allowlist", "unify_seed_empty"];
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");

        let mut schemas = Vec::new();
        for (suffix, seeds) in [("all", SeedSource::Everything), ("only", SeedSource::Only(ALLOWLIST))] {
            let schema = format!("unify_seed_{suffix}_{}", uuid::Uuid::new_v4().as_simple());
            let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
            sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&pool).await.expect("create clone schema");
            sqlx::query(&format!(r#"SET search_path TO "{schema}", public"#)).execute(&pool).await.expect("set path");
            clone_schema_from_template(&pool, &schema, &template, seeds).await.expect("clone");
            schemas.push(schema);
        }

        // Row count per table, for the three tables the two modes can disagree on.
        let mut rows = Vec::new();
        for schema in &schemas {
            let mut per_table = Vec::new();
            for table in ["unify_seed_in_allowlist", "unify_seed_outside_allowlist", "unify_seed_empty"] {
                let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM \"{schema}\".\"{table}\""))
                    .fetch_one(&pool)
                    .await
                    .unwrap_or_else(|e| panic!("count {schema}.{table}: {e}"));
                per_table.push((table.to_string(), n));
            }
            rows.push(per_table);
        }
        let (full, allowlisted) = (rows.remove(0), rows.remove(0));

        assert_eq!(
            full,
            vec![
                ("unify_seed_in_allowlist".to_string(), 1),
                ("unify_seed_outside_allowlist".to_string(), 1),
                ("unify_seed_empty".to_string(), 0),
            ],
            "`Everything` must reproduce every seeded baseline row"
        );
        assert_eq!(
            allowlisted,
            vec![
                ("unify_seed_in_allowlist".to_string(), 1),
                ("unify_seed_outside_allowlist".to_string(), 0),
                ("unify_seed_empty".to_string(), 0),
            ],
            "`Only` must copy the allowlisted seed, still create the unlisted table's structure, \
             and NOT copy the unlisted table's rows"
        );

        // The unlisted table still exists in the allowlist clone — only its rows
        // were withheld. Without this the assertion above would also pass for a
        // clone that lost the table entirely.
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{}.unify_seed_outside_allowlist", schemas[1]))
            .fetch_one(&pool)
            .await
            .expect("to_regclass");
        assert!(exists, "the allowlist must restrict ROWS, not the table inventory");

        for schema in schemas {
            let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
        }
    }

    /// Phase 1c must leave a table the allowlist emptied at a sequence position
    /// that a default-id insert can use. The design review flagged this as the
    /// one inferred (unmeasured) consequence of restricting phase 1b; measured
    /// on 2026-09-14: `max_id` is 0, the sequence stays at `NULL / is_called =
    /// false`, and the first default-id insert returns 1.
    #[tokio::test]
    async fn allowlist_clone_can_insert_into_a_table_it_emptied() {
        let Some(url) = test_database_url() else {
            return;
        };
        // `bigserial`, not `GENERATED BY DEFAULT AS IDENTITY`: the production
        // baseline binds serial columns to sequences, and it is that
        // `nextval('<template>.<seq>')` default which phase 1c has to rebind.
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_seed_refill (id bigserial PRIMARY KEY, label text NOT NULL);
INSERT INTO unify_seed_refill (id, label) VALUES (1, 'seeded');
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let schema = format!("unify_refill_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&pool).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{schema}", public"#)).execute(&pool).await.expect("set path");

        clone_schema_from_template(&pool, &schema, &template, SeedSource::Only(&[]))
            .await
            .expect("structure-only clone");

        // Prove the allowlist actually emptied the table, so the insert below is
        // testing the emptied case and not a clone that quietly kept the row.
        let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM \"{schema}\".unify_seed_refill"))
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(rows, 0, "an empty allowlist must copy no rows, so this table must be empty");

        // Pre-mutation evidence this test can fail: with `SeedSource::Everything`
        // the same table holds `id = 1` and the insert below would still be fine
        // (the sequence advances to 2), so the discriminating case is the empty
        // one — a clone sequence that had *not* been advanced would be at the
        // template's position and this insert would collide with nothing but
        // would return 2 if the template's sequence leaked in.
        let id: i64 = sqlx::query_scalar(&format!(
            "INSERT INTO \"{schema}\".unify_seed_refill (label) VALUES ('fresh') RETURNING id"
        ))
        .fetch_one(&pool)
        .await
        .expect("a default-id insert into an emptied table must not collide");
        assert_eq!(id, 1, "the emptied table's sequence must start at 1, not inherit the template's position");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
    }

    /// The pooled-schema path's failure mode, reproduced and then closed.
    ///
    /// `TRUNCATE ... RESTART IDENTITY` puts every sequence back at 1 without
    /// resetting `is_called`, and the re-seed that follows copies rows whose
    /// explicit ids start at 1. A default-id insert then returns 1 and collides.
    /// [`advance_schema_sequences`] is what makes the reused schema agree with a
    /// fresh clone.
    #[tokio::test]
    async fn reusing_a_schema_reseeds_its_sequences() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_reseed (id bigserial PRIMARY KEY, label text NOT NULL);
INSERT INTO unify_reseed (id, label) VALUES (1, 'seeded'), (2, 'seeded');
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let schema = format!("unify_reseed_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&pool).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{schema}", public"#)).execute(&pool).await.expect("set path");
        clone_schema_from_template(&pool, &schema, &template, SeedSource::Everything).await.expect("clone");
        // A fresh clone already works, so the failure below is specific to reuse.
        let cloned_id: i64 =
            sqlx::query_scalar(&format!("INSERT INTO \"{schema}\".unify_reseed (label) VALUES ('x') RETURNING id"))
                .fetch_one(&pool)
                .await
                .expect("a fresh clone must serve a default-id insert");
        assert_eq!(cloned_id, 3, "phase 1c advances the fresh clone past the 2 copied rows");

        // The clone's sequence must be the column's serial sequence. Without
        // `OWNED BY` this is NULL and `TRUNCATE ... RESTART IDENTITY` silently
        // leaves the sequence where it was, which is what made a reused schema
        // keep stale positions (measured: an unowned sequence stayed at 42 while
        // an owned one reset to 1).
        let owned_by_column: Option<String> = sqlx::query_scalar("SELECT pg_get_serial_sequence($1, $2)")
            .bind(format!("{schema}.unify_reseed"))
            .bind("id")
            .fetch_one(&pool)
            .await
            .expect("pg_get_serial_sequence");
        assert_eq!(
            owned_by_column.as_deref(),
            Some(format!("{schema}.unify_reseed_id_seq").as_str()),
            "the clone's sequence must be OWNED BY the clone's column, or RESTART IDENTITY \
             will not reset it"
        );

        // Now replay the reuse path: truncate with identity restart, then copy
        // the template's rows back (explicit ids 1 and 2).
        sqlx::query(&format!(r#"TRUNCATE TABLE "{schema}".unify_reseed RESTART IDENTITY CASCADE"#))
            .execute(&pool)
            .await
            .expect("truncate");
        let after_truncate: (i64, bool) =
            sqlx::query_as(&format!("SELECT last_value, is_called FROM \"{schema}\".unify_reseed_id_seq"))
                .fetch_one(&pool)
                .await
                .expect("read sequence after truncate");
        assert_eq!(after_truncate, (1, false), "OWNED BY must make RESTART IDENTITY actually reset the sequence");

        sqlx::query(&format!(r#"INSERT INTO "{schema}".unify_reseed SELECT * FROM "{template}".unify_reseed"#))
            .execute(&pool)
            .await
            .expect("re-seed");

        // Precondition AND evidence in one statement: with the sequence back at 1
        // while the re-seeded rows carry ids 1 and 2, a default-id insert must
        // collide. If it ever succeeds, the repair below may be unnecessary.
        let naive: Result<i64, sqlx::Error> =
            sqlx::query_scalar(&format!("INSERT INTO \"{schema}\".unify_reseed (label) VALUES ('y') RETURNING id"))
                .fetch_one(&pool)
                .await;
        assert!(naive.is_err(), "the re-seeded rows must collide with a sequence that is back at 1");

        advance_schema_sequences(&pool, &schema).await.expect("advance sequences");
        let repaired: i64 =
            sqlx::query_scalar(&format!("INSERT INTO \"{schema}\".unify_reseed (label) VALUES ('z') RETURNING id"))
                .fetch_one(&pool)
                .await
                .expect("after advancing, the default-id insert must succeed");
        assert_eq!(repaired, 3, "the sequence must resume after the highest copied id");

        // Idempotent: running it again must not rewind or skip.
        advance_schema_sequences(&pool, &schema).await.expect("advance sequences twice");
        let again: i64 =
            sqlx::query_scalar(&format!("INSERT INTO \"{schema}\".unify_reseed (label) VALUES ('w') RETURNING id"))
                .fetch_one(&pool)
                .await
                .expect("advancing twice must stay safe");
        assert_eq!(again, 4, "a second advance must not rewind the sequence");

        let rejected = advance_schema_sequences(&pool, "no; DROP SCHEMA public CASCADE").await;
        assert!(rejected.is_err(), "a non-identifier schema name must be refused, got {rejected:?}");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
    }

    #[tokio::test]
    async fn clone_matches_template_inventory() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_parent (id bigint PRIMARY KEY);
CREATE TABLE IF NOT EXISTS unify_child (
    id bigint PRIMARY KEY,
    parent_id bigint REFERENCES unify_parent(id)
);
CREATE OR REPLACE VIEW unify_view AS SELECT id FROM unify_parent;
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let schema = format!("unify_clone_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&pool).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{schema}", public"#)).execute(&pool).await.expect("set path");

        clone_schema_from_template(&pool, &schema, &template, SeedSource::Everything).await.expect("clone");

        let counts: (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
              (SELECT count(*) FROM pg_tables WHERE schemaname = $1),
              (SELECT count(*) FROM pg_constraint c
                 JOIN pg_class r ON r.oid = c.conrelid
                 JOIN pg_namespace n ON n.oid = r.relnamespace
                WHERE n.nspname = $1 AND c.contype = 'f'),
              (SELECT count(*) FROM pg_views WHERE schemaname = $1)
            "#,
        )
        .bind(&schema)
        .fetch_one(&pool)
        .await
        .expect("counts");
        assert_eq!(counts.0, 2, "two tables expected");
        assert_eq!(counts.1, 1, "the FK must be replayed (LIKE does not copy FKs)");
        assert_eq!(counts.2, 1, "the view must be replayed");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await;
    }

    /// Cloning from a template that does not exist must be an `Err`, never a
    /// vacuous `Ok`.
    ///
    /// The inventory query used to fabricate one row per name via
    /// `SELECT unnest(ARRAY[$1, $2])`, regardless of whether those schemas
    /// existed. Both of `validate_clone`'s `find` guards were therefore dead
    /// code, and a missing template produced `0/0` on both sides — the exact
    /// silent-fallback-to-`public` condition the validation exists to catch.
    #[tokio::test]
    async fn clone_from_missing_schema_or_template_is_an_error() {
        let Some(url) = test_database_url() else {
            return;
        };
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let missing_template = format!("unify_absent_template_{}", uuid::Uuid::new_v4().as_simple());
        let clone = format!("unify_absent_clone_{}", uuid::Uuid::new_v4().as_simple());

        // Neither side exists: the catalog-sourced inventory has no rows at all,
        // so the clone guard must fire instead of comparing 0 against 0.
        let error = clone_schema_from_template(&pool, &clone, &missing_template, SeedSource::Everything)
            .await
            .expect_err("cloning from a nonexistent template must not report success");
        assert!(error.contains("is not visible"), "unexpected error: {error}");

        // The clone schema exists but is empty and the template is absent: the
        // exact 0/0 case the fabricated inventory used to accept.
        sqlx::query(&format!(r#"CREATE SCHEMA "{clone}""#)).execute(&pool).await.expect("create empty clone");
        let error = clone_schema_from_template(&pool, &clone, &missing_template, SeedSource::Everything)
            .await
            .expect_err("a nonexistent template must be rejected even for an empty clone");
        assert!(error.contains("template schema"), "unexpected error: {error}");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&pool).await;
    }

    /// A view that reads another view must be replayed *after* it.
    ///
    /// The recursive CTE's `depth` counts an object's transitive dependencies
    /// (the leaves), so ordering ascending created `unify_outer_mv` before
    /// `unify_inner_mv` existed in the clone. The stripped, unqualified
    /// reference then bound to whatever the shared `public` schema happened to
    /// hold — or failed outright when `public` lacked it.
    #[tokio::test]
    async fn clone_creates_dependent_views_in_dependency_order() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_leaf_tbl (id bigint PRIMARY KEY, n int);
CREATE MATERIALIZED VIEW unify_inner_mv AS SELECT id FROM unify_leaf_tbl;
CREATE MATERIALIZED VIEW unify_outer_mv AS SELECT id FROM unify_inner_mv;
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let clone = format!("unify_order_clone_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{clone}""#)).execute(&admin).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{clone}", public"#)).execute(&pool).await.expect("set path");

        clone_schema_from_template(&pool, &clone, &template, SeedSource::Everything).await.expect("clone");

        // `pg_get_viewdef` strips the template qualifier, so a correct clone's
        // outer matview must depend on the clone's own inner matview...
        let same_schema: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM pg_depend d
            JOIN pg_rewrite w ON w.oid = d.objid
            JOIN pg_class c ON c.oid = w.ev_class
            JOIN pg_namespace cn ON cn.oid = c.relnamespace
            JOIN pg_class ref ON ref.oid = d.refobjid
            JOIN pg_namespace rn ON rn.oid = ref.relnamespace
            WHERE cn.nspname = $1 AND c.relname = 'unify_outer_mv'
              AND rn.nspname = $1 AND ref.relname = 'unify_inner_mv'
            "#,
        )
        .bind(&clone)
        .fetch_one(&admin)
        .await
        .expect("same-schema dependency count");
        assert_eq!(same_schema, 1, "unify_outer_mv must bind to the clone's own unify_inner_mv");

        // ...and must not reach across into the shared `public` schema.
        let cross_schema: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM pg_depend d
            JOIN pg_rewrite w ON w.oid = d.objid
            JOIN pg_class c ON c.oid = w.ev_class
            JOIN pg_namespace cn ON cn.oid = c.relnamespace
            JOIN pg_class ref ON ref.oid = d.refobjid
            JOIN pg_namespace rn ON rn.oid = ref.relnamespace
            WHERE cn.nspname = $1 AND c.relname = 'unify_outer_mv' AND rn.nspname <> $1
            "#,
        )
        .bind(&clone)
        .fetch_one(&admin)
        .await
        .expect("cross-schema dependency count");
        assert_eq!(cross_schema, 0, "the clone's matview must not depend on objects outside the clone");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }

    /// A replayed trigger must reference the clone's function, not the
    /// template's.
    ///
    /// Re-pointing only the `ON` clause left `EXECUTE FUNCTION {template}.f()`
    /// in place, so every clone held a dependency on the template schema;
    /// `ensure_template_schema` drops an incomplete template with `CASCADE`,
    /// which would then cascade into every clone's triggers. Runtime routing
    /// happened to keep working because the PL/pgSQL body resolves unqualified
    /// names via the session `search_path` — the dependency was real regardless.
    #[tokio::test]
    async fn clone_retargets_trigger_functions_to_the_clone() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_trg_tbl (id bigint PRIMARY KEY, n int);
CREATE FUNCTION unify_trg_fn() RETURNS trigger AS $$
BEGIN
    NEW.n := NEW.n + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER unify_trg AFTER INSERT ON unify_trg_tbl FOR EACH ROW EXECUTE FUNCTION unify_trg_fn();
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let clone = format!("unify_trg_clone_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{clone}""#)).execute(&admin).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{clone}", public"#)).execute(&pool).await.expect("set path");

        clone_schema_from_template(&pool, &clone, &template, SeedSource::Everything).await.expect("clone");

        // A fresh connection keeps the clone schema off `search_path`, so
        // `pg_get_triggerdef` renders the function schema-qualified.
        let def: String = sqlx::query_scalar(
            r#"
            SELECT pg_get_triggerdef(t.oid)
            FROM pg_trigger t
            JOIN pg_class c ON c.oid = t.tgrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1 AND c.relname = 'unify_trg_tbl' AND NOT t.tgisinternal
            "#,
        )
        .bind(&clone)
        .fetch_one(&admin)
        .await
        .expect("trigger definition");
        assert!(
            def.contains(&format!("EXECUTE FUNCTION {clone}.")),
            "trigger must execute the clone's function, got: {def}"
        );
        assert!(!def.contains(&template), "trigger must not reference the template schema, got: {def}");

        let foreign_deps: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM pg_depend d
            JOIN pg_trigger t ON t.oid = d.objid
            JOIN pg_class c ON c.oid = t.tgrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_proc p ON p.oid = d.refobjid
            JOIN pg_namespace pn ON pn.oid = p.pronamespace
            WHERE d.classid = 'pg_trigger'::regclass AND n.nspname = $1 AND pn.nspname <> $1
            "#,
        )
        .bind(&clone)
        .fetch_one(&admin)
        .await
        .expect("trigger dependency count");
        assert_eq!(foreign_deps, 0, "clone triggers must not depend on another schema's functions");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }

    /// The clone must not clobber the caller's remaining `search_path`
    /// entries.
    ///
    /// The documented precondition only guarantees the path *begins* with the
    /// clone schema, so a hard-coded `<clone>, public` tail silently drops
    /// e.g. an `extensions` entry for the rest of the session.
    #[tokio::test]
    async fn clone_preserves_the_caller_search_path_tail() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = "CREATE TABLE IF NOT EXISTS unify_path_tbl (id bigint PRIMARY KEY);";
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let clone = format!("unify_path_clone_{}", uuid::Uuid::new_v4().as_simple());
        let extra = format!("unify_path_extra_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");
        for schema in [&clone, &extra] {
            let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&admin).await;
            sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&admin).await.expect("create schema");
        }
        sqlx::query(&format!(r#"SET search_path TO "{clone}", "{extra}", public"#))
            .execute(&pool)
            .await
            .expect("set path");

        clone_schema_from_template(&pool, &clone, &template, SeedSource::Everything).await.expect("clone");

        let effective: String = sqlx::query_scalar("SHOW search_path").fetch_one(&pool).await.expect("show path");
        assert!(effective.contains(&extra), "the caller's `{extra}` entry was dropped: {effective}");
        let clone_pos = effective.find(&clone).expect("clone must stay on the path");
        let extra_pos = effective.find(&extra).expect("extra must stay on the path");
        assert!(clone_pos < extra_pos, "the clone must remain first on the path: {effective}");

        for schema in [&clone, &extra] {
            let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&admin).await;
        }
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }

    /// The clone must carry the template's **rows**, not only its structure.
    ///
    /// `CREATE TABLE ... (LIKE ... INCLUDING ALL)` copies structure only, so
    /// every clone silently lost the singleton rows the v11 baseline seeds
    /// (`sync_stream_id`, `server_retention_policy`, `server_media_quota`),
    /// while the previous statement-by-statement fixture had them. The
    /// materialized view is asserted too because it is populated at creation
    /// time: creating it over an empty table and filling the base table
    /// afterwards would leave it permanently stale at 0 rows.
    #[tokio::test]
    async fn clone_copies_seeded_rows() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_seeded (id bigint PRIMARY KEY, note text NOT NULL);
INSERT INTO unify_seeded (id, note) VALUES (1, 'seeded'), (2, 'also-seeded') ON CONFLICT DO NOTHING;
CREATE MATERIALIZED VIEW unify_seeded_mv AS SELECT id FROM unify_seeded;
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");

        // The template itself must carry the seed rows; otherwise the clone
        // assertion below would be comparing against a bad fixture.
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");
        let template_rows: i64 = sqlx::query_scalar(&format!(r#"SELECT count(*) FROM "{template}".unify_seeded"#))
            .fetch_one(&admin)
            .await
            .expect("template row count");
        assert_eq!(template_rows, 2, "the template must hold the baseline's seeded rows");

        let clone = format!("unify_seed_clone_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{clone}""#)).execute(&pool).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{clone}", public"#)).execute(&pool).await.expect("set path");

        clone_schema_from_template(&pool, &clone, &template, SeedSource::Everything).await.expect("clone");

        // Content, not just cardinality: an off-by-one positional copy could
        // still produce two rows with the wrong values.
        let rows: Vec<(i64, String)> =
            sqlx::query_as(&format!(r#"SELECT id, note FROM "{clone}".unify_seeded ORDER BY id"#))
                .fetch_all(&pool)
                .await
                .expect("clone rows");
        assert_eq!(
            rows,
            vec![(1, "seeded".to_string()), (2, "also-seeded".to_string())],
            "the clone must carry the template's seeded row data, not just its structure"
        );

        let matview_rows: i64 = sqlx::query_scalar(&format!(r#"SELECT count(*) FROM "{clone}".unify_seeded_mv"#))
            .fetch_one(&pool)
            .await
            .expect("matview row count");
        assert_eq!(matview_rows, 2, "a matview populated at clone time must see the copied rows");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }

    /// A clone must own its own sequences instead of drawing ids from the
    /// template's.
    ///
    /// `CREATE TABLE ... (LIKE ... INCLUDING ALL)` copies a `BIGSERIAL` column's
    /// DEFAULT *expression* — `nextval('<template>.<seq>'::regclass)` — and
    /// creates no sequence in the clone. Measured on the real template: a clone
    /// of `server_retention_policy` owned **0** sequences and its `id` default
    /// still named
    /// `test_isolation_template_bec240fb79ed438b.server_retention_policy_id_seq`.
    /// The baseline seeds that row with an explicit `id = 1`, so the template
    /// sequence can sit at its start (`last_value = 1, is_called = false`) and an
    /// `INSERT` that omits `id` fails with `duplicate key value violates unique
    /// constraint`. The pre-unification fixture gave every schema fresh
    /// sequences, so this was an isolation regression.
    #[tokio::test]
    async fn clone_owns_its_sequences_and_can_insert_without_id() {
        let Some(url) = test_database_url() else {
            return;
        };
        // The explicit-id seed is load-bearing: it leaves the template's
        // sequence uncalled at its start value, which is exactly what makes a
        // template-backed clone collide.
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_seq_tbl (id BIGSERIAL PRIMARY KEY, note text NOT NULL);
INSERT INTO unify_seq_tbl (id, note) VALUES (1, 'seed') ON CONFLICT DO NOTHING;
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let clone = format!("unify_seq_clone_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("pool");
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{clone}""#)).execute(&pool).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{clone}", public"#)).execute(&pool).await.expect("set path");

        clone_schema_from_template(&pool, &clone, &template, SeedSource::Everything).await.expect("clone");

        // 1. The clone owns a sequence. Before the phase-1c fix this was 0 and
        //    the clone borrowed the template's.
        let clone_seqs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relkind = 'S'",
        )
        .bind(&clone)
        .fetch_one(&admin)
        .await
        .expect("clone sequence count");
        assert_eq!(clone_seqs, 1, "the clone must own its own sequence, not borrow the template's");

        // 2. The column default names the CLONE's sequence.
        let default_expr: String = sqlx::query_scalar(
            "SELECT pg_get_expr(ad.adbin, ad.adrelid) \
             FROM pg_attrdef ad \
             JOIN pg_class c ON c.oid = ad.adrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute a ON a.attrelid = ad.adrelid AND a.attnum = ad.adnum \
             WHERE n.nspname = $1 AND c.relname = 'unify_seq_tbl' AND a.attname = 'id'",
        )
        .bind(&clone)
        .fetch_one(&admin)
        .await
        .expect("clone default expression");
        assert!(
            default_expr.contains(&clone),
            "the clone's id default must name the clone's sequence, got: {default_expr}"
        );
        assert!(
            !default_expr.contains(&template),
            "the clone's id default must not name the template's sequence, got: {default_expr}"
        );

        // 3. The copied row is present and the clone sequence has advanced past
        //    it: an insert that omits `id` succeeds with the next id. This is the
        //    exact statement that failed with a duplicate-key error before the
        //    fix.
        let (id, note): (i64, String) = sqlx::query_as(&format!(
            r#"INSERT INTO "{clone}".unify_seq_tbl (note) VALUES ('after') RETURNING id, note"#
        ))
        .fetch_one(&pool)
        .await
        .expect("a default-id insert must not collide with the copied row");
        assert_eq!((id, note.as_str()), (2, "after"), "the clone sequence must continue past the copied row");

        // 4. The template is untouched: it still owns exactly its own sequence
        //    and still holds only the seed row.
        let template_seqs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relkind = 'S'",
        )
        .bind(&template)
        .fetch_one(&admin)
        .await
        .expect("template sequence count");
        assert_eq!(template_seqs, 1, "the template must keep exactly its own sequence");
        let template_rows: i64 = sqlx::query_scalar(&format!(r#"SELECT count(*) FROM "{template}".unify_seq_tbl"#))
            .fetch_one(&admin)
            .await
            .expect("template row count");
        assert_eq!(template_rows, 1, "the clone's insert must not have landed in the template");

        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(&admin).await;
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }

    /// Create a clone of `template` and assert the shared validator accepts it.
    async fn make_validated_clone(url: &str, admin: &PgPool, template: &str) -> (PgPool, String) {
        let clone = format!("unify_val_clone_{}", uuid::Uuid::new_v4().as_simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(url).await.expect("pool");
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(admin).await;
        sqlx::query(&format!(r#"CREATE SCHEMA "{clone}""#)).execute(&pool).await.expect("create clone schema");
        sqlx::query(&format!(r#"SET search_path TO "{clone}", public"#)).execute(&pool).await.expect("set path");
        clone_schema_from_template(&pool, &clone, template, SeedSource::Everything)
            .await
            .expect("a complete clone must validate");
        (pool, clone)
    }

    /// `validate_clone`'s inventory comparison must reject an incomplete clone.
    ///
    /// The review proved the comparison had **zero** coverage: disabling it left
    /// all 13 `test_isolation` tests passing, because every other test only
    /// asserted a *successful* clone of a complete template. This test makes the
    /// clone deliberately short — first a baseline table, then a sequence — and
    /// asserts the validator returns `Err` with both counts. Making the
    /// comparison unconditional again turns this test red.
    #[tokio::test]
    async fn validate_clone_rejects_an_incomplete_clone() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = r#"
CREATE TABLE IF NOT EXISTS unify_short_a (id BIGSERIAL PRIMARY KEY);
CREATE TABLE IF NOT EXISTS unify_short_b (id bigint PRIMARY KEY);
"#;
        let template = ensure_template_schema(&url, baseline).await.expect("template");
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");

        // Missing table: the tables count must mismatch.
        let (pool_a, clone_a) = make_validated_clone(&url, &admin, &template).await;
        sqlx::query(&format!(r#"DROP TABLE "{clone_a}".unify_short_b"#)).execute(&pool_a).await.expect("drop table");
        let error = validate_clone(&pool_a, &clone_a, &template)
            .await
            .expect_err("validate_clone must reject a clone missing a baseline table");
        assert!(error.contains(&clone_a) && error.contains("incomplete"), "unexpected error: {error}");
        assert!(error.contains("tables 1/2"), "the error must report both table counts, got: {error}");

        // Missing sequence: the sequences count must mismatch. `CASCADE` is
        // required precisely because the clone's own default now depends on its
        // own sequence (phase 1c); it drops the default, not the column.
        let (pool_b, clone_b) = make_validated_clone(&url, &admin, &template).await;
        sqlx::query(&format!(r#"DROP SEQUENCE "{clone_b}".unify_short_a_id_seq CASCADE"#))
            .execute(&pool_b)
            .await
            .expect("drop sequence");
        let error = validate_clone(&pool_b, &clone_b, &template)
            .await
            .expect_err("validate_clone must reject a clone missing a baseline sequence");
        assert!(error.contains("sequences 0/1"), "the error must report both sequence counts, got: {error}");

        for (pool, clone) in [(&pool_a, &clone_a), (&pool_b, &clone_b)] {
            let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{clone}" CASCADE"#)).execute(pool).await;
        }
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }

    /// A hung advisory-lock holder must not stall every later test process.
    ///
    /// `pg_advisory_lock` waits forever and the pool's `acquire_timeout` only
    /// bounds *acquiring a connection*, not a lock wait on one already held, so
    /// the try-lock polling in `acquire_template_lock` is the only bound. This
    /// test holds the lock on a separate session and asserts the wait gives up
    /// promptly with an error naming the key, and that the timed-out
    /// acquisition did not build the template.
    #[tokio::test]
    async fn template_lock_wait_is_bounded_and_reports_the_key() {
        let Some(url) = test_database_url() else {
            return;
        };
        let holder = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("holder pool");
        // Other DB tests in the same process build their templates under the
        // same session-scoped lock, so a single try can lose the race. Retry
        // with a generous deadline (holders release in milliseconds-to-seconds)
        // instead of asserting on the first attempt.
        let acquire_deadline = std::time::Instant::now() + Duration::from_secs(30);
        let held = loop {
            let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
                .bind(TEMPLATE_ADVISORY_LOCK_KEY)
                .fetch_one(&holder)
                .await
                .expect("take the advisory lock for the test");
            if got {
                break true;
            }
            if std::time::Instant::now() >= acquire_deadline {
                break false;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert!(held, "the probe must eventually be able to take the template advisory lock");

        let probe_baseline = "CREATE TABLE IF NOT EXISTS unify_lock_probe (id bigint PRIMARY KEY);";
        let started = std::time::Instant::now();
        let result = ensure_template_schema_with_lock_timeout(&url, probe_baseline, Duration::from_millis(150)).await;
        let elapsed = started.elapsed();

        let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
            .bind(TEMPLATE_ADVISORY_LOCK_KEY)
            .fetch_one(&holder)
            .await
            .expect("release the advisory lock");
        assert!(unlocked, "the probe must still own the lock it took");

        let error = result.expect_err("a held template lock must not block forever");
        assert!(error.contains("timed out"), "the timeout error must say so, got: {error}");
        assert!(
            error.contains("0x53594e4150535445"),
            "the timeout error must name the advisory-lock key, got: {error}"
        );
        assert!(elapsed < Duration::from_secs(10), "the wait must be bounded by the injected 150ms, took {elapsed:?}");

        let probe_template = template_schema_name(probe_baseline);
        let built: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
            .bind(&probe_template)
            .fetch_one(&holder)
            .await
            .expect("probe template lookup");
        assert!(!built, "a timed-out acquisition must not build the template");
    }

    /// [`prune_isolation_templates`] treats a marker table with zero rows
    /// (legacy template) as "potentially in use", backfills a fresh timestamp,
    /// and spares it — preventing accidental deletion of a template another
    /// session may still be cloning from.
    #[tokio::test]
    async fn prune_backfills_legacy_zero_row_marker_and_spares_it() {
        let Some(url) = test_database_url() else {
            return;
        };
        let baseline = "CREATE TABLE IF NOT EXISTS unify_prune_test (id bigint PRIMARY KEY);";
        let template = template_schema_name(baseline);
        let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");

        // Build a complete template first.
        let _ = ensure_template_schema(&url, baseline).await.expect("build template");
        let has_rows: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM \"{template}\".\"{TEMPLATE_READY_TABLE}\""))
                .fetch_one(&admin)
                .await
                .expect("count marker rows after build");
        assert!(has_rows > 0, "a freshly built template must have at least one marker row, got {has_rows}");

        // Simulate a legacy template: DELETE all rows from the marker table.
        let deleted: u64 = sqlx::query(&format!("DELETE FROM \"{template}\".\"{TEMPLATE_READY_TABLE}\""))
            .execute(&admin)
            .await
            .expect("delete marker rows")
            .rows_affected();
        assert!(deleted > 0, "must have deleted at least one row, got {deleted}");

        // Now run prune while keeping the same template as the "protected" one.
        // The prune function should detect row_count==0, backfill a row, and spare it.
        let dropped = prune_stale_isolation_templates(&admin, &template).await.expect("prune");
        assert!(!dropped.iter().any(|s| s == &template), "the legacy template must not be dropped, got {dropped:?}");

        // Verify it was backfilled.
        let backfilled: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM \"{template}\".\"{TEMPLATE_READY_TABLE}\""))
                .fetch_one(&admin)
                .await
                .expect("count marker rows after prune");
        assert!(backfilled > 0, "prune must have backfilled a marker row, got {backfilled}");

        // Cleanup
        let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
    }
}
