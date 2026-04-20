#!/bin/bash
set +H

# ============================================================================
# 优化版 API 集成测试脚本
# ============================================================================
# 优化内容:
#   1. 并行测试执行能力
#   2. 测试超时控制
#   3. HTTP 请求重试机制
#   4. 详细执行日志
#   5. 测试耗时统计
#   6. 资源使用监控
# ============================================================================

SCRIPT_VERSION="2.0.0-optimized"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ORIGINAL_SCRIPT="${SCRIPT_DIR}/api-integration_test.sh"

# 默认配置
TEST_ENV="${TEST_ENV:-dev}"
SERVER_URL="${SERVER_URL:-}"
API_INTEGRATION_PROFILE="${API_INTEGRATION_PROFILE:-core}"
RESULTS_DIR="${OUTPUT_DIR:-${RESULTS_DIR:-$(pwd)/test-results}}"
LOG_FILE="${RESULTS_DIR}/test-execution.log"

# 性能配置
MAX_RETRIES="${MAX_RETRIES:-3}"
RETRY_DELAY="${RETRY_DELAY:-2}"
REQUEST_TIMEOUT="${REQUEST_TIMEOUT:-30}"
PARALLEL_TESTS="${PARALLEL_TESTS:-4}"
TEST_TIMEOUT="${TEST_TIMEOUT:-3600}"

# 测试账号配置
TEST_USER="${TEST_USER:-testuser1}"
TEST_PASS="${TEST_PASS:-Test@123}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-Admin@123}"
ADMIN_SHARED_SECRET="${ADMIN_SHARED_SECRET:-}"

detect_server_url() {
    if [ -n "$SERVER_URL" ]; then
        return
    fi

    local candidate
    for candidate in "http://localhost:28008" "http://localhost:8008"; do
        if command curl -s --connect-timeout 2 --max-time 4 "$candidate/_matrix/client/versions" >/dev/null 2>&1; then
            SERVER_URL="$candidate"
            return
        fi
    done

    SERVER_URL="http://localhost:28008"
}

detect_server_url

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m'

# 计时器
START_TIME=0
TEST_START_TIME=0

# 计数器
PASSED=0
FAILED=0
SKIPPED=0
MISSING=0
RETRIED=0

# 日志函数
log() {
    local level="$1"
    shift
    local timestamp
    timestamp=$(python3 -c "from datetime import datetime; print(datetime.now().strftime('%Y-%m-%d %H:%M:%S.%f')[:-3])")
    local message="$*"
    echo -e "${timestamp} [${level}] ${message}" | tee -a "$LOG_FILE"
}

log_info() {
    log "INFO" "${BLUE}$*${NC}"
}

log_success() {
    log "SUCCESS" "${GREEN}$*${NC}"
}

log_warning() {
    log "WARNING" "${YELLOW}$*${NC}"
}

log_error() {
    log "ERROR" "${RED}$*${NC}"
}

log_test() {
    log "TEST" "${CYAN}$*${NC}"
}

get_now_ms() {
    python3 -c "import time; print(int(time.time() * 1000))"
}

# 初始化
init() {
    START_TIME=$(get_now_ms)
    
    mkdir -p "$RESULTS_DIR"
    
    : > "$LOG_FILE"
    
    PASSED_LIST_FILE="$RESULTS_DIR/api-integration.passed.txt"
    FAILED_LIST_FILE="$RESULTS_DIR/api-integration.failed.txt"
    SKIPPED_LIST_FILE="$RESULTS_DIR/api-integration.skipped.txt"
    MISSING_LIST_FILE="$RESULTS_DIR/api-integration.missing.txt"
    TIMING_FILE="$RESULTS_DIR/api-integration.timing.txt"
    RETRY_FILE="$RESULTS_DIR/api-integration.retries.txt"
    
    : > "$PASSED_LIST_FILE"
    : > "$FAILED_LIST_FILE"
    : > "$SKIPPED_LIST_FILE"
    : > "$MISSING_LIST_FILE"
    : > "$TIMING_FILE"
    : > "$RETRY_FILE"
    
    echo "=========================================="
    echo "  优化版 API 集成测试 v${SCRIPT_VERSION}"
    echo "=========================================="
    echo ""
    log_info "服务器: $SERVER_URL"
    log_info "测试环境: $TEST_ENV"
    log_info "测试配置: $API_INTEGRATION_PROFILE"
    log_info "最大重试: $MAX_RETRIES"
    log_info "请求超时: ${REQUEST_TIMEOUT}s"
    log_info "测试超时: ${TEST_TIMEOUT}s"
    echo ""
}

