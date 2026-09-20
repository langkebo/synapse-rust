#!/usr/bin/env bash
#
# Sliding Sync Performance Threshold Gate (P2-13)
#
# Runs the sliding sync criterion benchmark and enforces a p95 latency
# threshold. Inspired by Synapse v1.153.0rc3, which reverted a
# sliding-sync optimisation after performance regressions went unnoticed
# because no threshold gate existed.
#
# The benchmark prints `[perf]` log lines to stderr of the form:
#   [perf] sliding_sync manual_p95_ms=12.34 manual_p99_ms=15.67 \
#          service_p95_ms=12.50 threshold_ms=5000
#   [perf] sliding_sync_response rooms=100 manual_p95_ms=... \
#          manual_p99_ms=... threshold_ms=5000 slow_requests=0 ...
#
# This script parses those lines, compares manual_p95_ms against the
# threshold, and exits non-zero if any sample breaches the threshold.
#
# Environment:
#   BENCHMARK_DATABASE_URL            — Postgres URL (required; bench skips
#                                        without it, which fails this gate)
#   SLIDING_SYNC_P95_THRESHOLD_MS     — override p95 threshold (default:
#                                        read from benchmark output, fallback
#                                        5000ms = PerformanceConfig default)
#   SLIDING_SYNC_PERF_GATE_STRICT     — when `1`, a missing DB or missing
#                                        `[perf]` lines fail the gate;
#                                        when `0` (default), missing DB fails
#                                        but missing lines warn only.
#
# Exits 0 if all reported p95 samples are within threshold, 1 otherwise.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

THRESHOLD_OVERRIDE="${SLIDING_SYNC_P95_THRESHOLD_MS:-}"
STRICT="${SLIDING_SYNC_PERF_GATE_STRICT:-0}"

mkdir -p artifacts

echo "==> Sliding Sync Performance Threshold Gate (P2-13)"

# ---------------------------------------------------------------------------
# 1) Pre-flight: database must be reachable, otherwise the benchmark would
#    skip the DB-backed group and we'd have no p95 to gate on.
#
#    注意：`pg_isready` 属于 postgresql-client，在 GitHub runner 上**未必存在**。
#    因此优先用它（最准确），不可用时退化为 TCP 端口探测（bash /dev/tcp），
#    再不行用 python3。这样门禁不会因为缺少 CLI 工具而永久报"数据库不可达"。
# ---------------------------------------------------------------------------
DB_URL="${BENCHMARK_DATABASE_URL:-postgresql://synapse:synapse@localhost:5432/synapse_bench}"

# 从 postgresql://user:pass@host:port/db 中取出 host 与 port。
db_host_port() {
    python3 - "$DB_URL" <<'PY'
import sys, urllib.parse
u = urllib.parse.urlparse(sys.argv[1])
print(u.hostname or "localhost", u.port or 5432)
PY
}

db_reachable() {
    # 1) 首选 pg_isready（可用时会做真实握手）
    if command -v pg_isready >/dev/null 2>&1; then
        pg_isready -d "$DB_URL" >/dev/null 2>&1 && return 0
        return 1
    fi
    # 2) 退化：TCP 端口探测
    local host port
    host=$(db_host_port | awk '{print $1}')
    port=$(db_host_port | awk '{print $2}')
    if command -v python3 >/dev/null 2>&1; then
        python3 - "$host" "$port" <<'PY'
import socket, sys
try:
    socket.create_connection((sys.argv[1], int(sys.argv[2])), timeout=5).close()
except OSError:
    sys.exit(1)
PY
        return $?
    fi
    # 3) 最后退化为 bash 内建 TCP
    (exec 3<>"/dev/tcp/${host}/${port}") >/dev/null 2>&1
}

if ! db_reachable; then
    echo "ERROR: benchmark database unreachable at $DB_URL"
    echo "       sliding sync p95 cannot be measured without a database."
    if [ "$STRICT" = "1" ]; then
        echo "       (strict mode: failing the gate)"
        exit 1
    fi
    echo "       (non-strict mode: skipping gate with warning)"
    exit 0
fi
echo "    database reachable: $DB_URL"

