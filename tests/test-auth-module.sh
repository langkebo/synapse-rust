#!/bin/bash

###############################################################################
# Synapse Rust 认证模块测试脚本
#
# 功能: 测试认证模块的所有13个API
# 版本: 1.0.0
# 创建日期: 2026-02-02
###############################################################################

set -e  # 遇到错误立即退出

###############################################################################
# 配置变量
###############################################################################

# 服务器配置
SERVER_URL="${SERVER_URL:-http://localhost:8008}"
SERVER_NAME="${SERVER_NAME:-localhost}"

# 测试用户配置
TEST_USER1="testuser1_$(date +%s)"
TEST_USER2="testuser2_$(date +%s)"
TEST_ADMIN="adminuser_$(date +%s)"
TEST_PASSWORD="TestPassword123"
TEST_ADMIN_PASSWORD="AdminPassword789"

# 临时变量
ACCESS_TOKEN=""
REFRESH_TOKEN=""
DEVICE_ID=""
USER_ID=""
ADMIN_TOKEN=""
ADMIN_REFRESH_TOKEN=""
ADMIN_USER_ID=""

# 结果文件
RESULT_DIR="./tests/results"
RESULT_FILE="${RESULT_DIR}/auth-test-results-$(date +%Y%m%d-%H%M%S).json"
LOG_FILE="${RESULT_DIR}/auth-test-log-$(date +%Y%m%d-%H%M%S).txt"

# 测试统计
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
SKIPPED_TESTS=0

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

###############################################################################
# 辅助函数
###############################################################################

# 打印带颜色的消息
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1" | tee -a "$LOG_FILE"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1" | tee -a "$LOG_FILE"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$LOG_FILE"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1" | tee -a "$LOG_FILE"
}

# 创建结果目录
mkdir -p "$RESULT_DIR"

# 初始化结果JSON
init_results() {
    cat > "$RESULT_FILE" << EOF
{
  "test_suite": "认证模块测试",
  "test_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "test_environment": {
    "server_url": "$SERVER_URL",
    "server_name": "$SERVER_NAME"
  },
  "summary": {
    "total_tests": 0,
    "passed": 0,
    "failed": 0,
    "skipped": 0,
    "success_rate": "0%"
  },
  "results": []
}
EOF
}

# 更新测试结果
update_result() {
    local test_id="$1"
    local api="$2"
    local test_name="$3"
    local status="$4"
    local duration="$5"
    local http_status="$6"
    local response="$7"
    local error="$8"

    # 使用临时文件更新JSON
    local temp_file="${RESULT_FILE}.tmp"
    
    # 读取现有JSON
    local existing_json=$(cat "$RESULT_FILE")
    
    # 更新统计
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    if [ "$status" = "passed" ]; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
    elif [ "$status" = "failed" ]; then
        FAILED_TESTS=$((FAILED_TESTS + 1))
    else
        SKIPPED_TESTS=$((SKIPPED_TESTS + 1))
    fi
    
    local success_rate=$(awk "BEGIN {printf \"%.1f\", ($PASSED_TESTS/$TOTAL_TESTS)*100}")
    
    # 构建新的结果条目JSON字符串
    local new_result_json=$(cat << EOF
{
  "test_id": "$test_id",
  "api": "$api",
  "test_name": "$test_name",
  "status": "$status",
  "duration_ms": $duration,
  "http_status": $http_status,
  "response": $response,
  "error": $error
}
EOF
)
    
    # 使用Python更新JSON
    python3 << PYTHON_SCRIPT
import json
import sys

# 读取现有JSON
with open('$RESULT_FILE', 'r') as f:
    data = json.load(f)

# 更新统计
data['summary']['total_tests'] = $TOTAL_TESTS
data['summary']['passed'] = $PASSED_TESTS
data['summary']['failed'] = $FAILED_TESTS
data['summary']['skipped'] = $SKIPPED_TESTS
data['summary']['success_rate'] = '$success_rate%'

# 添加新结果
new_result = json.loads('''$new_result_json''')
data['results'].append(new_result)

# 写回文件
with open('$RESULT_FILE', 'w') as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
PYTHON_SCRIPT
}

