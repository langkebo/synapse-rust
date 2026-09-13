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

/// Advisory-lock key guarding shared template creation.
const TEMPLATE_ADVISORY_LOCK_KEY: i64 = 0x5359_4E41_5053_5445;
/// Marker table written into the template only after a complete build.
const TEMPLATE_READY_TABLE: &str = "_synapse_test_template_ready";

// ============================================================================
// Shared baseline template
// ============================================================================
//
// Before: `IsolatedTestPool::new()` split and executed the v11 baseline
// (253 CREATE TABLE + 373 CREATE INDEX + 45 ALTER TABLE + functions/views/
// triggers) as one round trip *per statement* on every call.
//
// Measured on the instrumented build (`[ISO_TIMING]`, 89-case cohort):
//   baseline_replay median 4.416s  p90 5.093s  max 5.815s   (99% of setup)
//   admin_connect   median 0.030s
//   cleanup         median 0.520s
// Under --test-threads 8 this DDL storm contends and the tail crosses the
// 30s acquire window, surfacing as `Operation timed out`.
//
// After: the baseline is applied exactly once per database into a *template*
// schema, and each test clones it with a single `DO $$` round trip.
//
// Serialization: nextest runs one process per test, so a process-local
// `OnceLock` cannot stop concurrent processes from racing to `CREATE SCHEMA`.
// A session-level advisory lock serializes the build across processes. It must
// outlive individual statements, so it is session-scoped (`pg_advisory_lock`)
// rather than transaction-scoped.

