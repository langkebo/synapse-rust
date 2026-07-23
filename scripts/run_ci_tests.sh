#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# B.3: raised from 2 → 8 (DB pool max=40 + template-schema cloning semaphore
# both support the extra concurrency; cuts integration-test wall time ~30-40%).
TEST_THREADS="${TEST_THREADS:-8}"
TEST_RETRIES="${TEST_RETRIES:-2}"
NEXTEST_PROFILE_NAME="${NEXTEST_PROFILE_NAME:-ci}"
RUN_PERF_SMOKE="${RUN_PERF_SMOKE:-0}"
PERF_SMOKE_LOG="${PERF_SMOKE_LOG:-artifacts/perf_smoke.log}"
RUN_PERF_SOAK="${RUN_PERF_SOAK:-0}"
PERF_SOAK_BASE_URL="${PERF_SOAK_BASE_URL:-http://localhost:8008}"
PERF_SOAK_RESULTS_DIR="${PERF_SOAK_RESULTS_DIR:-artifacts/perf_soak}"
PERF_SOAK_VUS="${PERF_SOAK_VUS:-40}"
PERF_SOAK_DURATION="${PERF_SOAK_DURATION:-24h}"

# ─── Test target selection ──────────────────────────────────────
# Usage: run_ci_tests.sh [--lib] [--unit] [--integration]
#   --lib          Run library unit tests (src/ inline tests)
#   --unit         Run unit test target (tests/unit/, no DB required)
#   --integration  Run integration test target (tests/integration/, requires DB)
# No flags: runs all three targets (full CI suite).

RUN_LIB=0
RUN_UNIT=0
RUN_INTEGRATION=0

for arg in "$@"; do
    case "$arg" in
        --lib) RUN_LIB=1 ;;
        --unit) RUN_UNIT=1 ;;
        --integration) RUN_INTEGRATION=1 ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 1
            ;;
    esac
done

# Default: run everything if no target specified
if [ "$RUN_LIB" -eq 0 ] && [ "$RUN_UNIT" -eq 0 ] && [ "$RUN_INTEGRATION" -eq 0 ]; then
    RUN_LIB=1
    RUN_UNIT=1
    RUN_INTEGRATION=1
fi

run_perf_smoke_gate() {
    mkdir -p "$(dirname "$PERF_SMOKE_LOG")"
    cargo test --test performance_manual --features performance-tests --locked -- --ignored --nocapture \
        2>&1 | tee "$PERF_SMOKE_LOG"

    python3 - "$PERF_SMOKE_LOG" <<'PY'
import json
import pathlib
import sys

log_path = pathlib.Path(sys.argv[1])
results = {}
for line in log_path.read_text().splitlines():
    marker = "PERF_SMOKE_JSON="
    if marker not in line:
        continue
    payload = json.loads(line.split(marker, 1)[1].strip())
    results[payload["name"]] = payload

expected = {
    "sliding_sync_poc_load_smoke": {
        "other": ("<=", 0),
        "ok": (">=", 1),
        "p95_ms": ("<=", 1500),
        "p99_ms": ("<=", 3000),
        "limited_ratio_percent": ("<=", 95.0),
    },
    "beacon_hot_room_backpressure_load_smoke": {
        "other": ("<=", 0),
        "ok": (">=", 1),
        "limited": (">=", 1),
        "p95_ms": ("<=", 3000),
        "p99_ms": ("<=", 5000),
    },
}

missing = [name for name in expected if name not in results]
if missing:
    raise SystemExit(f"Missing PERF_SMOKE_JSON output for: {', '.join(missing)}")

def check(op, actual, expected_value):
    if op == "<=":
        return actual <= expected_value
    if op == ">=":
        return actual >= expected_value
    raise ValueError(op)

failures = []
for name, constraints in expected.items():
    payload = results[name]
    for field, (op, expected_value) in constraints.items():
        actual = payload.get(field)
        if actual is None or not check(op, actual, expected_value):
            failures.append(
                f"{name}: expected {field} {op} {expected_value}, got {actual}"
            )

if failures:
    raise SystemExit("\n".join(failures))

print("Performance smoke gate passed")
PY
}