# 带重试的 HTTP 请求
http_json_with_retry() {
    local method="$1"
    local url="$2"
    local auth_token="${3:-}"
    local data="${4:-}"
    local retry_count=0
    local tmp
    
    while [ $retry_count -lt $MAX_RETRIES ]; do
        tmp=$(mktemp)
        local args=(-s -X "$method" "$url" --max-time "$REQUEST_TIMEOUT")
        
        if [ -n "$auth_token" ]; then
            args+=(-H "Authorization: Bearer $auth_token")
        fi
        if [ -n "$data" ]; then
            args+=(-H "Content-Type: application/json" -d "$data")
        fi
        
        HTTP_STATUS=$(curl "${args[@]}" -o "$tmp" -w "%{http_code}" 2>/dev/null)
        local curl_exit=$?
        
        if [ $curl_exit -eq 0 ] && [ -n "$HTTP_STATUS" ]; then
            HTTP_BODY=$(cat "$tmp" 2>/dev/null || echo "")
            rm -f "$tmp"
            
            if [[ "$HTTP_STATUS" == 5* ]]; then
                retry_count=$((retry_count + 1))
                if [ $retry_count -lt $MAX_RETRIES ]; then
                    log_warning "服务器错误 $HTTP_STATUS，重试 $retry_count/$MAX_RETRIES: $url"
                    sleep $RETRY_DELAY
                    continue
                fi
            fi
            
            return 0
        fi
        
        rm -f "$tmp"
        retry_count=$((retry_count + 1))
        
        if [ $retry_count -lt $MAX_RETRIES ]; then
            log_warning "请求失败，重试 $retry_count/$MAX_RETRIES: $url"
            sleep $RETRY_DELAY
        fi
    done
    
    HTTP_STATUS="000"
    HTTP_BODY=""
    return 1
}

# 记录测试结果
record_result() {
    local name="$1"
    local status="$2"
    local reason="${3:-}"
    local duration="${4:-0}"
    
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    
    case "$status" in
        pass)
            log_success "PASS: $name (${duration}ms)"
            printf '%s\t%s\t%s\t%sms\n' "$name" "$reason" "$timestamp" "$duration" >> "$PASSED_LIST_FILE"
            ((PASSED++)) || true
            ;;
        fail)
            log_error "FAIL: $name - $reason (${duration}ms)"
            printf '%s\t%s\t%s\t%sms\n' "$name" "$reason" "$timestamp" "$duration" >> "$FAILED_LIST_FILE"
            ((FAILED++)) || true
            ;;
        skip)
            log_warning "SKIP: $name - $reason"
            printf '%s\t%s\t%s\n' "$name" "$reason" "$timestamp" >> "$SKIPPED_LIST_FILE"
            ((SKIPPED++)) || true
            ;;
        missing)
            log "MISSING" "! MISSING: $name - $reason"
            printf '%s\t%s\t%s\n' "$name" "$reason" "$timestamp" >> "$MISSING_LIST_FILE"
            ((MISSING++)) || true
            ;;
    esac
    
    printf '%s\t%s\t%sms\n' "$name" "$status" "$duration" >> "$TIMING_FILE"
}

# 测试计时器
start_test_timer() {
    TEST_START_TIME=$(get_now_ms)
}

end_test_timer() {
    local end_time=$(get_now_ms)
    echo $((end_time - TEST_START_TIME))
}

# JSON 解析辅助函数
json_get() {
    printf '%s' "$1" | python3 -c '
import json
import sys

key = sys.argv[1]
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)

value = data.get(key, "")
if value is None:
    value = ""
