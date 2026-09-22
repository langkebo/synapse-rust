#!/usr/bin/env bash
# Performance Test Scripts
#分层压测脚本

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${BASE_URL:-http://localhost:8008}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-Admin@123}"
RESULTS_DIR="${RESULTS_DIR:-$SCRIPT_DIR/results}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
SOAK_VUS="${SOAK_VUS:-40}"
SOAK_DURATION="${SOAK_DURATION:-24h}"

mkdir -p "$RESULTS_DIR"

run_smoke_test() {
    echo "========================================"
    echo "  SMOKE TEST (10 concurrent users, 30s)"
    echo "  Target: $BASE_URL"
    echo "========================================"

    # 检查目标是否可达
    if ! curl -sf --max-time 5 "$BASE_URL/_matrix/static/" >/dev/null 2>&1; then
        echo "⚠️  警告: 目标服务器 $BASE_URL 未响应静态端点"
        echo "    继续运行测试，但结果可能无效"
    fi

    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 10 \
        --duration 30s \
        --summary-export "${RESULTS_DIR}/smoke_results.json" \
        --out "json=${RESULTS_DIR}/smoke_details.json" \
        "$SCRIPT_DIR/api_matrix_core.js" 2>&1 | tee "${RESULTS_DIR}/smoke_output.log"

    local exit_code=${PIPESTATUS[0]}
    if [ $exit_code -ne 0 ]; then
        echo "❌ SMOKE TEST FAILED (exit code: $exit_code)"
        echo "   检查日志: ${RESULTS_DIR}/smoke_output.log"
        echo "   可能原因: 服务器未启动 / 网络不通 / 认证失败"
    else
        echo "✅ SMOKE TEST PASSED"
    fi
    return $exit_code
}

run_baseline_test() {
    echo "========================================"
    echo "  BASELINE TEST (50 users, 60s)"
    echo "  Target: $BASE_URL"
    echo "========================================"

    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 50 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/baseline_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js" 2>&1 | tee "${RESULTS_DIR}/baseline_output.log"

    local exit_code=${PIPESTATUS[0]}
    [ $exit_code -ne 0 ] && echo "❌ BASELINE TEST FAILED" || echo "✅ BASELINE TEST PASSED"
    return $exit_code
}

run_stress_test() {
    echo "========================================"
    echo "  STRESS TEST (100 users, 60s)"
    echo "  Target: $BASE_URL"
    echo "========================================"

    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 100 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/stress_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js" 2>&1 | tee "${RESULTS_DIR}/stress_output.log"

    local exit_code=${PIPESTATUS[0]}
    [ $exit_code -ne 0 ] && echo "❌ STRESS TEST FAILED" || echo "✅ STRESS TEST PASSED"
    return $exit_code
}

run_peak_test() {
    echo "========================================"
    echo "  PEAK TEST (200 users, 60s)"
    echo "  Target: $BASE_URL"
    echo "========================================"

    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 200 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/peak_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js" 2>&1 | tee "${RESULTS_DIR}/peak_output.log"

    local exit_code=${PIPESTATUS[0]}
    [ $exit_code -ne 0 ] && echo "❌ PEAK TEST FAILED" || echo "✅ PEAK TEST PASSED"
    return $exit_code
}

run_friend_test() {
    echo "Running Friend Search/List Test (100 concurrent users)..."
    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus 100 \
        --duration 60s \
        --summary-export "${RESULTS_DIR}/friends_results.json" \
        "$SCRIPT_DIR/friend_search_and_list.js"
}

run_soak_test() {
    echo "========================================"
    echo "  SOAK TEST (${SOAK_VUS} users, ${SOAK_DURATION})"
    echo "  Target: $BASE_URL"
    echo "========================================"

    k6 run \
        --env BASE_URL="$BASE_URL" \
        --env ADMIN_USER="$ADMIN_USER" \
        --env ADMIN_PASS="$ADMIN_PASS" \
        --vus "$SOAK_VUS" \
        --duration "$SOAK_DURATION" \
        --summary-export "${RESULTS_DIR}/soak_results.json" \
        "$SCRIPT_DIR/api_matrix_core.js" 2>&1 | tee "${RESULTS_DIR}/soak_output.log"

    local exit_code=${PIPESTATUS[0]}
    [ $exit_code -ne 0 ] && echo "❌ SOAK TEST FAILED" || echo "✅ SOAK TEST PASSED"
    return $exit_code
}

generate_report() {
    local scenarios=("$@")
    if [ "${#scenarios[@]}" -eq 0 ]; then
        scenarios=(smoke baseline stress peak)
    fi

    echo "========================================"
    echo "  GENERATING PERFORMANCE REPORT"
    echo "  Scenarios: ${scenarios[*]}"
    echo "========================================"

    "$PYTHON_BIN" "$SCRIPT_DIR/guardrail.py" \
        --results-dir "$RESULTS_DIR" \
        --base-url "$BASE_URL" \
        --scenarios "${scenarios[@]}" \
        --fail-on-breach

    local exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "✅ Performance Guardrail Report: PASS"
    else
        echo "❌ Performance Guardrail Report: FAIL"
    fi

    echo "Report: ${RESULTS_DIR}/performance_guardrail_report.md"
    echo ""

    return $exit_code
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
    friends)
        run_friend_test
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
        run_friend_test
        generate_report smoke baseline stress peak friends
        ;;
    report)
        generate_report
        ;;
    *)
        echo "Usage: $0 {smoke|baseline|stress|peak|friends|soak|all|report}"
        echo ""
        echo "Examples:"
        echo "  $0 smoke           # Run smoke test only"
        echo "  $0 all             # Run all tests sequentially"
        echo "  BASE_URL=http://10.0.0.1:8008 $0 smoke   # Custom server"
        exit 1
        ;;
esac