run_perf_soak_gate() {
    mkdir -p "$PERF_SOAK_RESULTS_DIR"
    BASE_URL="$PERF_SOAK_BASE_URL" \
        RESULTS_DIR="$PERF_SOAK_RESULTS_DIR" \
        SOAK_VUS="$PERF_SOAK_VUS" \
        SOAK_DURATION="$PERF_SOAK_DURATION" \
        "$ROOT_DIR/scripts/test/perf/run_tests.sh" soak
}

# Check whether PostgreSQL and Redis are reachable before running integration tests.
# Reads DATABASE_URL and REDIS_URL from the environment (same vars used by the server).
check_infra_ready() {
    local missing=""

    # PostgreSQL check: parse host/port from DATABASE_URL if set, else try defaults
    local db_host="localhost"
    local db_port="${DB_PORT:-5432}"
    if [ -n "${DATABASE_URL:-}" ]; then
        # Extract host:port from postgres://user:pass@host:port/db
        db_host=$(echo "$DATABASE_URL" | sed -n 's|.*@\([^:/]*\).*|\1|p')
        db_port=$(echo "$DATABASE_URL" | sed -n 's|.*:\([0-9]\+\)/.*|\1|p')
        [ -z "$db_port" ] && db_port=5432
    fi

    if command -v pg_isready >/dev/null 2>&1; then
        if ! pg_isready -h "$db_host" -p "$db_port" -t 3 >/dev/null 2>&1; then
            missing="PostgreSQL (${db_host}:${db_port})"
        fi
    elif ! timeout 3 bash -c "echo >/dev/tcp/${db_host}/${db_port}" 2>/dev/null; then
        missing="PostgreSQL (${db_host}:${db_port})"
    fi

    # Redis check: parse host/port from REDIS_URL if set
    local redis_host="localhost"
    local redis_port="${REDIS_PORT:-6379}"
    if [ -n "${REDIS_URL:-}" ]; then
        redis_host=$(echo "$REDIS_URL" | sed -n 's|.*@\([^:/]*\).*|\1|p')
        redis_port=$(echo "$REDIS_URL" | sed -n 's|.*:\([0-9]\+\)[^0-9]*$|\1|p; s|.*:\([0-9]\+\)$|\1|p')
        [ -z "$redis_port" ] && redis_port=6379
    fi

    if command -v redis-cli >/dev/null 2>&1; then
        if ! redis-cli -h "$redis_host" -p "$redis_port" ping >/dev/null 2>&1; then
            missing="${missing:+$missing, }Redis (${redis_host}:${redis_port})"
        fi
    elif ! timeout 3 bash -c "echo >/dev/tcp/${redis_host}/${redis_port}" 2>/dev/null; then
        missing="${missing:+$missing, }Redis (${redis_host}:${redis_port})"
    fi

    if [ -n "$missing" ]; then
        echo "=== Infrastructure not ready: $missing ==="
        echo "Integration tests require PostgreSQL and Redis."
        echo "Start them with:  cd docker && docker compose up -d db redis"
        echo "Then re-run:      TEST_THREADS=${TEST_THREADS} TEST_RETRIES=${TEST_RETRIES} bash scripts/run_ci_tests.sh --integration"
        return 1
    fi
    return 0
}

export RUST_TEST_SHUFFLE="${RUST_TEST_SHUFFLE:-1}"
if [ -z "${RUST_TEST_SHUFFLE_SEED:-}" ]; then
    if [ -n "${GITHUB_RUN_ID:-}" ]; then
        export RUST_TEST_SHUFFLE_SEED="$GITHUB_RUN_ID"
    else
        export RUST_TEST_SHUFFLE_SEED="$(date +%s%N)"
    fi
fi
echo "RUST_TEST_SHUFFLE_SEED=$RUST_TEST_SHUFFLE_SEED"