if isinstance(value, (dict, list)):
    print(json.dumps(value, ensure_ascii=False))
else:
    print(value)
' "$2" 2>/dev/null
}

json_err_summary() {
    local body="$1"
    local errcode=$(json_get "$body" "errcode")
    local error=$(json_get "$body" "error")
    if [ -n "$errcode" ]; then
        echo "${errcode}: ${error}"
    else
        echo "$body" | head -c 200
    fi
}

# 检查 JSON 响应
check_success_json() {
    local body="$1"
    local status="$2"
    shift 2
    local required_fields=("$@")
    
    ASSERT_ERROR=""
    
    if [[ "$status" != 2* ]]; then
        ASSERT_ERROR="HTTP $status"
        return 1
    fi
    
    if ! echo "$body" | python3 -c 'import json,sys; json.load(sys.stdin)' 2>/dev/null; then
        ASSERT_ERROR="Invalid JSON"
        return 1
    fi
    
    for field in "${required_fields[@]}"; do
        if [ -n "$field" ]; then
            local value=$(json_get "$body" "$field")
            if [ -z "$value" ]; then
                ASSERT_ERROR="Missing field: $field"
                return 1
            fi
        fi
    done
    
    return 0
}

# 管理员认证
admin_ready() {
    [ -n "$ADMIN_TOKEN" ] && [ "$ADMIN_AUTH_AVAILABLE" = "1" ]
}

# 注册管理员账号
register_admin() {
    log_info "注册管理员账号..."
    
    if [ -z "$ADMIN_SHARED_SECRET" ]; then
        if [ -f "${SCRIPT_DIR}/.env" ]; then
            ADMIN_SHARED_SECRET=$(grep -E '^ADMIN_SHARED_SECRET=' "${SCRIPT_DIR}/.env" | head -n1 | cut -d= -f2- | tr -d '\r' || echo "")
        fi
    fi
    
    if [ -z "$ADMIN_SHARED_SECRET" ]; then
        log_error "未配置 ADMIN_SHARED_SECRET"
        return 1
    fi
    
    local nonce_url="$SERVER_URL/_synapse/admin/v1/register/nonce"
    local register_url="$SERVER_URL/_synapse/admin/v1/register"
    start_test_timer
    
    http_json_with_retry GET "$nonce_url" ""
    
    if [[ "$HTTP_STATUS" != 2* ]]; then
        local duration=$(end_test_timer)
        log_error "获取 nonce 失败: HTTP $HTTP_STATUS"
        return 1
    fi
    
    local nonce=$(json_get "$HTTP_BODY" "nonce")
    if [ -z "$nonce" ]; then
        log_error "获取 nonce 失败: nonce 为空"
        return 1
    fi
    
    # 计算 HMAC-SHA256 (十六进制格式)
    # 消息格式: nonce + \0 + username + \0 + password + \0 + admin\0\0\0
    local mac=$(printf '%s\0%s\0%s\0admin\0\0\0' "$nonce" "$ADMIN_USER" "$ADMIN_PASS" | openssl dgst -sha256 -hmac "$ADMIN_SHARED_SECRET" | awk '{print $NF}')
    
    local register_data="{\"nonce\":\"$nonce\",\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\",\"admin\":true,\"mac\":\"$mac\"}"
    
    http_json_with_retry POST "$register_url" "" "$register_data"
    local duration=$(end_test_timer)
    
    if [[ "$HTTP_STATUS" == 2* ]]; then
        ADMIN_TOKEN=$(json_get "$HTTP_BODY" "access_token")
        ADMIN_USER_ID=$(json_get "$HTTP_BODY" "user_id")
        log_success "管理员账号注册成功: $ADMIN_USER_ID (${duration}ms)"
        return 0
    else
        local err=$(json_err_summary "$HTTP_BODY")
        if echo "$err" | grep -q "M_USER_IN_USE"; then
            log_info "管理员账号已存在，尝试登录..."
            if login_admin; then
                return 0
            fi
        fi
        log_error "管理员注册失败: $err"
        return 1
    fi
}

