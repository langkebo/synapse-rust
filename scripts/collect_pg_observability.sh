#!/usr/bin/env bash
set -euo pipefail

# Collect PostgreSQL observability snapshots for baseline or 7x24 sampling.
# Supports direct psql access or docker exec via PSQL_CONTAINER.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/artifacts/db-observability/pg}"
SAMPLE_LABEL="${SAMPLE_LABEL:-$(date +%Y%m%d-%H%M%S)}"
SAMPLE_DIR="$OUTPUT_DIR/$SAMPLE_LABEL"

DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-postgres}"
DB_PASSWORD="${DB_PASSWORD:-}"
PSQL_CONTAINER="${PSQL_CONTAINER:-}"

mkdir -p "$SAMPLE_DIR"

if [[ -n "$DB_PASSWORD" ]]; then
    export PGPASSWORD="$DB_PASSWORD"
fi

run_psql() {
    local sql="$1"
    if [[ -n "$PSQL_CONTAINER" ]]; then
        docker exec -e PGPASSWORD="${DB_PASSWORD:-}" "$PSQL_CONTAINER" \
            psql -X -A -F $'\t' -v ON_ERROR_STOP=1 -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -P pager=off -c "$sql"
    else
        psql -X -A -F $'\t' -v ON_ERROR_STOP=1 -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -P pager=off -c "$sql"
    fi
}

run_psql_file() {
    local sql="$1"
    local output_file="$2"
    {
        echo "-- sample_label: $SAMPLE_LABEL"
        echo "-- generated_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "-- db: $DB_NAME"
        echo
        run_psql "$sql"
    } >"$output_file"
}

run_psql_file "SELECT now() AS collected_at, current_database() AS database_name, version() AS server_version;" \
    "$SAMPLE_DIR/00_meta.sql.txt"

run_psql_file "SELECT * FROM pg_stat_statements ORDER BY total_exec_time DESC LIMIT 20;" \
    "$SAMPLE_DIR/10_pg_stat_statements_top_total_exec.sql.txt"

run_psql_file "SELECT queryid, calls, mean_exec_time, total_exec_time, rows, shared_blks_hit, shared_blks_read, query FROM pg_stat_statements WHERE query NOT LIKE '%pg_stat_statements%' ORDER BY mean_exec_time DESC LIMIT 20;" \
    "$SAMPLE_DIR/11_pg_stat_statements_top_mean_exec.sql.txt"

run_psql_file "SELECT wait_event_type, wait_event, state, COUNT(*) AS count FROM pg_stat_activity WHERE datname = current_database() GROUP BY wait_event_type, wait_event, state ORDER BY count DESC;" \
    "$SAMPLE_DIR/20_pg_stat_activity_waits.sql.txt"

run_psql_file "SELECT pid, usename, state, wait_event_type, wait_event, now() - query_start AS running_for, query FROM pg_stat_activity WHERE datname = current_database() AND state <> 'idle' ORDER BY query_start ASC;" \
    "$SAMPLE_DIR/21_pg_stat_activity_active.sql.txt"

run_psql_file "SELECT blocked_activity.pid AS blocked_pid, blocked_activity.query AS blocked_query, blocking_activity.pid AS blocking_pid, blocking_activity.query AS blocking_query FROM pg_catalog.pg_locks blocked_locks JOIN pg_catalog.pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid JOIN pg_catalog.pg_locks blocking_locks ON blocking_locks.locktype = blocked_locks.locktype AND blocking_locks.database IS NOT DISTINCT FROM blocked_locks.database AND blocking_locks.relation IS NOT DISTINCT FROM blocked_locks.relation AND blocking_locks.page IS NOT DISTINCT FROM blocked_locks.page AND blocking_locks.tuple IS NOT DISTINCT FROM blocked_locks.tuple AND blocking_locks.transactionid IS NOT DISTINCT FROM blocked_locks.transactionid AND blocking_locks.classid IS NOT DISTINCT FROM blocked_locks.classid AND blocking_locks.objid IS NOT DISTINCT FROM blocked_locks.objid AND blocking_locks.objsubid IS NOT DISTINCT FROM blocked_locks.objsubid AND blocking_locks.pid != blocked_locks.pid JOIN pg_catalog.pg_stat_activity blocking_activity ON blocking_activity.pid = blocking_locks.pid WHERE NOT blocked_locks.granted;" \
    "$SAMPLE_DIR/30_blocking_locks.sql.txt"

run_psql_file "SELECT relname, seq_scan, seq_tup_read, idx_scan, n_tup_ins, n_tup_upd, n_tup_del, n_live_tup, n_dead_tup FROM pg_stat_user_tables ORDER BY seq_tup_read DESC LIMIT 30;" \
    "$SAMPLE_DIR/40_pg_stat_user_tables_hotspots.sql.txt"

run_psql_file "SELECT schemaname, relname, indexrelname, idx_scan, idx_tup_read, idx_tup_fetch FROM pg_stat_user_indexes ORDER BY idx_scan ASC, idx_tup_fetch ASC LIMIT 50;" \
    "$SAMPLE_DIR/41_pg_stat_user_indexes_low_usage.sql.txt"

run_psql_file "SELECT relname, last_vacuum, last_autovacuum, last_analyze, last_autoanalyze, vacuum_count, autovacuum_count, analyze_count, autoanalyze_count, n_dead_tup FROM pg_stat_user_tables ORDER BY n_dead_tup DESC LIMIT 30;" \
    "$SAMPLE_DIR/42_pg_stat_user_tables_vacuum.sql.txt"

run_psql_file "SELECT datname, xact_commit, xact_rollback, blks_read, blks_hit, tup_returned, tup_fetched, tup_inserted, tup_updated, tup_deleted, deadlocks, temp_files, temp_bytes FROM pg_stat_database WHERE datname = current_database();" \
    "$SAMPLE_DIR/50_pg_stat_database.sql.txt"

run_psql_file "SELECT extname, extversion FROM pg_extension ORDER BY extname;" \
    "$SAMPLE_DIR/60_pg_extension.sql.txt"

cat >"$SAMPLE_DIR/README.txt" <<EOF
PostgreSQL observability snapshot
sample_label=$SAMPLE_LABEL
generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
db_host=$DB_HOST
db_port=$DB_PORT
db_name=$DB_NAME
db_user=$DB_USER
psql_container=${PSQL_CONTAINER:-none}
EOF

echo "PostgreSQL snapshot written to: $SAMPLE_DIR"
