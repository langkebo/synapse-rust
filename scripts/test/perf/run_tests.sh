#!/usr/bin/env bash
# Performance Test Scripts
#分层压测脚本

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${BASE_URL:-http://localhost:28008}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-Admin@123}"
RESULTS_DIR="${RESULTS_DIR:-$SCRIPT_DIR/results}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
SOAK_VUS="${SOAK_VUS:-40}"
SOAK_DURATION="${SOAK_DURATION:-24h}"

mkdir -p "$RESULTS_DIR"

run_smoke_test() {
    echo "Running Smoke Test (10 concurrent users)..."
    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 10 \
        --duration 30s \
        --summary-export "${RESULTS_DIR}/smoke_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js"
}

run_baseline_test() {
    echo "Running Baseline Test (50 concurrent users)..."
    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 50 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/baseline_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js"
}

run_stress_test() {
    echo "Running Stress Test (100 concurrent users)..."
    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 100 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/stress_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js"
}

run_peak_test() {
    echo "Running Peak Test (200 concurrent users)..."
    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 200 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/peak_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js"
}

run_soak_test() {
    echo "Running Soak Test (${SOAK_VUS} concurrent users for ${SOAK_DURATION})..."
    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus "$SOAK_VUS" \
        --duration "$SOAK_DURATION" \
        --summary-export "${RESULTS_DIR}/soak_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js"
}

generate_report() {
    local scenarios=("$@")
    if [ "${#scenarios[@]}" -eq 0 ]; then
        scenarios=(smoke baseline stress peak)
    fi
    echo "Generating Performance Report..."
    "$PYTHON_BIN" "$SCRIPT_DIR/guardrail.py" \
        --results-dir "$RESULTS_DIR" \
        --base-url "$BASE_URL" \
        --scenarios "${scenarios[@]}" \
        --fail-on-breach
    echo "Report generated: ${RESULTS_DIR}/performance_guardrail_report.md"
}

case "${1:-all}" in
    smoke)
        run_smoke_test
        ;;
    baseline)
        run_baseline_test
        ;;
    stress)
        run_stress_test
        ;;
    peak)
        run_peak_test
        ;;
    soak)
        run_soak_test
        generate_report soak
        ;;
    all)
        run_smoke_test
        run_baseline_test
        run_stress_test
        run_peak_test
        generate_report smoke baseline stress peak
        ;;
    report)
        generate_report
        ;;
    *)
        echo "Usage: $0 {smoke|baseline|stress|peak|soak|all|report}"
        exit 1
        ;;
esac