# 执行测试并记录结果
run_test() {
    local test_id="$1"
    local api="$2"
    local test_name="$3"
    local method="$4"
    local endpoint="$5"
    local data="$6"
    local expected_status="$7"
    local auth_header="$8"
    
    print_info "执行测试: $test_id - $test_name"
    print_info "API: $method $endpoint"
    
    local start_time=$(date +%s%3N)
    local http_status=""
    local response=""
    local error="null"
    local status="passed"
    
    # 构建curl命令
    local curl_cmd="curl -s -w '\n%{http_code}' -X $method"
    
    if [ -n "$auth_header" ]; then
        curl_cmd="$curl_cmd -H 'Authorization: Bearer $auth_header'"
    fi
    
    if [ -n "$data" ]; then
        curl_cmd="$curl_cmd -H 'Content-Type: application/json' -d '$data'"
    fi
    
    curl_cmd="$curl_cmd '$SERVER_URL$endpoint'"
    
    # 执行curl命令
    local output
    output=$(eval "$curl_cmd" 2>&1)
    local exit_code=$?
    
    local end_time=$(date +%s%3N)
    local duration=$((end_time - start_time))
    
    # 解析响应
    if [ $exit_code -eq 0 ]; then
        http_status=$(echo "$output" | tail -n1)
        response=$(echo "$output" | sed '$d')
        
        # 验证HTTP状态码
        if [ "$http_status" = "$expected_status" ]; then
            print_success "测试通过: HTTP $http_status"
            status="passed"
        else
            print_error "测试失败: 期望 HTTP $expected_status, 实际 HTTP $http_status"
            status="failed"
            error="\"Expected HTTP $expected_status, got $http_status\""
        fi
    else
        print_error "curl执行失败: 退出码 $exit_code"
        http_status=0
        status="failed"
        error="\"curl failed with exit code $exit_code\""
    fi
    
    # 格式化响应为JSON
    if [ -n "$response" ]; then
        # 尝试解析为JSON
        if echo "$response" | jq . >/dev/null 2>&1; then
            response=$(echo "$response" | jq -c .)
        else
            response="\"$response\""
        fi
    else
        response="null"
    fi
    
    # 更新结果
    update_result "$test_id" "$api" "$test_name" "$status" "$duration" "$http_status" "$response" "$error"
    
    echo "" | tee -a "$LOG_FILE"
}

###############################################################################
# 测试函数
###############################################################################

# 测试1.1: 获取客户端版本 - 正常请求
test_1_1() {
    run_test "1.1" "GET /_matrix/client/versions" "正常请求" \
        "GET" "/_matrix/client/versions" "" "200" ""
}

# 测试1.2: 获取客户端版本 - 重复请求
test_1_2() {
    run_test "1.2" "GET /_matrix/client/versions" "重复请求" \
        "GET" "/_matrix/client/versions" "" "200" ""
}

# 测试2.1: 检查用户名可用性 - 不存在的用户名
test_2_1() {
    run_test "2.1" "GET /_matrix/client/r0/register/available" "检查不存在的用户名" \
        "GET" "/_matrix/client/r0/register/available?username=$TEST_USER1" "" "200" ""
}

# 测试2.2: 检查用户名可用性 - 已存在的用户名
test_2_2() {
    # 先注册一个用户
    local register_data="{\"username\":\"$TEST_USER1\",\"password\":\"$TEST_PASSWORD\"}"
    curl -s -X POST -H "Content-Type: application/json" \
        -d "$register_data" "$SERVER_URL/_matrix/client/r0/register" >/dev/null
    
    # 添加延迟避免速率限制
    sleep 2
    
    run_test "2.2" "GET /_matrix/client/r0/register/available" "检查已存在的用户名" \
        "GET" "/_matrix/client/r0/register/available?username=$TEST_USER1" "" "200" ""
}

# 测试2.3: 检查用户名可用性 - 空用户名
test_2_3() {
    run_test "2.3" "GET /_matrix/client/r0/register/available" "空用户名" \
        "GET" "/_matrix/client/r0/register/available?username=" "" "400" ""
}

