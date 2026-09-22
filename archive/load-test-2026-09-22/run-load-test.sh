#!/bin/bash
# Matrix 负载测试脚本 - 逐步加压
# 用法: ./run-load-test.sh [stages]
#
# 阶段:
#   light   - 100 并发, 10 分钟
#   medium  - 500 并发, 10 分钟
#   heavy   - 1000 并发, 10 分钟
#   stress  - 5000 并发, 10 分钟
#   extreme - 10000 并发, 10 分钟
#   all     - 全部阶段顺序执行

set -e

SYNAPSE_URL="${SYNAPSE_URL:-http://localhost:8008}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOAD_TEST="$SCRIPT_DIR/matrix-load-test.js"
RESULTS_DIR="$SCRIPT_DIR/results"
mkdir -p "$RESULTS_DIR"

stages() {
  echo "=== Matrix Load Test Stages ==="
  echo "  light   - 100 VU × 10min"
  echo "  medium  - 500 VU × 10min"
  echo "  heavy   - 1000 VU × 10min"
  echo "  stress  - 5000 VU × 10min"
  echo "  extreme - 10000 VU × 10min"
  echo "  all     - 全部阶段顺序执行"
  echo "==============================="
}

run_stage() {
  local stage_name=$1
  local vus=$2
  local duration=$3

  echo ""
  echo "╔══════════════════════════════════════════╗"
  echo "║  Stage: $stage_name ($vus VU × $duration)  ║"
  echo "╚══════════════════════════════════════════╝"
  echo ""

  local timestamp=$(date +%Y%m%d_%H%M%S)
  local output_file="$RESULTS_DIR/${stage_name}_${timestamp}.json"
  local log_file="$RESULTS_DIR/${stage_name}_${timestamp}.log"

  k6 run \
    --vus "$vus" \
    --duration "$duration" \
    --out json="$output_file" \
    --env SYNAPSE_URL="$SYNAPSE_URL" \
    --env USERS="$vus" \
    "$LOAD_TEST" 2>&1 | tee "$log_file"

  echo "  Results: $output_file"
  echo "  Log: $log_file"
}

case "${1:-all}" in
  light)
    run_stage "light" 100 "10m"
    ;;
  medium)
    run_stage "medium" 500 "10m"
    ;;
  heavy)
    run_stage "heavy" 1000 "10m"
    ;;
  stress)
    run_stage "stress" 5000 "10m"
    ;;
  extreme)
    run_stage "extreme" 10000 "10m"
    ;;
  all)
    run_stage "light" 100 "10m"
    run_stage "medium" 500 "10m"
    run_stage "heavy" 1000 "10m"
    run_stage "stress" 5000 "10m"
    run_stage "extreme" 10000 "10m"
    echo ""
    echo "✅ 全部阶段完成！结果在 $RESULTS_DIR/"
    echo "   使用: k6 cloud --summary-export=$RESULTS_DIR/summary.json $RESULTS_DIR/*.json"
    ;;
  *)
    stages
    exit 1
    ;;
esac