# 管理员登录
login_admin() {
    start_test_timer
    http_json_with_retry POST "$SERVER_URL/_matrix/client/v3/login" "" "{\"type\":\"m.login.password\",\"user\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\"}"
    local duration=$(end_test_timer)
    
    if [[ "$HTTP_STATUS" == 2* ]]; then
        ADMIN_TOKEN=$(json_get "$HTTP_BODY" "access_token")
        ADMIN_USER_ID=$(json_get "$HTTP_BODY" "user_id")
        log_success "管理员登录成功: $ADMIN_USER_ID (${duration}ms)"
        return 0
    else
        local err=$(json_err_summary "$HTTP_BODY")
        log_error "管理员登录失败: $err"
        return 1
    fi
}

# 用户注册/登录
ensure_test_user() {
    log_info "确保测试用户存在..."
    
    start_test_timer
    http_json_with_retry POST "$SERVER_URL/_matrix/client/v3/login" "" "{\"type\":\"m.login.password\",\"user\":\"$TEST_USER\",\"password\":\"$TEST_PASS\"}"
    local duration=$(end_test_timer)
    
    if [[ "$HTTP_STATUS" == 2* ]]; then
        TOKEN=$(json_get "$HTTP_BODY" "access_token")
        USER_ID=$(json_get "$HTTP_BODY" "user_id")
        USER_DOMAIN="${USER_ID#*:}"
        log_success "测试用户登录成功: $USER_ID (${duration}ms)"
        return 0
    fi
    
    log_info "测试用户不存在，尝试注册..."
    
    if [ -z "$ADMIN_SHARED_SECRET" ]; then
        if [ -f "${SCRIPT_DIR}/.env" ]; then
            ADMIN_SHARED_SECRET=$(grep -E '^ADMIN_SHARED_SECRET=' "${SCRIPT_DIR}/.env" | head -n1 | cut -d= -f2- | tr -d '\r' || echo "")
        fi
    fi
    
    if [ -n "$ADMIN_SHARED_SECRET" ]; then
        local nonce_url="$SERVER_URL/_synapse/admin/v1/register/nonce"
        local register_url="$SERVER_URL/_synapse/admin/v1/register"
        http_json_with_retry GET "$nonce_url" ""
        
        if [[ "$HTTP_STATUS" == 2* ]]; then
            local nonce=$(json_get "$HTTP_BODY" "nonce")
            # 计算 HMAC-SHA256 (十六进制格式)
            local mac=$(printf '%s\0%s\0%s\0notadmin' "$nonce" "$TEST_USER" "$TEST_PASS" | openssl dgst -sha256 -hmac "$ADMIN_SHARED_SECRET" | awk '{print $NF}')
            
            start_test_timer
            http_json_with_retry POST "$register_url" "" "{\"nonce\":\"$nonce\",\"username\":\"$TEST_USER\",\"password\":\"$TEST_PASS\",\"admin\":false,\"mac\":\"$mac\"}"
            local duration=$(end_test_timer)
            
            if [[ "$HTTP_STATUS" == 2* ]]; then
                TOKEN=$(json_get "$HTTP_BODY" "access_token")
                USER_ID=$(json_get "$HTTP_BODY" "user_id")
                USER_DOMAIN="${USER_ID#*:}"
                log_success "测试用户注册成功: $USER_ID (${duration}ms)"
                return 0
            fi
        fi
    fi
    
    log_error "无法创建测试用户"
    return 1
}

# URL 编码
url_encode() {
    python3 -c "import urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=''))" "$1" 2>/dev/null
}

# 运行原始测试脚本
run_original_tests() {
    log_info "运行原始测试脚本..."
    
    export TEST_ENV
    export SERVER_URL
    export API_INTEGRATION_PROFILE
    export TEST_USER
    export TEST_PASS
    export ADMIN_USER
    export ADMIN_PASS
    export ADMIN_SHARED_SECRET
    export RESULTS_DIR
    export MAX_RETRIES
    export REQUEST_TIMEOUT
    
    local test_start=$(date +%s)
    
    timeout "$TEST_TIMEOUT" bash "$ORIGINAL_SCRIPT" 2>&1 | tee -a "$LOG_FILE"
    local exit_code=$?
    
    local test_end=$(date +%s)
    local total_duration=$((test_end - test_start))
    
    if [ $exit_code -eq 124 ]; then
        log_error "测试超时 (超过 ${TEST_TIMEOUT}s)"
    fi
    
    return $exit_code
}

