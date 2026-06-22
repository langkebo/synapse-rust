#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

RUST_TEST_THREADS="${RUST_TEST_THREADS:-1}"
TEST_ISOLATED_SCHEMAS="${TEST_ISOLATED_SCHEMAS:-1}"
COMMON_ARGS=(--locked --features test-utils --test integration)

log() {
    printf '[e2ee-observability-gate] %s\n' "$*"
}

run_suite() {
    local label="$1"
    local filter="$2"
    local match_mode="${3:-substring}"

    log "Running ${label} (${filter}; ${match_mode})"
    if [ "$match_mode" = "exact" ]; then
        TEST_ISOLATED_SCHEMAS="$TEST_ISOLATED_SCHEMAS" \
            cargo test "${COMMON_ARGS[@]}" "$filter" -- --exact --nocapture --test-threads="$RUST_TEST_THREADS"
    else
        TEST_ISOLATED_SCHEMAS="$TEST_ISOLATED_SCHEMAS" \
            cargo test "${COMMON_ARGS[@]}" "$filter" -- --nocapture --test-threads="$RUST_TEST_THREADS"
    fi
}

log "Starting /keys/changes + /sync + sliding-sync observability gate"
run_suite "/keys/changes shared-room regression" "api_e2ee_tests::test_key_changes_allows_users_with_shared_rooms" "exact"
run_suite "/keys/changes cross-signing regression" "api_e2ee_tests::test_key_changes_exposes_cross_signing_updates_for_shared_users" "exact"
run_suite "/keys/changes non-shared regression" "api_e2ee_tests::test_key_changes_filters_users_without_shared_rooms" "exact"
run_suite "classic /sync device_lists composite gate" "test_sync_device_lists_"
run_suite "sliding-sync e2ee composite gate" "sliding_sync_extensions_e2ee_"
log "All observability suites passed"
