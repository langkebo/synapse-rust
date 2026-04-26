#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/artifacts/perf}"
BASE_URL="${BASE_URL:-http://localhost:28008}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-Admin@123}"
SUMMARY_EXPORT="$OUTPUT_DIR/k6-summary.json"
LOG_EXPORT="$OUTPUT_DIR/k6-run.log"

mkdir -p "$OUTPUT_DIR"

if ! command -v k6 >/dev/null 2>&1; then
  echo "k6 is required"
  exit 1
fi

BASE_URL="$BASE_URL" \
ADMIN_USER="$ADMIN_USER" \
ADMIN_PASS="$ADMIN_PASS" \
k6 run \
  --summary-export "$SUMMARY_EXPORT" \
  "$ROOT_DIR/docker/k6_test.js" | tee "$LOG_EXPORT"
