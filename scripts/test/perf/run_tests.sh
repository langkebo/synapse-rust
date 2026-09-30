#!/usr/bin/env bash
#
# k6 负载测试脚本
#
# 用途：运行不同阶段的性能测试
# 用法:
#   ./run_tests.sh smoke
#   ./run_tests.sh light
#   ./run_tests.sh moderate
#   ./run_tests.sh heavy
#
# 前提条件:
#   - synapse-rust 服务已在 http://localhost:8008 运行
#   - k6 已安装
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
K6_SCRIPT="${SCRIPT_DIR}/api_matrix_core.js"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ========== 配置 ==========
# 这些可以通过环境变量覆盖
BASE_URL="${BASE_URL:-http://localhost:8008}"
TEST_USERNAME="${TEST_USERNAME:-admin}"
TEST_PASSWORD="${TEST_PASSWORD:-Admin@123}"

# ========== 阶段配置 ==========
declare -A SCENARIOS=(
    ["smoke"]="10,30s 10,1m 0,30s"
    ["light"]="50,1m 50,2m 0,30s"
    ["moderate"]="100,1m 100,3m 0,30s"
    ["heavy"]="200,1m 200,5m 0,30s"
)

# ========== 辅助函数 ==========
print_header() {
    echo -e "\n${BLUE}================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}================================${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

check_server() {
    print_header "检查服务器可达性"

    if ! curl -s "${BASE_URL}/_matrix/client/versions" >/dev/null 2>&1; then
        print_error "Synapse server not reachable at ${BASE_URL}"
        echo "请确保服务正在运行："
        echo "  /opt/homebrew/bin/docker ps | grep synapse-app"
        exit 1
    fi

    print_success "Server is reachable at ${BASE_URL}"

    # 检查 Prometheus
    if curl -s "http://localhost:9092/api/v1/query?query=up" >/dev/null 2>&1; then
        print_success "Prometheus is running on port 9092"
    else
        print_warning "Prometheus not reachable (metrics will not be collected)"
    fi

    # 检查 Grafana
    if curl -s "http://localhost:3000/api/health" >/dev/null 2>&1; then
        print_success "Grafana is running on port 3000"
    else
        print_warning "Grafana not reachable"
    fi
}

parse_result() {
    local result_file="$1"

    echo -e "\n${BLUE}===== 测试结果摘要 =====${NC}"

    # 从 JSON 结果中提取关键指标
    if [[ -f "${result_file}" ]]; then
        local duration=$(jq -r '.state.testRunDurationMs / 1000' "${result_file}")
        local requests=$(jq -r '.metrics.http_reqs.values.count' "${result_file}")
        local errors=$(jq -r '.metrics.http_req_failed.values.rate * 100' "${result_file}")
        local p50=$(jq -r '.metrics.http_req_duration.values["p(50)"]' "${result_file}")
        local p95=$(jq -r '.metrics.http_req_duration.values["p(95)"]' "${result_file}")
        local p99=$(jq -r '.metrics.http_req_duration.values["p(99)"]' "${result_file}")

        printf "  持续时间：%ss\n" "${duration}"
        printf "  总请求数：%s\n" "${requests}"
        printf "  错误率：  %.2f%%\n" "${errors}"
        printf "  P50:      %sms\n" "${p50}"
        printf "  P95:      %sms\n" "${p95}"
        printf "  P99:      %sms\n" "${p99}"
    else
        print_warning "Results file not found: ${result_file}"
    fi

    echo ""
}

compare_thresholds() {
    local scenario="$1"
    local p95="$2"
    local errors="$3"

    echo -e "\n${BLUE}===== 性能阈值对比 =====${NC}"

    case "$scenario" in
        "smoke")
            local p95_target=100
            local errors_target=1.0
            ;;
        "light")
            local p95_target=200
            local errors_target=1.0
            ;;
        "moderate")
            local p95_target=500
            local errors_target=2.0
            ;;
        "heavy")
            local p95_target=1000
            local errors_target=5.0
            ;;
        *)
            local p95_target=500
            local errors_target=1.0
            ;;
    esac

    # P95 比较
    if (($(echo "$p95 < $p95_target" | bc -l))); then
        print_success "P95 (${p95}ms) < ${p95_target}ms (target)"
    elif (($(echo "$p95 < $p95_target * 1.5" | bc -l))); then
        print_warning "P95 (${p95}ms) > ${p95_target}ms (target), but acceptable"
    else
        print_error "P95 (${p95}ms) significantly exceeds ${p95_target}ms (target)"
    fi

    # 错误率比较
    if (($(echo "$errors < $errors_target" | bc -l))); then
        print_success "Error rate (${errors}%) < ${errors_target}% (threshold)"
    else
        print_error "Error rate (${errors}%) exceeds ${errors_target}% (threshold)"
    fi
}