# 测试3.1: 用户注册 - 正常注册
test_3_1() {
    local register_data="{\"username\":\"$TEST_USER2\",\"password\":\"$TEST_PASSWORD\",\"displayname\":\"Test User 2\"}"
    local output
    output=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$register_data" "$SERVER_URL/_matrix/client/r0/register")
    
    # 提取用户ID和访问令牌
    USER_ID=$(echo "$output" | jq -r '.user_id // empty')
    ACCESS_TOKEN=$(echo "$output" | jq -r '.access_token // empty')
    DEVICE_ID=$(echo "$output" | jq -r '.device_id // empty')
    
    if [ -n "$USER_ID" ] && [ -n "$ACCESS_TOKEN" ]; then
        print_success "用户注册成功: $USER_ID"
        print_info "Access Token: ${ACCESS_TOKEN:0:20}..."
    else
        print_error "用户注册失败"
    fi
    
    run_test "3.1" "POST /_matrix/client/r0/register" "正常注册" \
        "POST" "/_matrix/client/r0/register" "$register_data" "200" ""
}

# 测试3.2: 用户注册 - 重复注册相同用户名
test_3_2() {
    # 添加延迟避免速率限制
    sleep 2
    
    local register_data="{\"username\":\"$TEST_USER2\",\"password\":\"$TEST_PASSWORD\"}"
    run_test "3.2" "POST /_matrix/client/r0/register" "重复注册相同用户名" \
        "POST" "/_matrix/client/r0/register" "$register_data" "400" ""
}

# 测试3.3: 用户注册 - 密码太短
test_3_3() {
    # 添加延迟避免速率限制
    sleep 2
    
    local register_data="{\"username\":\"$TEST_USER2_short\",\"password\":\"123456\"}"
    run_test "3.3" "POST /_matrix/client/r0/register" "密码太短" \
        "POST" "/_matrix/client/r0/register" "$register_data" "400" ""
}

# 测试3.4: 用户注册 - 缺少必填字段
test_3_4() {
    # 添加延迟避免速率限制
    sleep 2
    
    local register_data="{\"username\":\"$TEST_USER2_missing\"}"
    run_test "3.4" "POST /_matrix/client/r0/register" "缺少必填字段" \
        "POST" "/_matrix/client/r0/register" "$register_data" "400" ""
}

# 测试3.5: 用户注册 - 注册管理员账户
test_3_5() {
    # 添加延迟避免速率限制
    sleep 2
    
    local register_data="{\"username\":\"$TEST_ADMIN\",\"password\":\"$TEST_ADMIN_PASSWORD\",\"admin\":true}"
    local output
    output=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$register_data" "$SERVER_URL/_matrix/client/r0/register")
    
    # 提取管理员令牌
    ADMIN_TOKEN=$(echo "$output" | jq -r '.access_token // empty')
    ADMIN_USER_ID=$(echo "$output" | jq -r '.user_id // empty')
    ADMIN_REFRESH_TOKEN=$(echo "$output" | jq -r '.refresh_token // empty')
    
    if [ -n "$ADMIN_TOKEN" ]; then
        print_success "管理员注册成功: $ADMIN_USER_ID"
    else
        print_error "管理员注册失败"
    fi
    
    run_test "3.5" "POST /_matrix/client/r0/register" "注册管理员账户" \
        "POST" "/_matrix/client/r0/register" "$register_data" "200" ""
}

# 测试4.1: 用户登录 - 正常登录
test_4_1() {
    local login_data="{\"user\":\"$TEST_USER2\",\"password\":\"$TEST_PASSWORD\"}"
    local output
    output=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$login_data" "$SERVER_URL/_matrix/client/r0/login")
    
    # 更新访问令牌
    ACCESS_TOKEN=$(echo "$output" | jq -r '.access_token // empty')
    REFRESH_TOKEN=$(echo "$output" | jq -r '.refresh_token // empty')
    
    if [ -n "$ACCESS_TOKEN" ]; then
        print_success "用户登录成功"
    else
        print_error "用户登录失败"
    fi
    
    run_test "4.1" "POST /_matrix/client/r0/login" "正常登录" \
        "POST" "/_matrix/client/r0/login" "$login_data" "200" ""
}

