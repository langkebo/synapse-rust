#!/usr/bin/env bash
# Performance Test Scripts
#分层压测脚本

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${BASE_URL:-http://localhost:8008}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-Admin@123}"
RESULTS_DIR="${RESULTS_DIR:-$SCRIPT_DIR/results}"

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

generate_report() {
    echo "Generating Performance Report..."
    local report_file="${RESULTS_DIR}/performance_report_$(date +%Y%m%d_%H%M%S).md"

    cat > "$report_file" << 'EOF'
# Performance Test Report

## Test Configuration

| Parameter | Value |
|-----------|-------|
| Base URL | {BASE_URL} |
| Test Date | {TEST_DATE} |

## Smoke Test (10 VUs)

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Login P95 | < 500ms | TBD | TBD |
| CreateRoom P95 | < 800ms | TBD | TBD |
| SendMessage P95 | < 600ms | TBD | TBD |
| Sync P95 | < 1000ms | TBD | TBD |
| RoomSummary P95 | < 500ms | TBD | TBD |

## Baseline Test (50 VUs)

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Login P95 | < 500ms | TBD | TBD |
| CreateRoom P95 | < 800ms | TBD | TBD |
| SendMessage P95 | < 600ms | TBD | TBD |
| Sync P95 | < 1000ms | TBD | TBD |
| RoomSummary P95 | < 500ms | TBD | TBD |

## Stress Test (100 VUs)

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Login P95 | < 600ms | TBD | TBD |
| CreateRoom P95 | < 1000ms | TBD | TBD |
| SendMessage P95 | < 800ms | TBD | TBD |
| Sync P95 | < 1200ms | TBD | TBD |
| RoomSummary P95 | < 600ms | TBD | TBD |

## Peak Test (200 VUs)

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Login P95 | < 600ms | TBD | TBD |
| CreateRoom P95 | < 1000ms | TBD | TBD |
| SendMessage P95 | < 800ms | TBD | TBD |
| Sync P95 | < 1200ms | TBD | TBD |
| RoomSummary P95 | < 600ms | TBD | TBD |

## Conclusion

TBD

EOF

    sed -i "s/{BASE_URL}/$BASE_URL/g" "$report_file"
    sed -i "s/{TEST_DATE}/$(date)/g" "$report_file"

    echo "Report generated: $report_file"
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
    all)
        run_smoke_test
        run_baseline_test
        run_stress_test
        run_peak_test
        generate_report
        ;;
    report)
        generate_report
        ;;
    *)
        echo "Usage: $0 {smoke|baseline|stress|peak|all|report}"
        exit 1
        ;;
esac
