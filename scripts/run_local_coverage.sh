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
#   --all-features        Compile every source-level feature so all .rs files
#                         are visible to the instrumenter
#   --features test-utils Build the integration/unit/e2e test binaries declared
#                         with `required-features = ["test-utils"]` in Cargo.toml
#   --include-tests       Run integration test targets (tests/*) in addition
#                         to lib/bin unit tests
#   --out Lcov            Emit coverage/lcov.info for analyze_coverage.py
#   --locked              Respect Cargo.lock (CI parity)
# Note: `--test-threads` is intentionally NOT passed. The installed tarpaulin
#   version doesn't accept it as a top-level flag, and passing it via `--`
#   causes "Option 'test-threads' given more than once" when --include-tests
#   is enabled (tarpaulin forwards its own copy to libtest). Tarpaulin's
#   default parallelism (1 thread per CPU) is acceptable for coverage runs.
cargo tarpaulin \
    --workspace \
    --all-features \
    --features test-utils \
    --include-tests \
    --out Lcov \
    --output-dir "$OUTPUT_DIR" \
    ${FAIL_UNDER:+--fail-under "$FAIL_UNDER"} \
    --locked

echo "==> Coverage report written to $OUTPUT_DIR/lcov.info"
echo
echo "Analyze with:"
echo "    python3 scripts/analyze_coverage.py"