# 测试4.2: 用户登录 - 错误密码
test_4_2() {
    local login_data="{\"user\":\"$TEST_USER2\",\"password\":\"WrongPassword123\"}"
    run_test "4.2" "POST /_matrix/client/r0/login" "错误密码" \
        "POST" "/_matrix/client/r0/login" "$login_data" "401" ""
}

# 测试4.3: 用户登录 - 不存在的用户
test_4_3() {
    local login_data="{\"user\":\"nonexistentuser\",\"password\":\"$TEST_PASSWORD\"}"
    run_test "4.3" "POST /_matrix/client/r0/login" "不存在的用户" \
        "POST" "/_matrix/client/r0/login" "$login_data" "401" ""
}

# 测试4.4: 用户登录 - 缺少密码
test_4_4() {
    local login_data="{\"user\":\"$TEST_USER2\"}"
    run_test "4.4" "POST /_matrix/client/r0/login" "缺少密码" \
        "POST" "/_matrix/client/r0/login" "$login_data" "400" ""
}

# 测试5.1: 获取当前用户信息 - 正常请求
test_5_1() {
    run_test "5.1" "GET /_matrix/client/r0/account/whoami" "正常请求" \
        "GET" "/_matrix/client/r0/account/whoami" "" "200" "$ACCESS_TOKEN"
}

# 测试5.2: 获取当前用户信息 - 无效令牌
test_5_2() {
    run_test "5.2" "GET /_matrix/client/r0/account/whoami" "无效令牌" \
        "GET" "/_matrix/client/r0/account/whoami" "" "401" "invalid_token_12345"
}

# 测试5.3: 获取当前用户信息 - 缺少令牌
test_5_3() {
    run_test "5.3" "GET /_matrix/client/r0/account/whoami" "缺少令牌" \
        "GET" "/_matrix/client/r0/account/whoami" "" "401" ""
}

# 测试6.1: 获取用户资料 - 获取自己的资料
test_6_1() {
    run_test "6.1" "GET /_matrix/client/r0/account/profile/{user_id}" "获取自己的资料" \
        "GET" "/_matrix/client/r0/account/profile/$USER_ID" "" "200" "$ACCESS_TOKEN"
}

# 测试6.2: 获取用户资料 - 获取其他用户资料
test_6_2() {
    run_test "6.2" "GET /_matrix/client/r0/account/profile/{user_id}" "获取其他用户资料" \
        "GET" "/_matrix/client/r0/account/profile/$ADMIN_USER_ID" "" "200" "$ACCESS_TOKEN"
}

# 测试6.3: 获取用户资料 - 不存在的用户
test_6_3() {
    run_test "6.3" "GET /_matrix/client/r0/account/profile/{user_id}" "不存在的用户" \
        "GET" "/_matrix/client/r0/account/profile/@nonexistent:server.com" "" "404" "$ACCESS_TOKEN"
}

# 测试7.1: 更新显示名称 - 更新自己的显示名
test_7_1() {
    local displayname_data="{\"displayname\":\"Updated Test User\"}"
    run_test "7.1" "PUT /_matrix/client/r0/account/profile/{user_id}/displayname" "更新自己的显示名" \
        "PUT" "/_matrix/client/r0/account/profile/$USER_ID/displayname" "$displayname_data" "200" "$ACCESS_TOKEN"
}

# 测试7.2: 更新显示名称 - 更新为空字符串
test_7_2() {
    local displayname_data="{\"displayname\":\"\"}"
    run_test "7.2" "PUT /_matrix/client/r0/account/profile/{user_id}/displayname" "更新为空字符串" \
        "PUT" "/_matrix/client/r0/account/profile/$USER_ID/displayname" "$displayname_data" "200" "$ACCESS_TOKEN"
}

# 测试7.3: 更新显示名称 - 无效令牌
test_7_3() {
    local displayname_data="{\"displayname\":\"Test\"}"
    run_test "7.3" "PUT /_matrix/client/r0/account/profile/{user_id}/displayname" "无效令牌" \
        "PUT" "/_matrix/client/r0/account/profile/$USER_ID/displayname" "$displayname_data" "401" "invalid_token"
}

