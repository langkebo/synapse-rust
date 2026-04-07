#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TEST_THREADS="${TEST_THREADS:-2}"
TEST_RETRIES="${TEST_RETRIES:-2}"
NEXTEST_PROFILE_NAME="${NEXTEST_PROFILE_NAME:-ci}"
RUN_PERF_SMOKE="${RUN_PERF_SMOKE:-0}"
PERF_SMOKE_LOG="${PERF_SMOKE_LOG:-artifacts/perf_smoke.log}"
RUN_PERF_SOAK="${RUN_PERF_SOAK:-0}"
PERF_SOAK_BASE_URL="${PERF_SOAK_BASE_URL:-http://localhost:8008}"
PERF_SOAK_RESULTS_DIR="${PERF_SOAK_RESULTS_DIR:-artifacts/perf_soak}"
PERF_SOAK_VUS="${PERF_SOAK_VUS:-40}"
PERF_SOAK_DURATION="${PERF_SOAK_DURATION:-24h}"

run_perf_smoke_gate() {
  mkdir -p "$(dirname "$PERF_SMOKE_LOG")"
  cargo test --test performance_manual --features performance-tests --locked -- --ignored --nocapture \
    2>&1 | tee "$PERF_SMOKE_LOG"

  python3 - <<'PY' "$PERF_SMOKE_LOG"
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
  local attempt=1
  local max_attempts=$((TEST_RETRIES + 1))
  local cargo_test_cmd=(cargo test --all-features --locked -- --test-threads="$TEST_THREADS")

  if rustc -V | grep -q "nightly"; then
    cargo_test_cmd=(cargo test --all-features --locked -Z unstable-options -- --shuffle --test-threads="$TEST_THREADS")
    echo "Using nightly cargo test shuffle fallback"
  else
    echo "Stable toolchain detected; running cargo test fallback without --shuffle"
  fi

  while [ "$attempt" -le "$max_attempts" ]; do
    echo "cargo test attempt ${attempt}/${max_attempts}"

    if "${cargo_test_cmd[@]}"; then
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
  cargo nextest run \
    --profile "$NEXTEST_PROFILE_NAME" \
    --all-features \
    --locked \
    --test-threads "$TEST_THREADS"
else
  run_cargo_test_with_retries
fi

if [ "$RUN_PERF_SMOKE" = "1" ]; then
  run_perf_smoke_gate
fi

if [ "$RUN_PERF_SOAK" = "1" ]; then
  run_perf_soak_gate
fi