cleanup_old_results() {
    find "${SCRIPT_DIR}/results" -name "*.json" -mtime +7 -delete 2>/dev/null || true
}

# ========== 主逻辑 ==========
main() {
    local stage="${1:-smoke}"

    print_header "Synapse-rust 性能测试 (k6)"

    echo "阶段：${stage}"
    echo "剧本：${SCENARIOS[$stage]}"
    echo "脚本：${K6_SCRIPT}"
    echo ""

    # 检查 k6
    if ! command -v k6 &>/dev/null; then
        print_error "k6 is not installed"
        echo "请安装 k6:"
        echo "  brew install k6"
        exit 1
    fi

    check_server

    # 清理旧结果
    cleanup_old_results

    # 创建结果目录
    mkdir -p "${SCRIPT_DIR}/results"

    # 生成时间戳文件名
    local timestamp=$(date +%Y%m%d_%H%M%S)
    local result_file="${SCRIPT_DIR}/results/${stage}_${timestamp}.json"
    local log_file="${SCRIPT_DIR}/results/${stage}_${timestamp}.log"

    # 设置环境变量
    export BASE_URL TEST_USERNAME TEST_PASSWORD

    print_header "开始测试：${stage}"

    # 提取场景参数
    local stages="${SCENARIOS[$stage]}"

    # 构建 k6 命令
    local vus_start=$(echo "$stages" | cut -d',' -f1)
    local vus_end=$(echo "$stages" | rev | cut -d',' -f1 | rev | cut -d',' -f1)
    local total_duration=$(echo "$stages" | tr ' ' '\n' | cut -d',' -f2 | paste -sd'+' | bc)

    echo "初始并发：${vus_start}"
    echo "最大并发：${vus_end}"
    echo "预计时长：${total_duration}s"
    echo ""

    # 运行 k6
    set +e
    k6 run \
        --out json="${result_file}" \
        --env BASE_URL="${BASE_URL}" \
        --env TEST_USERNAME="${TEST_USERNAME}" \
        --env TEST_PASSWORD="${TEST_PASSWORD}" \
        "${K6_SCRIPT}" 2>&1 | tee "${log_file}"
    local exit_code=${PIPESTATUS[0]}
    set -e

    # 解析结果
    parse_result "${result_file}"

    # 提取并比较阈值
    if [[ -f "${result_file}" ]]; then
        local p95=$(jq -r '.metrics.http_req_duration.values["p(95)"]' "${result_file}")
        local errors=$(jq -r '.metrics.http_req_failed.values.rate * 100' "${result_file}")
        compare_thresholds "${stage}" "${p95}" "${errors}"
    fi

    print_header "测试完成"

    echo "结果文件:"
    echo "  JSON: ${result_file}"
    echo "  Log:  ${log_file}"

    echo -e "\n${BLUE}下一步:${NC}"
    echo "  - 查看详��日志：cat ${log_file}"
    echo "  - 查看 Grafana 面板：http://localhost:3000/d/system-overview"
    echo "  - 运行下一阶段：./run_tests.sh next_stage"

    exit ${exit_code}
}

# 启动
main "$@"