# 测试8.1: 更新头像 - 更新自己的头像
test_8_1() {
    local avatar_data="{\"avatar_url\":\"mxc://server.com/media123\"}"
    run_test "8.1" "PUT /_matrix/client/r0/account/profile/{user_id}/avatar_url" "更新自己的头像" \
        "PUT" "/_matrix/client/r0/account/profile/$USER_ID/avatar_url" "$avatar_data" "200" "$ACCESS_TOKEN"
}

# 测试8.2: 更新头像 - 更新为空字符串
test_8_2() {
    local avatar_data="{\"avatar_url\":\"\"}"
    run_test "8.2" "PUT /_matrix/client/r0/account/profile/{user_id}/avatar_url" "更新为空字符串" \
        "PUT" "/_matrix/client/r0/account/profile/$USER_ID/avatar_url" "$avatar_data" "200" "$ACCESS_TOKEN"
}

# 测试8.3: 更新头像 - 无效令牌
test_8_3() {
    local avatar_data="{\"avatar_url\":\"mxc://server.com/media123\"}"
    run_test "8.3" "PUT /_matrix/client/r0/account/profile/{user_id}/avatar_url" "无效令牌" \
        "PUT" "/_matrix/client/r0/account/profile/$USER_ID/avatar_url" "$avatar_data" "401" "invalid_token"
}

# 测试9.1: 修改密码 - 正常修改密码
test_9_1() {
    local password_data="{\"new_password\":\"NewPassword456\"}"
    run_test "9.1" "POST /_matrix/client/r0/account/password" "正常修改密码" \
        "POST" "/_matrix/client/r0/account/password" "$password_data" "200" "$ACCESS_TOKEN"
}

# 测试9.2: 修改密码 - 新密码太短
test_9_2() {
    local password_data="{\"new_password\":\"123456\"}"
    run_test "9.2" "POST /_matrix/client/r0/account/password" "新密码太短" \
        "POST" "/_matrix/client/r0/account/password" "$password_data" "400" "$ACCESS_TOKEN"
}

# 测试9.3: 修改密码 - 缺少new_password
test_9_3() {
    local password_data="{}"
    run_test "9.3" "POST /_matrix/client/r0/account/password" "缺少new_password" \
        "POST" "/_matrix/client/r0/account/password" "$password_data" "400" "$ACCESS_TOKEN"
}

# 测试9.4: 修改密码 - 无效令牌
test_9_4() {
    local password_data="{\"new_password\":\"NewPassword456\"}"
    run_test "9.4" "POST /_matrix/client/r0/account/password" "无效令牌" \
        "POST" "/_matrix/client/r0/account/password" "$password_data" "401" "invalid_token"
}

# 测试10.1: 刷新令牌 - 正常刷新令牌
test_10_1() {
    # 修改密码后需要重新登录获取新的刷新令牌
    local login_data="{\"user\":\"$TEST_USER2\",\"password\":\"NewPassword456\"}"
    local output
    output=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$login_data" "$SERVER_URL/_matrix/client/r0/login")
    
    # 更新访问令牌和刷新令牌
    ACCESS_TOKEN=$(echo "$output" | jq -r '.access_token // empty')
    REFRESH_TOKEN=$(echo "$output" | jq -r '.refresh_token // empty')
    
    if [ -z "$REFRESH_TOKEN" ]; then
        print_error "重新登录失败，无法获取刷新令牌"
        return 1
    fi
    
    local refresh_data="{\"refresh_token\":\"$REFRESH_TOKEN\"}"
    local refresh_output
    refresh_output=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$refresh_data" "$SERVER_URL/_matrix/client/r0/refresh")
    
    # 更新访问令牌
    ACCESS_TOKEN=$(echo "$refresh_output" | jq -r '.access_token // empty')
    
    run_test "10.1" "POST /_matrix/client/r0/refresh" "正常刷新令牌" \
        "POST" "/_matrix/client/r0/refresh" "$refresh_data" "200" ""
}

# 测试10.2: 刷新令牌 - 无效refresh_token
test_10_2() {
    local refresh_data="{\"refresh_token\":\"invalid_refresh_token\"}"
    run_test "10.2" "POST /_matrix/client/r0/refresh" "无效refresh_token" \
        "POST" "/_matrix/client/r0/refresh" "$refresh_data" "401" ""
}