run_cargo_test_with_retries() {
    local label="$1"
    shift
    local attempt=1
    local max_attempts=$((TEST_RETRIES + 1))

    while [ "$attempt" -le "$max_attempts" ]; do
        echo "[${label}] cargo test attempt ${attempt}/${max_attempts}"

        if cargo test "$@"; then
            return 0
        fi

        if [ "$attempt" -eq "$max_attempts" ]; then
            return 1
        fi

        attempt=$((attempt + 1))
    done
}

if cargo nextest --version >/dev/null 2>&1; then
    export NEXTEST_RETRIES="${NEXTEST_RETRIES:-$TEST_RETRIES}"
    nextest_base=(cargo nextest run --profile "$NEXTEST_PROFILE_NAME" --locked --test-threads "$TEST_THREADS")

    echo "=== Test matrix ==="
    echo "  --lib:          $RUN_LIB"
    echo "  --unit:         $RUN_UNIT"
    echo "  --integration:  $RUN_INTEGRATION"
    echo ""

    if [ "$RUN_LIB" -eq 1 ]; then
        echo ">>> Running library unit tests (--lib) ..."
        "${nextest_base[@]}" --lib --all-features
    fi

    if [ "$RUN_UNIT" -eq 1 ]; then
        echo ">>> Running unit test target (--test unit) ..."
        "${nextest_base[@]}" --test unit --features test-utils
    fi

    if [ "$RUN_INTEGRATION" -eq 1 ]; then
        if check_infra_ready; then
            echo ">>> Running integration test target (--test integration) ..."
            "${nextest_base[@]}" --test integration --all-features

            # P4-2: Snapshot gate — fail if any insta snapshot drifted or is missing.
            # Local devs run `cargo insta test --review` to accept new snapshots;
            # CI runs in non-interactive --check mode and fails on snapshot drift.
            if cargo insta --version >/dev/null 2>&1; then
                echo ">>> Running insta snapshot gate (--check) ..."
                if ! cargo insta test --check --test-runner nextest -- --all-features --locked --test-threads "$TEST_THREADS"; then
                    echo "❌ Snapshot drift detected. Run locally:"
                    echo "   cargo insta test --review"
                    exit 1
                fi
                echo "✅ All insta snapshots are up to date."
            else
                echo "⚠️  cargo-insta not installed; skipping snapshot gate. Install with: cargo install cargo-insta"
            fi
        else
            echo ">>> SKIPPED: Integration tests require PostgreSQL + Redis."
        fi
    fi
else
    echo "Using cargo test fallback (no nextest detected)"
    if [ "$RUN_LIB" -eq 1 ]; then
        echo ">>> Running library unit tests (--lib) ..."
        run_cargo_test_with_retries "lib" --lib --all-features --locked -- --test-threads="$TEST_THREADS"
    fi
    if [ "$RUN_UNIT" -eq 1 ]; then
        echo ">>> Running unit test target (--test unit) ..."
        run_cargo_test_with_retries "unit" --test unit --features test-utils --locked -- --test-threads="$TEST_THREADS"
    fi
    if [ "$RUN_INTEGRATION" -eq 1 ]; then
        if check_infra_ready; then
            echo ">>> Running integration test target (--test integration) ..."
            run_cargo_test_with_retries "integration" --test integration --all-features --locked -- --test-threads="$TEST_THREADS"

            # P4-2: Snapshot gate (cargo test fallback path).
            if cargo insta --version >/dev/null 2>&1; then
                echo ">>> Running insta snapshot gate (INSTA_UPDATE=no, cargo test runner) ..."
                if ! INSTA_UPDATE=no cargo test --test integration --all-features --locked -- --test-threads="$TEST_THREADS"; then
                    echo "❌ Snapshot drift detected. Run locally: cargo insta test --review"
                    exit 1
                fi
                echo "✅ All insta snapshots are up to date."
            fi
        else
            echo ">>> SKIPPED: Integration tests require PostgreSQL + Redis."
        fi
    fi
fi

if [ "$RUN_PERF_SMOKE" = "1" ]; then
    run_perf_smoke_gate
fi

if [ "$RUN_PERF_SOAK" = "1" ]; then
    run_perf_soak_gate
fi
