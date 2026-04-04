#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TEST_THREADS="${TEST_THREADS:-4}"
TEST_RETRIES="${TEST_RETRIES:-2}"
NEXTEST_PROFILE_NAME="${NEXTEST_PROFILE_NAME:-ci}"

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

  while [ "$attempt" -le "$max_attempts" ]; do
    echo "cargo test attempt ${attempt}/${max_attempts}"

    if cargo test --all-features --locked -- --shuffle --test-threads="$TEST_THREADS"; then
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