# 测试10.3: 刷新令牌 - 缺少refresh_token
test_10_3() {
    local refresh_data="{}"
    run_test "10.3" "POST /_matrix/client/r0/refresh" "缺少refresh_token" \
        "POST" "/_matrix/client/r0/refresh" "$refresh_data" "400" ""
}

# 测试11.1: 登出 - 正常登出
test_11_1() {
    run_test "11.1" "POST /_matrix/client/r0/logout" "正常登出" \
        "POST" "/_matrix/client/r0/logout" "" "200" "$ACCESS_TOKEN"
}

# 测试11.2: 登出 - 无效令牌
test_11_2() {
    run_test "11.2" "POST /_matrix/client/r0/logout" "无效令牌" \
        "POST" "/_matrix/client/r0/logout" "" "401" "invalid_token"
}

# 测试11.3: 登出 - 缺少令牌
test_11_3() {
    run_test "11.3" "POST /_matrix/client/r0/logout" "缺少令牌" \
        "POST" "/_matrix/client/r0/logout" "" "401" ""
}

# 测试12.1: 全部登出 - 正常全部登出
test_12_1() {
    # 先重新登录以获取新令牌
    local login_data="{\"user\":\"$TEST_USER2\",\"password\":\"NewPassword456\"}"
    local output
    output=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$login_data" "$SERVER_URL/_matrix/client/r0/login")
    
    ACCESS_TOKEN=$(echo "$output" | jq -r '.access_token // empty')
    
    if [ -z "$ACCESS_TOKEN" ]; then
        print_error "重新登录失败，无法获取访问令牌"
        return 1
    fi
    
    run_test "12.1" "POST /_matrix/client/r0/logout/all" "正常全部登出" \
        "POST" "/_matrix/client/r0/logout/all" "" "200" "$ACCESS_TOKEN"
}

# 测试12.2: 全部登出 - 无效令牌
test_12_2() {
    run_test "12.2" "POST /_matrix/client/r0/logout/all" "无效令牌" \
        "POST" "/_matrix/client/r0/logout/all" "" "401" "invalid_token"
}

# 测试13.1: 停用账户 - 正常停用账户
test_13_1() {
    # 先重新登录
    local login_data="{\"user\":\"$TEST_USER2\",\"password\":\"NewPassword456\"}"
    local output
    output=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$login_data" "$SERVER_URL/_matrix/client/r0/login")
    
    ACCESS_TOKEN=$(echo "$output" | jq -r '.access_token // empty')
    
    run_test "13.1" "POST /_matrix/client/r0/account/deactivate" "正常停用账户" \
        "POST" "/_matrix/client/r0/account/deactivate" "" "200" "$ACCESS_TOKEN"
}

# 测试13.2: 停用账户 - 无效令牌
test_13_2() {
    run_test "13.2" "POST /_matrix/client/r0/account/deactivate" "无效令牌" \
        "POST" "/_matrix/client/r0/account/deactivate" "" "401" "invalid_token"
}

# 测试13.3: 停用账户 - 缺少令牌
test_13_3() {
    run_test "13.3" "POST /_matrix/client/r0/account/deactivate" "缺少令牌" \
        "POST" "/_matrix/client/r0/account/deactivate" "" "401" ""
}

###############################################################################
# 主测试流程
###############################################################################