/// Fingerprint of the baseline SQL, so a template built from a stale baseline
/// is never reused. Content-hashed (not mtime) so it is reproducible across
/// checkouts.
fn baseline_fingerprint() -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for sql in [
        include_str!("../../migrations/00000000_unified_schema_v11.sql"),
        include_str!("../../migrations/00000001_extensions_v10.sql"),
    ] {
        for byte in sql.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

/// Name of the shared template schema for the current baseline content.
fn template_schema_name() -> String {
    format!("test_isolation_template_{}", baseline_fingerprint())
}

/// Resolve the test database URL.
///
/// Precedence: `TEST_DATABASE_URL`, then `DATABASE_URL`, then the documented
/// local convention. The env-var path is deliberately zero-probe so the hot
/// path never pays connection-probe latency.
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

/// Ensure the shared template schema exists and is complete for the current
/// baseline. Returns the template schema name.
async fn ensure_template_schema(db_url: &str) -> Result<String, sqlx::Error> {
    let template = template_schema_name();
    let admin_pool =
        PgPoolOptions::new().max_connections(2).acquire_timeout(Duration::from_secs(60)).connect(db_url).await?;

    // The advisory lock is session-scoped, so it must be taken and released on
    // the same connection that performs the build.
    let mut conn = admin_pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock($1)").bind(TEMPLATE_ADVISORY_LOCK_KEY).execute(&mut *conn).await?;

    let result = build_template(&mut conn, &template).await;

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

async fn build_template(conn: &mut sqlx::PgConnection, template: &str) -> Result<(), sqlx::Error> {
    let ready: bool = sqlx::query_scalar("SELECT to_regclass(format('%I.%I', $1, $2)) IS NOT NULL")
        .bind(template)
        .bind(TEMPLATE_READY_TABLE)
        .fetch_one(&mut *conn)
        .await?;
    if ready {
        return Ok(());
    }

    // Either absent, or a previous build was interrupted (timeout / SIGKILL /
    // panic) and left a table-less schema. Cloning from an incomplete template
    // would silently fall back to `public` via search_path, so rebuild.
    sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{}" CASCADE"#, template)).execute(&mut *conn).await?;
    sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, template)).execute(&mut *conn).await?;
    sqlx::query(&format!(r#"SET search_path TO "{}", public"#, template)).execute(&mut *conn).await?;

    // Fail loudly: a partially applied baseline leaves the template incomplete,
    // and every clone would then silently read/write the shared `public` schema.
    let baseline_sql = strip_copy_blocks(include_str!("../../migrations/00000000_unified_schema_v11.sql"));
    let extensions_sql = include_str!("../../migrations/00000001_extensions_v10.sql");
    for (label, sql) in [("baseline", baseline_sql.as_str()), ("extensions", extensions_sql)] {
        for stmt in split_sql_statements(sql) {
            let trimmed = stmt.trim();
            if trimmed.is_empty() {
                continue;
            }
            sqlx::query(trimmed).execute(&mut *conn).await.map_err(|error| {
                sqlx::Error::Protocol(format!(
                    "template {label} statement failed: {error} | stmt head: {}",
                    first_line(trimmed)
                ))
            })?;
        }
    }

    // Readiness marker: written only after every statement succeeded, so its
    // presence is a truthful statement about completeness.
    sqlx::query(&format!(
        r#"CREATE TABLE "{}"."{TEMPLATE_READY_TABLE}" (built_at timestamptz NOT NULL DEFAULT now())"#,
        template
    ))
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// Build the single-round-trip clone statement for `schema` from `template`.
///
/// Two phases, deliberately:
///
/// * Phase 1 (`search_path` = template): `CREATE TABLE ... (LIKE ... INCLUDING ALL)`.
///   This carries columns, defaults, generated expressions, identity, indexes
///   and PRIMARY KEY / UNIQUE / CHECK constraints. It does **not** carry
///   FOREIGN KEYs (measured: 0/127 survived the copy), so those are replayed
///   explicitly in phase 2.
///
/// * Phase 2 (`search_path` = clone): replay functions, views, materialized
///   views, foreign keys and triggers, which `LIKE` cannot copy. The
///   `search_path` matters for *correctness*: a PL/pgSQL body is not
///   schema-bound, so a function created while `search_path` points at the
///   template would silently resolve unqualified names to the **template's**
///   tables — cross-schema writes from the clone. Replaying with the clone on
///   `search_path` binds them to the clone. Verified: no cloned function body
///   mentions the template schema afterwards.
fn clone_statement(schema: &str, template: &str) -> String {
    format!(
        r#"
        DO $do$
        DECLARE
            r RECORD;
            def TEXT;
        BEGIN
            EXECUTE format('CREATE SCHEMA %I', '{schema}');
        
            -- Phase 1: every table, with indexes / defaults / CHECK / PK / FK.
            FOR r IN
                SELECT tablename FROM pg_tables WHERE schemaname = '{template}' ORDER BY tablename
            LOOP
                EXECUTE format(
                    'CREATE TABLE %I.%I (LIKE %I.%I INCLUDING ALL)',
                    '{schema}', r.tablename, '{template}', r.tablename
                );
            END LOOP;

            -- Phase 2: non-table objects must bind to the clone, not the template.
            EXECUTE format('SET search_path TO %I, public', '{schema}');

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

            -- Views / materialized views. Ordered by dependency depth so a view
            -- that reads another view is created after it.
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
                ORDER BY max(deps.depth), c.relname
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

            -- Triggers. The clone's own tables carry the trigger; the function
            -- resolves to the clone because `search_path` points there.
            FOR r IN
                SELECT c.relname AS tbl, t.tgname AS name, pg_get_triggerdef(t.oid) AS def
                FROM pg_trigger t
                JOIN pg_class c ON c.oid = t.tgrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = '{template}' AND NOT t.tgisinternal
            LOOP
                def := replace(r.def, ' ON {template}.', ' ON {schema}.');
                def := replace(def, ' ON "{template}".', ' ON "{schema}".');
                EXECUTE def;
            END LOOP;
        END
        $do$;
        "#
    )
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

/// Verify the clone reproduces the template's object inventory.
///
/// A clone that silently lacks tables/functions/views makes every query for the
/// missing objects resolve against the shared `public` schema via the
/// `search_path`. That is the failure mode behind the order-dependent
/// `media::tests` and `*::db_tests` breakage, so it must be an immediate error.
async fn validate_clone(pool: &PgPool, schema: &str, template: &str) -> Result<(), sqlx::Error> {
    // One query against a fixed set of relations, aggregating per schema.
    /// Object inventory for one schema. Named fields rather than a 7-tuple:
    /// positional access to seven `i64`s is exactly the kind of thing that
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
    }

    let inventory: Vec<Inventory> = sqlx::query_as(
        r#"
        WITH target AS (
            SELECT unnest(ARRAY[$1, $2]) AS nsp
        )
        SELECT t.nsp,
            (SELECT count(*) FROM pg_tables tb WHERE tb.schemaname = t.nsp) AS tbls,
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
              WHERE n.nspname = t.nsp AND NOT tr.tgisinternal) AS triggers
        FROM target t
        "#,
    )
    .bind(schema)
    .bind(template)
    .fetch_all(pool)
    .await?;

    let Some(clone) = inventory.iter().find(|row| row.nsp == schema) else {
        return Err(sqlx::Error::Protocol(format!("schema {schema} not visible after clone")));
    };
    let Some(tmpl) = inventory.iter().find(|row| row.nsp == template) else {
        return Err(sqlx::Error::Protocol(format!("template {template} not visible")));
    };

    if clone.tbls != tmpl.tbls
        || clone.fks != tmpl.fks
        || clone.funcs != tmpl.funcs
        || clone.views != tmpl.views
        || clone.mviews != tmpl.mviews
        || clone.triggers != tmpl.triggers
    {
        return Err(sqlx::Error::Protocol(format!(
            "isolated schema {schema} is incomplete vs template {template}: \
             tables {0}/{1}, fks {2}/{3}, functions {4}/{5}, views {6}/{7}, \
             matviews {8}/{9}, triggers {10}/{11}. An incomplete clone silently \
             falls back to `public` via search_path.",
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
            tmpl.triggers
        )));
    }

    Ok(())
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
        let db_url = test_database_url();

        let template = ensure_template_schema(&db_url).await?;

        let schema = format!("test_{}", uuid::Uuid::new_v4().as_simple());

        // One round trip: every table, index, constraint, function, view and
        // trigger. See `clone_statement` for why the search_path switch matters.
        let clone_pool =
            PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(60)).connect(&db_url).await?;
        // `raw_sql` (simple protocol) rather than `query` (extended protocol):
        // the DO block embeds `pg_get_functiondef` output, which contains
        // dollar-quoted bodies and `::` casts that the extended-protocol
        // statement description mangles into a truncated statement
        // (`42601 syntax error at end of input`). The block has no bind
        // parameters, so the simple protocol is both correct and cheaper.
        sqlx::raw_sql(&clone_statement(&schema, &template)).execute(&clone_pool).await?;

        // Fail loud rather than fall back to `public`. A clone missing objects
        // makes queries resolve against the shared `public` schema through the
        // `search_path`, which shows up later as bizarre, order-dependent test
        // failures (see docs/audit/P3_* / P5_*). Comparing against the template
        // turns that into an immediate, local error.
        validate_clone(&clone_pool, &schema, &template).await?;
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
