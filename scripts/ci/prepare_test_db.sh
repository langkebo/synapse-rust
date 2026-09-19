#!/usr/bin/env bash
# CI seed step for the test database `synapse_test`.
#
# Creates TWO coexisting schemas so the whole test matrix can share one DB:
#   * `public`          — storage db_tests (`connect_shared_test_pool`) connect
#                         straight to `public`; needs the full migrated baseline.
#   * `test_template_ci`— services/root `prepare_shared_test_pool` clone THIS
#                         schema for per-test isolation.
#
# Test steps then set `TEST_DB_TEMPLATE_SCHEMA=test_template_ci` (see
# `src/test_utils.rs::configured_test_db_template_schema`). When that env is
# set, `prepare_shared_test_pool` takes the **verify-only** path
# (`ensure_template_schema_exists`) and NEVER enters `init_template_schema` —
# the function that contains the `DROP SCHEMA public` that destroyed a real
# deployment on 2026-09-12. This makes the wipe structurally impossible, not
# merely guarded.
#
# Both schemas come from ONE implementation, `scripts/init_test_public_schema.sh`,
# which applies `migrations/*.sql` with psql one file at a time. This script used
# to drive `sqlx migrate run` twice (with a `?options=-c search_path=…` URL hack
# for the template). That cannot work with the current baseline:
#
#   * sqlx-cli 0.8.x does **not** honour a migration file's `-- no-transaction`
#     directive — reproduced 2026-09-19 with a minimal probe (a migration
#     containing only the directive plus one `CREATE INDEX CONCURRENTLY` still
#     failed with "cannot run inside a transaction block"), while
#     `sqlx migrate run --help` offers no `--no-transaction` flag either;
#   * the baseline contains 14 such statements, so the seed step aborted with
#     `error: while executing migration 0: … CREATE INDEX CONCURRENTLY cannot run
#     inside a transaction block` — i.e. `test_template_ci` was never actually
#     (re)built on any machine that ran this script.
#
# psql runs in autocommit and is the same path `docker/db_migrate.sh` (the
# documented migration source of truth) uses. `RESET_PUBLIC=0` is passed so the
# apply is idempotent and never `DROP SCHEMA public CASCADE` — that cascade also
# removes objects in *other* schemas that depend on public's extensions (e.g. the
# `gin_trgm_ops` indexes on the isolation templates), which silently degrades a
# template that still carries its "ready" marker.
#
# The DB-name guard in src/test_utils.rs (`current_database()` contains "test")
# additionally protects against pointing at a deployed DB. That flag stays OFF
# everywhere in CI — if a test is ever repointed at the application database
# `synapse`, the guard fails fast instead of silently wiping it.
set -euo pipefail

cd "$(dirname "$0")/../.."   # repo root (scripts/ci -> repo root)

export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgresql://synapse:synapse@localhost:5432/synapse_test}"
export DATABASE_URL="$TEST_DATABASE_URL"
unset SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE

TEMPLATE_SCHEMA="${TEST_DB_TEMPLATE_SCHEMA:-test_template_ci}"

echo "==> [1/3] applying the migration baseline to public in $TEST_DATABASE_URL"
RESET_PUBLIC=0 TARGET_SCHEMA=public bash scripts/init_test_public_schema.sh

echo "==> [2/3] building template schema '$TEMPLATE_SCHEMA' (same migrations, pinned search_path)"
# Rebuild it from scratch. The previous sqlx-based version never dropped it: once
# `_sqlx_migrations` recorded the baseline as applied, the template pass became a
# no-op and a template built from an *older* baseline stayed stale forever
# (silently missing columns/tables added since). Re-applying over a stale schema
# is not a repair either — the baseline is idempotent on a current schema, not on
# an arbitrary older one (measured 2026-09-19: `column "recipient_user_id" does
# not exist` at migrations/…_v12.sql:3655 against a stale template). Dropping is
# safe: the template is a derived cache, and dropping it cannot touch `public`
# (the dependency direction is template → public).
psql "$TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -c "DROP SCHEMA IF EXISTS \"$TEMPLATE_SCHEMA\" CASCADE" >/dev/null
# Order matters: public must already carry the extensions — `CREATE EXTENSION
# IF NOT EXISTS` is database-wide, so the second pass no-ops instead of installing
# them inside the template. The unqualified DDL of this pass lands in the FIRST
# search_path entry, which the init script pins via PGOPTIONS.
TARGET_SCHEMA="$TEMPLATE_SCHEMA" bash scripts/init_test_public_schema.sh

# Write the ready-marker that `synapse-test-utils::template_marker_dir()` uses, so
# `scripts/cleanup_test_schemas.sh` recognises this template through its generic
# marker mechanism (keep reason #1) instead of depending on the static
# `STATIC_KEEP` list. That is what keeps a *future* shell-created live template
# from being one forgotten list entry away from a `--apply` CASCADE drop
# (sweep §15.8.4 / this round's §2.5). `STATIC_KEEP` stays as an unconditional
# backstop for templates seeded before this change or by non-marker-aware tools.
MARKER_ROOT="${SYNAPSE_TEMPLATE_MARKER_DIR:-${CARGO_TARGET_TMPDIR:-$PWD/target/tmp}}"
MARKER_DIR="$MARKER_ROOT/synapse_test_templates"
mkdir -p "$MARKER_DIR"
touch "$MARKER_DIR/synapse_test_template_ready_${TEMPLATE_SCHEMA}"
echo "==> ready-marker written: $MARKER_DIR/synapse_test_template_ready_${TEMPLATE_SCHEMA}"

echo "==> [3/3] verifying both schemas"
# 用 $TEST_DATABASE_URL 而不是硬编码 `-d synapse_test`：库名是本脚本的输入
# （第 34 行的默认值可在调用处覆盖），硬编码会让 `PUBLIC_TABLES`/`TEMPLATE_TABLES`
# 静默统计**另一个库**的 schema —— 与 docker/db_migrate.sh 的 H-14 同型。
PUBLIC_TABLES=$(psql "$TEST_DATABASE_URL" -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'")
TEMPLATE_TABLES=$(psql "$TEST_DATABASE_URL" -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema='${TEMPLATE_SCHEMA}' AND table_type='BASE TABLE'")
echo "==> public: ${PUBLIC_TABLES} tables; ${TEMPLATE_SCHEMA}: ${TEMPLATE_TABLES} tables"
if [ "$PUBLIC_TABLES" -lt 200 ] || [ "$TEMPLATE_TABLES" -lt 200 ]; then
  echo "::error::Seed incomplete: expected >=200 tables in both public and ${TEMPLATE_SCHEMA} (got ${PUBLIC_TABLES} / ${TEMPLATE_TABLES})"
  exit 1
fi

echo "==> synapse_test ready: public + ${TEMPLATE_SCHEMA} coexist."
