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
# Implementation notes:
#   * Migrations are NOT schema-qualified (`CREATE TABLE IF NOT EXISTS x`), so
#     `sqlx migrate run` lands tables in the FIRST schema of `search_path`.
#   * The template is therefore built by re-running the same migrations with
#     `PGOPTIONS='-c search_path=test_template_ci,public'` so they land in the
#     template schema instead of public.
#
# The DB-name guard in src/test_utils.rs (`current_database()` contains "test")
# additionally protects against pointing at a deployed DB. That flag stays OFF
# everywhere in CI — if a test is ever repointed at the application database
# `synapse`, the guard fails fast instead of silently wiping it.
set -euo pipefail

cd "$(dirname "$0")/../.."   # repo root (scripts/ci -> repo root)

export SQLX_OFFLINE=true
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgresql://synapse:synapse@localhost:5432/synapse_test}"
export DATABASE_URL="$TEST_DATABASE_URL"
unset SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE

TEMPLATE_SCHEMA="${TEST_DB_TEMPLATE_SCHEMA:-test_template_ci}"

echo "==> [1/3] migrating public baseline into $TEST_DATABASE_URL"
sqlx migrate run --source artifacts/sqlx-migrations

echo "==> [2/3] building template schema '$TEMPLATE_SCHEMA' (same migrations, pinned search_path)"
PGOPTIONS="-c search_path=${TEMPLATE_SCHEMA},public" \
  sqlx migrate run --source artifacts/sqlx-migrations

echo "==> [3/3] verifying both schemas"
PUBLIC_TABLES=$(psql -d synapse_test -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'")
TEMPLATE_TABLES=$(psql -d synapse_test -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema='${TEMPLATE_SCHEMA}' AND table_type='BASE TABLE'")
echo "==> public: ${PUBLIC_TABLES} tables; ${TEMPLATE_SCHEMA}: ${TEMPLATE_TABLES} tables"
if [ "$PUBLIC_TABLES" -lt 200 ] || [ "$TEMPLATE_TABLES" -lt 200 ]; then
  echo "::error::Seed incomplete: expected >=200 tables in both public and ${TEMPLATE_SCHEMA} (got ${PUBLIC_TABLES} / ${TEMPLATE_TABLES})"
  exit 1
fi

echo "==> synapse_test ready: public + ${TEMPLATE_SCHEMA} coexist."