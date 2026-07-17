#!/usr/bin/env bash
set -euo pipefail

# Collect Redis observability snapshots for latency, slowlog, memory, and clients.
# Supports direct redis-cli access or docker exec via REDIS_CONTAINER.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/artifacts/db-observability/redis}"
SAMPLE_LABEL="${SAMPLE_LABEL:-$(date +%Y%m%d-%H%M%S)}"
SAMPLE_DIR="$OUTPUT_DIR/$SAMPLE_LABEL"

REDIS_HOST="${REDIS_HOST:-localhost}"
REDIS_PORT="${REDIS_PORT:-6379}"
REDIS_PASSWORD="${REDIS_PASSWORD:-}"
REDIS_CONTAINER="${REDIS_CONTAINER:-}"
REDIS_LATENCY_THRESHOLD_MS="${REDIS_LATENCY_THRESHOLD_MS:-100}"

mkdir -p "$SAMPLE_DIR"

run_redis() {
    if [[ -n "$REDIS_CONTAINER" ]]; then
        docker exec "$REDIS_CONTAINER" redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ${REDIS_PASSWORD:+-a "$REDIS_PASSWORD"} "$@"
    else
        redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ${REDIS_PASSWORD:+-a "$REDIS_PASSWORD"} "$@"
    fi
}

run_redis_file() {
    local name="$1"
    shift
    {
        echo "# sample_label: $SAMPLE_LABEL"
        echo "# generated_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo
        run_redis "$@"
    } >"$SAMPLE_DIR/$name"
}

run_redis CONFIG SET latency-monitor-threshold "$REDIS_LATENCY_THRESHOLD_MS" >"$SAMPLE_DIR/00_config_set_latency_monitor.txt" || true
run_redis_file "01_info_server.txt" INFO server
run_redis_file "02_info_stats.txt" INFO stats
run_redis_file "03_info_commandstats.txt" INFO commandstats
run_redis_file "04_info_memory.txt" INFO memory
run_redis_file "05_info_clients.txt" INFO clients
run_redis_file "06_info_keyspace.txt" INFO keyspace
run_redis_file "10_slowlog_len.txt" SLOWLOG LEN
run_redis_file "11_slowlog_get_128.txt" SLOWLOG GET 128
run_redis_file "12_latency_latest.txt" LATENCY LATEST
run_redis_file "13_latency_doctor.txt" LATENCY DOCTOR
run_redis_file "14_client_list.txt" CLIENT LIST
run_redis_file "15_memory_stats.txt" MEMORY STATS
run_redis_file "16_memory_doctor.txt" MEMORY DOCTOR

cat >"$SAMPLE_DIR/README.txt" <<EOF
Redis observability snapshot
sample_label=$SAMPLE_LABEL
generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
redis_host=$REDIS_HOST
redis_port=$REDIS_PORT
redis_container=${REDIS_CONTAINER:-none}
latency_monitor_threshold_ms=$REDIS_LATENCY_THRESHOLD_MS
EOF

echo "Redis snapshot written to: $SAMPLE_DIR"