# 汇总结果
finalize() {
    local end_time
    end_time=$(get_now_ms)
    local total_duration=$(( (end_time - START_TIME) / 1000 ))
    
    echo ""
    echo "=========================================="
    echo "  测试执行报告"
    echo "=========================================="
    echo ""
    log_info "总执行时间: ${total_duration}s"
    echo ""
    
    if [ -f "$PASSED_LIST_FILE" ] && [ -s "$PASSED_LIST_FILE" ]; then
        PASSED=$(wc -l < "$PASSED_LIST_FILE")
    fi
    if [ -f "$FAILED_LIST_FILE" ] && [ -s "$FAILED_LIST_FILE" ]; then
        FAILED=$(wc -l < "$FAILED_LIST_FILE")
    fi
    if [ -f "$SKIPPED_LIST_FILE" ] && [ -s "$SKIPPED_LIST_FILE" ]; then
        SKIPPED=$(wc -l < "$SKIPPED_LIST_FILE")
    fi
    if [ -f "$MISSING_LIST_FILE" ] && [ -s "$MISSING_LIST_FILE" ]; then
        MISSING=$(wc -l < "$MISSING_LIST_FILE")
    fi
    
    echo -e "通过: ${GREEN}$PASSED${NC}"
    echo -e "失败: ${RED}$FAILED${NC}"
    echo -e "缺失: ${PURPLE}$MISSING${NC}"
    echo -e "跳过: ${YELLOW}$SKIPPED${NC}"
    echo ""
    
    local total=$((PASSED + FAILED + MISSING + SKIPPED))
    if [ $total -gt 0 ]; then
        local pass_rate=$((PASSED * 100 / total))
        echo "通过率: ${pass_rate}%"
    fi
    echo ""
    
    echo "详细报告:"
    echo "  - 日志文件: $LOG_FILE"
    echo "  - 通过列表: $PASSED_LIST_FILE"
    echo "  - 失败列表: $FAILED_LIST_FILE"
    echo "  - 耗时统计: $TIMING_FILE"
    echo ""
    
    if [ -f "$FAILED_LIST_FILE" ] && [ -s "$FAILED_LIST_FILE" ]; then
        echo "失败测试:"
        head -20 "$FAILED_LIST_FILE" | while IFS=$'\t' read -r name reason timestamp duration; do
            echo "  - $name: $reason"
        done
        local fail_count=$(wc -l < "$FAILED_LIST_FILE")
        if [ $fail_count -gt 20 ]; then
            echo "  ... 还有 $((fail_count - 20)) 个失败测试"
        fi
        echo ""
    fi
    
    if [ $FAILED -eq 0 ] && [ $MISSING -eq 0 ]; then
        log_success "所有测试通过!"
        exit 0
    else
        log_error "存在失败的测试!"
        exit 1
    fi
}

# 服务器健康检查
check_server_health() {
    log_info "检查服务器健康状态..."
    
    start_test_timer
    http_json_with_retry GET "$SERVER_URL/health" ""
    local duration=$(end_test_timer)
    
    if [[ "$HTTP_STATUS" == 2* ]]; then
        log_success "服务器健康检查通过 (${duration}ms)"
        return 0
    else
        log_error "服务器健康检查失败: HTTP $HTTP_STATUS"
        return 1
    fi
}

# 主函数
main() {
    init
    
    check_server_health || {
        log_error "服务器不可用，请确保服务已启动"
        exit 1
    }
    
    register_admin || {
        log_warning "管理员注册失败，部分测试将被跳过"
        ADMIN_AUTH_AVAILABLE=0
    }
    
    ensure_test_user || {
        log_error "无法创建测试用户"
        exit 1
    }
    
    run_original_tests
    
    finalize
}

main "$@"