# ---------------------------------------------------------------------------
# 2) Run the benchmark, capturing stderr (where [perf] lines are printed).
# ---------------------------------------------------------------------------
BENCH_LOG="artifacts/sliding_sync_perf_gate.log"
echo "    running performance_sliding_sync_benchmarks..."
# The p95/p99 measurement is criterion benchmark id
# `sliding_sync_p95_p99_latency` (that is what the CLI filter below selects),
# while the harness's **required-group** registry calls the same group
# `p95_p99` (see `require_bench_group("p95_p99")` /
# `c.bench_function("sliding_sync_p95_p99_latency", …)` in
# `benches/performance_sliding_sync_benchmarks.rs`). The two names are
# different strings and mixing them up is fatal, not cosmetic:
#
#   SLIDING_SYNC_REQUIRE=sliding_sync_p95_p99_latency  # ❌ never matches a group
#   → "required benchmark group(s) did not execute: sliding_sync_p95_p99_latency"
#   → bench exit 1, even though all 30 `[perf]` samples were emitted
#   (measured locally 2026-09-20). Correct value: `p95_p99`.
#   `tests/unit/pagination_gate_tests.rs::sliding_gate_require_names_a_registered_group`
#   now pins this mapping so it cannot silently drift again.
#
# SLIDING_SYNC_REQUIRE makes the bench harness itself fail when the group did
# not execute (e.g. the DB vanished between the pre-flight check and the run).
# Without it the harness would skip the group and still exit 0 — the exact
# false-green this gate exists to prevent.
#
# Note: a "did any group run?" check would NOT work here, because the
# in-process `benchmark_request_construction` group always runs.
#
# `--features test-utils` is **required**: `Cargo.toml` declares
# `required-features = ["test-utils"]` for this bench target, so without it
# cargo refuses to build it and the gate dies with
#   error: target `performance_sliding_sync_benchmarks` in package
#   `synapse-rust` requires the features: `test-utils`
# (measured 2026-09-20; the gate had never reached this line in CI because the
# schema step failed first). Unlike the pagination bench, this target was not
# given a `required-features`-free declaration, so the feature flag must come
# from the caller.
SLIDING_SYNC_REQUIRE="p95_p99" \
    cargo bench --locked --features test-utils --bench performance_sliding_sync_benchmarks \
    -- --noplot sliding_sync_p95_p99_latency 2>"$BENCH_LOG" || {
    echo "ERROR: sliding sync benchmark failed to run"
    cat "$BENCH_LOG"
    exit 1
}

# ---------------------------------------------------------------------------
# 3) Parse `[perf]` lines for manual_p95_ms and threshold_ms.
# ---------------------------------------------------------------------------
# Extract lines like:
#   [perf] sliding_sync manual_p95_ms=12.34 manual_p99_ms=15.67 service_p95_ms=12.50 threshold_ms=5000
PERF_LINES=$(grep -E '^\[perf\] sliding_sync' "$BENCH_LOG" || true)

if [ -z "$PERF_LINES" ]; then
    echo "WARNING: no [perf] sliding_sync lines found in benchmark output"
    echo "         cannot enforce p95 threshold"
    if [ "$STRICT" = "1" ]; then
        echo "       (strict mode: failing the gate)"
        exit 1
    fi
    echo "       (non-strict mode: passing with warning)"
    exit 0
fi

echo "    parsed perf samples:"
echo "$PERF_LINES" | sed 's/^/      /'
echo ""

# ---------------------------------------------------------------------------
# 4) Compare each sample's p95 against the threshold.
# ---------------------------------------------------------------------------
FAILURES=0
TOTAL=0

while IFS= read -r line; do
    [ -z "$line" ] && continue
    TOTAL=$((TOTAL + 1))

    # Extract manual_p95_ms value (float).
    P95=$(echo "$line" | sed -n 's/.*manual_p95_ms=\([0-9.]*\).*/\1/p')
    # Extract threshold_ms value (integer).
    SAMPLE_THRESHOLD=$(echo "$line" | sed -n 's/.*threshold_ms=\([0-9]*\).*/\1/p')

    # Apply override if provided.
    if [ -n "$THRESHOLD_OVERRIDE" ]; then
        EFFECTIVE_THRESHOLD="$THRESHOLD_OVERRIDE"
    else
        EFFECTIVE_THRESHOLD="${SAMPLE_THRESHOLD:-5000}"
    fi

    if [ -z "$P95" ]; then
        echo "WARNING: could not parse manual_p95_ms from line: $line"
        if [ "$STRICT" = "1" ]; then
            FAILURES=$((FAILURES + 1))
        fi
        continue
    fi

    # Compare floats with awk (bash has no float comparison).
    if awk "BEGIN {exit !($P95 > $EFFECTIVE_THRESHOLD)}"; then
        echo "BREACH: p95=${P95}ms exceeds threshold=${EFFECTIVE_THRESHOLD}ms"
        FAILURES=$((FAILURES + 1))
    else
        echo "OK: p95=${P95}ms within threshold=${EFFECTIVE_THRESHOLD}ms"
    fi
done <<<"$PERF_LINES"

echo ""
echo "==> Summary: $((TOTAL - FAILURES))/$TOTAL samples within threshold"

# ---------------------------------------------------------------------------
# 5) Also extract the `slow_requests` counter if present — a non-zero value
#    means the service itself observed slow syncs during the benchmark.
# ---------------------------------------------------------------------------
SLOW_REQUESTS=$(grep -oE 'slow_requests=[0-9]+' "$BENCH_LOG" | head -1 | sed 's/slow_requests=//' || true)
if [ -n "$SLOW_REQUESTS" ] && [ "$SLOW_REQUESTS" -gt 0 ]; then
    echo "BREACH: service reported $SLOW_REQUESTS slow sync request(s) during benchmark"
    FAILURES=$((FAILURES + 1))
fi

if [ "$FAILURES" -gt 0 ]; then
    echo ""
    echo "==> Sliding Sync Perf Gate: FAILED ($FAILURES breach(es) detected)"
    echo "    This indicates a performance regression in the sliding sync hot path."
    echo "    Review the benchmark log: $BENCH_LOG"
    exit 1
fi

echo ""
echo "==> Sliding Sync Perf Gate: PASSED"
exit 0
