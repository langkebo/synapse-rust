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

use sqlx::postgres::{PgPool, PgPoolOptions};
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

/// Build the single-round-trip clone statement for `schema` from `template`.
///
/// Three steps, deliberately:
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
/// * Phase 2 (`search_path` = clone, then the caller's remaining entries):
///   replay functions, views, materialized views, foreign keys and triggers,
///   which `LIKE` cannot copy. The `search_path` matters for *correctness*: a
///   PL/pgSQL body is not schema-bound, so a function created while
///   `search_path` points at the template would silently resolve unqualified
///   names to the **template's** tables — cross-schema writes from the clone.
///   Replaying with the clone on `search_path` binds them to the clone.
///   Verified: no cloned function body mentions the template schema
///   afterwards. The caller's tail (everything after the clone) is preserved
///   rather than replaced with a literal `public`, so a caller path such as
///   `<clone>, public, extensions` keeps its `extensions` entry.
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
fn clone_statement(schema: &str, template: &str) -> String {
    format!(
        r#"
        DO $do$
        DECLARE
            r RECORD;
            def TEXT;
            rest TEXT;
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
            FOR r IN
                SELECT tablename FROM pg_tables
                WHERE schemaname = '{template}' AND tablename <> '{TEMPLATE_READY_TABLE}'
                ORDER BY tablename
            LOOP
                EXECUTE format(
                    'INSERT INTO %I.%I SELECT * FROM %I.%I',
                    '{schema}', r.tablename, '{template}', r.tablename
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
    )
}

/// Verify the clone reproduces the template's object inventory.
///
/// A clone that silently lacks tables/functions/views makes every query for the
/// missing objects resolve against the shared `public` schema via the
/// `search_path`. That is the failure mode behind the order-dependent
/// `media::tests` and `*::db_tests` breakage, so it must be an immediate error.
///
/// The template-only bookkeeping table ([`TEMPLATE_READY_TABLE`]) is excluded
/// from the table count on both sides, matching [`clone_statement`], which does
/// not copy it.
async fn validate_clone(pool: &PgPool, schema: &str, template: &str) -> Result<(), String> {
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
              WHERE n.nspname = t.nsp AND NOT tr.tgisinternal) AS triggers
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
    {
        return Err(format!(
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
        ));
    }

    Ok(())
}

/// Clone the complete `template` schema into `schema` in one round trip.
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
pub async fn clone_schema_from_template(pool: &sqlx::PgPool, schema: &str, template: &str) -> Result<(), String> {
    sqlx::raw_sql(&clone_statement(schema, template))
        .execute(pool)
        .await
        .map_err(|e| format!("clone of {schema} from {template} failed: {e}"))?;
    validate_clone(pool, schema, template).await
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

    /// The clone must reproduce the template's inventory exactly. A shortfall
    /// is what makes queries fall back to `public`.
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

        clone_schema_from_template(&pool, &schema, &template).await.expect("clone");

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
        let error = clone_schema_from_template(&pool, &clone, &missing_template)
            .await
            .expect_err("cloning from a nonexistent template must not report success");
        assert!(error.contains("is not visible"), "unexpected error: {error}");

        // The clone schema exists but is empty and the template is absent: the
        // exact 0/0 case the fabricated inventory used to accept.
        sqlx::query(&format!(r#"CREATE SCHEMA "{clone}""#)).execute(&pool).await.expect("create empty clone");
        let error = clone_schema_from_template(&pool, &clone, &missing_template)
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

        clone_schema_from_template(&pool, &clone, &template).await.expect("clone");

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

        clone_schema_from_template(&pool, &clone, &template).await.expect("clone");

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

        clone_schema_from_template(&pool, &clone, &template).await.expect("clone");

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

        clone_schema_from_template(&pool, &clone, &template).await.expect("clone");

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
}
