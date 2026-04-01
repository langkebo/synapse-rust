#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${OUT_DIR:-reports/quality}"
RESULTS_DIR="${RESULTS_DIR:-test-results}"
RUN_TESTS="${RUN_TESTS:-1}"
RUN_AUDIT="${RUN_AUDIT:-1}"
RUN_COVERAGE="${RUN_COVERAGE:-0}"

mkdir -p "$OUT_DIR"

run_step() {
  local name="$1"
  shift
  echo "==> $name"
  "$@" 2>&1 | tee "$OUT_DIR/$name.log"
  echo ""
}

tool_available() {
  command -v "$1" >/dev/null 2>&1
}

run_step cargo_fmt cargo fmt -- --check || true
run_step cargo_clippy cargo clippy -- -D warnings || true
run_step text_encoding bash scripts/quality/check_text_encoding.sh || true
if [ "$RUN_TESTS" = "1" ]; then
  run_step cargo_test cargo test --locked || true
fi

if [ "$RUN_AUDIT" = "1" ]; then
  if tool_available cargo-audit; then
    run_step cargo_audit cargo audit || true
  else
    echo "==> cargo_audit"
    echo "cargo-audit not found"
    echo "" | tee "$OUT_DIR/cargo_audit.log" >/dev/null
  fi
fi

if [ "$RUN_COVERAGE" = "1" ]; then
  if tool_available cargo-tarpaulin; then
    run_step cargo_tarpaulin cargo tarpaulin --ignore-tests --out Lcov --output-dir "$OUT_DIR/coverage" --locked -- --test-threads 1 || true
  else
    echo "==> cargo_tarpaulin"
    echo "cargo-tarpaulin not found"
    echo "" | tee "$OUT_DIR/cargo_tarpaulin.log" >/dev/null
  fi
fi

python3 scripts/quality/parse_api_integration_results.py --results-dir "$RESULTS_DIR" --out "$OUT_DIR/api_integration_summary.md" || true
echo "==> done"
echo "outputs:"
echo " - $OUT_DIR/"