main() {
    echo "========================================" | tee -a "$LOG_FILE"
    echo "Synapse Rust 认证模块测试" | tee -a "$LOG_FILE"
    echo "========================================" | tee -a "$LOG_FILE"
    echo "服务器URL: $SERVER_URL" | tee -a "$LOG_FILE"
    echo "服务器名称: $SERVER_NAME" | tee -a "$LOG_FILE"
    echo "测试时间: $(date -u +%Y-%m-%dT%H:%M:%SZ)" | tee -a "$LOG_FILE"
    echo "========================================" | tee -a "$LOG_FILE"
    echo "" | tee -a "$LOG_FILE"
    
    # 初始化结果文件
    init_results
    
    # 执行测试
    print_info "开始执行测试..."
    echo "" | tee -a "$LOG_FILE"
    
    # 1. 获取客户端版本
    print_info "=== 1. 获取客户端版本 ==="
    test_1_1
    test_1_2
    echo "" | tee -a "$LOG_FILE"
    
    # 2. 检查用户名可用性
    print_info "=== 2. 检查用户名可用性 ==="
    test_2_1
    test_2_2
    test_2_3
    echo "" | tee -a "$LOG_FILE"
    
    # 3. 用户注册
    print_info "=== 3. 用户注册 ==="
    test_3_1
    test_3_2
    test_3_3
    test_3_4
    test_3_5
    echo "" | tee -a "$LOG_FILE"
    
    # 4. 用户登录
    print_info "=== 4. 用户登录 ==="
    test_4_1
    test_4_2
    test_4_3
    test_4_4
    echo "" | tee -a "$LOG_FILE"
    
    # 5. 获取当前用户信息
    print_info "=== 5. 获取当前用户信息 ==="
    test_5_1
    test_5_2
    test_5_3
    echo "" | tee -a "$LOG_FILE"
    
    # 6. 获取用户资料
    print_info "=== 6. 获取用户资料 ==="
    test_6_1
    test_6_2
    test_6_3
    echo "" | tee -a "$LOG_FILE"
    
    # 7. 更新显示名称
    print_info "=== 7. 更新显示名称 ==="
    test_7_1
    test_7_2
    test_7_3
    echo "" | tee -a "$LOG_FILE"
    
    # 8. 更新头像
    print_info "=== 8. 更新头像 ==="
    test_8_1
    test_8_2
    test_8_3
    echo "" | tee -a "$LOG_FILE"
    
    # 9. 修改密码
    print_info "=== 9. 修改密码 ==="
    test_9_1
    test_9_2
    test_9_3
    test_9_4
    echo "" | tee -a "$LOG_FILE"
    
    # 10. 刷新令牌
    print_info "=== 10. 刷新令牌 ==="
    test_10_1
    test_10_2
    test_10_3
    echo "" | tee -a "$LOG_FILE"
    
    # 11. 登出
    print_info "=== 11. 登出 ==="
    test_11_1
    test_11_2
    test_11_3
    echo "" | tee -a "$LOG_FILE"
    
    # 12. 全部登出
    print_info "=== 12. 全部登出 ==="
    test_12_1
    test_12_2
    echo "" | tee -a "$LOG_FILE"
    
    # 13. 停用账户
    print_info "=== 13. 停用账户 ==="
    test_13_1
    test_13_2
    test_13_3
    echo "" | tee -a "$LOG_FILE"
    
    # 打印测试摘要
    print_info "========================================"
    print_info "测试摘要"
    print_info "========================================"
    print_info "总测试数: $TOTAL_TESTS"
    print_success "通过: $PASSED_TESTS"
    print_error "失败: $FAILED_TESTS"
    print_warning "跳过: $SKIPPED_TESTS"
    
    local success_rate=$(awk "BEGIN {printf \"%.1f\", ($PASSED_TESTS/$TOTAL_TESTS)*100}")
    print_info "通过率: $success_rate%"
    print_info "========================================"
    print_info "结果文件: $RESULT_FILE"
    print_info "日志文件: $LOG_FILE"
    print_info "========================================"
    
    # 根据测试结果返回退出码
    if [ $FAILED_TESTS -eq 0 ]; then
        print_success "所有测试通过!"
        exit 0
    else
        print_error "有 $FAILED_TESTS 个测试失败"
        exit 1
    fi
}

###############################################################################
# 脚本入口
###############################################################################

# 检查依赖
check_dependencies() {
    local missing_deps=()
    
    command -v curl >/dev/null 2>&1 || missing_deps+=("curl")
    command -v jq >/dev/null 2>&1 || missing_deps+=("jq")
    command -v python3 >/dev/null 2>&1 || missing_deps+=("python3")
    
    if [ ${#missing_deps[@]} -gt 0 ]; then
        print_error "缺少依赖: ${missing_deps[*]}"
        print_info "请安装缺少的依赖:"
        for dep in "${missing_deps[@]}"; do
            echo "  - $dep"
        done
        exit 1
    fi
}

# 检查依赖并运行主函数
check_dependencies
main
