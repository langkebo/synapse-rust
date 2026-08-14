#!/usr/bin/env bash
# Generate lcov.info for synapse-rust with full source-file coverage.
#
# Background:
#   The previous command `cargo tarpaulin --ignore-tests --out Lcov ...` was
#   missing the `--features test-utils` flag, so the test binaries declared in
#   Cargo.toml with `required-features = ["test-utils"]` (tests/integration,
#   tests/unit, tests/e2e) were never built or run. As a result lcov.info only
#   contained 173/400 source files and the coverage report was misleading.
#
# This script:
#   1. Enables --workspace to cover all crates (not just the root package)
#   2. Enables --all-features so every source-level feature is compiled
#   3. Explicitly adds --features test-utils to build the integration/unit/e2e
#      test binaries (defensive: --all-features should already cover this)
#   4. Uses --include-tests to actually execute the integration test binaries
#   5. Writes lcov format to coverage/lcov.info for analyze_coverage.py
#
# Usage:
#   bash scripts/run_local_coverage.sh            # use defaults
#   TEST_THREADS=4 bash scripts/run_local_coverage.sh
#
# Environment variables:
#   DATABASE_URL       - Postgres DSN (default: localhost:15432/synapse_test)
#   TEST_DATABASE_URL  - Postgres DSN for tests (default: same as DATABASE_URL)
#   TEST_THREADS       - parallelism passed to the test runner (default: 1)
#   FAIL_UNDER         - coverage floor; non-zero exit if below (default: 0)

set -euo pipefail

cd "$(dirname "$0")/.."

# --- Defaults ---------------------------------------------------------------
export DATABASE_URL="${DATABASE_URL:-postgresql://synapse:synapse@localhost:15432/synapse_test}"
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
TEST_THREADS="${TEST_THREADS:-1}"
FAIL_UNDER="${FAIL_UNDER:-0}"
OUTPUT_DIR="coverage"

mkdir -p "$OUTPUT_DIR"

echo "==> Generating lcov.info with full workspace + test-utils coverage"
echo "    DATABASE_URL     = $DATABASE_URL"
echo "    TEST_THREADS     = $TEST_THREADS"
echo "    Output directory = $OUTPUT_DIR/"

# Tarpaulin flag rationale:
#   --workspace           Cover every crate in the workspace (not just root)
#   --features            Minimal set: test-utils (test binaries) + features
#                         required by integration/unit/e2e test targets
#   --include-tests       Run integration test targets (tests/*) in addition
#                         to lib/bin unit tests
#   --implicit-test-threads  Don't let tarpaulin inject its own `--test-threads`
#                         (which defaults to #CPUs); instead honor RUST_TEST_THREADS
#                         so DB integration tests run at a safe concurrency. CI's
#                         GitHub runner (2 cores) is fine at default, but local
#                         8-16 core machines cause DB contention / data conflicts
#                         when tarpaulin runs 1 test thread per CPU.
#   --out Lcov            Emit coverage/lcov.info for analyze_coverage.py
#   --locked              Respect Cargo.lock (CI parity)
export RUST_TEST_THREADS="${TEST_THREADS}"
cargo tarpaulin \
    --workspace \
    --features "test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications" \
    --include-tests \
    --implicit-test-threads \
    --no-fail-fast \
    --out Lcov \
    --output-dir "$OUTPUT_DIR" \
    ${FAIL_UNDER:+--fail-under "$FAIL_UNDER"} \
    --locked

echo "==> Coverage report written to $OUTPUT_DIR/lcov.info"
echo
echo "Analyze with:"
echo "    python3 scripts/analyze_coverage.py"
