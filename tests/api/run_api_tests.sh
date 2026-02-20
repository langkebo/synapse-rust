#!/bin/bash

# Synapse Matrix Server - 自动化 API 测试脚本
# 服务器地址
SERVER="http://localhost:8008"

# 颜色输出
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 测试统计
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# 测试结果文件
RESULT_FILE="/tmp/synapse_test_results_$(date +%Y%m%d_%H%M%S).txt"
echo "Synapse Matrix Server API 测试报告" > "$RESULT_FILE"
echo "测试时间: $(date)" >> "$RESULT_FILE"
echo "==========================================" >> "$RESULT_FILE"
echo "" >> "$RESULT_FILE"

# 函数：执行测试
run_test() {
    local test_id=$1
    local test_name=$2
    local command=$3
    local expected=$4
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    echo -e "${YELLOW}[$test_id] $test_name${NC}"
    
    # 执行命令
    result=$(eval "$command" 2>&1)
    exit_code=$?
    
    # 检查结果
    if [ $exit_code -eq 0 ] && echo "$result" | grep -q "$expected"; then
        echo -e "${GREEN}✓ 通过${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
        echo "[$test_id] $test_name: 通过" >> "$RESULT_FILE"
    else
        echo -e "${RED}✗ 失败${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        echo "[$test_id] $test_name: 失败" >> "$RESULT_FILE"
        echo "  预期: $expected" >> "$RESULT_FILE"
        echo "  实际: $(echo "$result" | head -n 1)" >> "$RESULT_FILE"
    fi
    
    echo ""
}

echo "=========================================="
echo "Synapse Matrix Server - 自动化 API 测试"
echo "=========================================="
echo ""
echo "测试服务器: $SERVER"
echo "测试时间: $(date)"
echo ""

# ==========================================
# 模块 1: 基础服务 API 测试
# ==========================================
echo -e "${BLUE}模块 1: 基础服务 API 测试${NC}"
echo "----------------------------------------"

run_test "TC001" "健康检查" \
    "curl -s $SERVER/health" \
    "healthy"

run_test "TC002" "客户端版本" \
    "curl -s $SERVER/_matrix/client/versions" \
    "versions"

run_test "TC003" "服务器发现" \
    "curl -s $SERVER/.well-known/matrix/server" \
    "m.server"

run_test "TC004" "客户端发现" \
    "curl -s $SERVER/.well-known/matrix/client" \
    "m.homeserver"

run_test "TC005" "服务器版本" \
    "curl -s $SERVER/_matrix/federation/v1/version" \
    "server"

# ==========================================
# 模块 2: 用户注册与认证 API 测试
# ==========================================
echo -e "${BLUE}模块 2: 用户注册与认证 API 测试${NC}"
echo "----------------------------------------"

# 使用已有的测试账户
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46Y2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAYWRtaW46Y2p5c3R4LnRvcCIsImFkbWluIjpmYWxzZSwiZXhwIjoxNzcxMDQ2NjQ5LCJpYXQiOjE3NzEwNDMwNDksImRldmljZV9pZCI6IlRFU1RfREVWSUNFX2FkbWluIn0.HoSQO7Cv9j9IM8_gkA9P9HF2YNALTCTh9qlYqsf_sPQ"

run_test "TC006" "检查用户名可用性" \
    "curl -s $SERVER/_matrix/client/r0/register/available?username=newtestuser123" \
    "available"

run_test "TC007" "用户登录 (admin)" \
    "curl -s -X POST $SERVER/_matrix/client/r0/login -H 'Content-Type: application/json' -d '{\"type\":\"m.login.password\",\"user\":\"admin\",\"password\":\"Admin@123\"}'" \
    "access_token"

run_test "TC008" "错误密码登录" \
    "curl -s -X POST $SERVER/_matrix/client/r0/login -H 'Content-Type: application/json' -d '{\"type\":\"m.login.password\",\"user\":\"admin\",\"password\":\"wrongpassword\"}'" \
    "M_FORBIDDEN"

# ==========================================
# 模块 3: 账户管理 API 测试
# ==========================================
echo -e "${BLUE}模块 3: 账户管理 API 测试${NC}"
echo "----------------------------------------"

run_test "TC009" "获取当前用户信息" \
    "curl -s -X GET $SERVER/_matrix/client/r0/account/whoami -H 'Authorization: Bearer $ADMIN_TOKEN'" \
    "user_id"

run_test "TC010" "获取用户资料" \
    "curl -s -X GET $SERVER/_matrix/client/r0/profile/@admin:cjystx.top" \
    "displayname"

# ==========================================
# 模块 4: 设备管理 API 测试
# ==========================================
echo -e "${BLUE}模块 4: 设备管理 API 测试${NC}"
echo "----------------------------------------"

run_test "TC011" "获取设备列表" \
    "curl -s -X GET $SERVER/_matrix/client/r0/devices -H 'Authorization: Bearer $ADMIN_TOKEN'" \
    "devices"

# ==========================================
# 模块 5: 房间管理 API 测试
# ==========================================
echo -e "${BLUE}模块 5: 房间管理 API 测试${NC}"
echo "----------------------------------------"

run_test "TC012" "创建房间" \
    "curl -s -X POST $SERVER/_matrix/client/r0/createRoom -H 'Authorization: Bearer $ADMIN_TOKEN' -H 'Content-Type: application/json' -d '{\"name\":\"Test Room\",\"visibility\":\"public\",\"preset\":\"public_chat\"}'" \
    "room_id"

run_test "TC013" "获取公开房间列表" \
    "curl -s -X GET $SERVER/_matrix/client/r0/publicRooms" \
    "chunk"

# ==========================================
# 模块 6: 在线状态 API 测试
# ==========================================
echo -e "${BLUE}模块 6: 在线状态 API 测试${NC}"
echo "----------------------------------------"

run_test "TC014" "设置在线状态" \
    "curl -s -X PUT $SERVER/_matrix/client/r0/presence/@admin:cjystx.top/status -H 'Authorization: Bearer $ADMIN_TOKEN' -H 'Content-Type: application/json' -d '{\"presence\":\"online\",\"status_msg\":\"Testing\"}'" \
    "status_msg"

run_test "TC015" "获取在线状态" \
    "curl -s -X GET $SERVER/_matrix/client/r0/presence/@admin:cjystx.top/status -H 'Authorization: Bearer $ADMIN_TOKEN'" \
    "presence"

# ==========================================
# 模块 7: 用户目录 API 测试
# ==========================================
echo -e "${BLUE}模块 7: 用户目录 API 测试${NC}"
echo "----------------------------------------"

run_test "TC016" "搜索用户" \
    "curl -s -X POST $SERVER/_matrix/client/r0/user_directory/search -H 'Authorization: Bearer $ADMIN_TOKEN' -H 'Content-Type: application/json' -d '{\"search_term\":\"admin\"}'" \
    "results"

# ==========================================
# 模块 8: 同步 API 测试
# ==========================================
echo -e "${BLUE}模块 8: 同步 API 测试${NC}"
echo "----------------------------------------"

run_test "TC017" "同步数据" \
    "curl -s -X GET '$SERVER/_matrix/client/r0/sync?timeout=1000' -H 'Authorization: Bearer $ADMIN_TOKEN'" \
    "next_batch"

# ==========================================
# 模块 9: 媒体 API 测试
# ==========================================
echo -e "${BLUE}模块 9: 媒体 API 测试${NC}"
echo "----------------------------------------"

run_test "TC018" "上传媒体" \
    "curl -s -X POST $SERVER/_matrix/media/r0/upload -H 'Authorization: Bearer $ADMIN_TOKEN' -H 'Content-Type: text/plain' -d 'test content'" \
    "content_uri"

# ==========================================
# 模块 10: 联邦 API 测试
# ==========================================
echo -e "${BLUE}模块 10: 联邦 API 测试${NC}"
echo "----------------------------------------"

run_test "TC019" "获取服务器密钥" \
    "curl -s $SERVER/_matrix/key/v2/server" \
    "server_name"

run_test "TC020" "联邦版本" \
    "curl -s $SERVER/_matrix/federation/v1/version" \
    "Synapse Rust"

# ==========================================
# 测试总结
# ==========================================
echo "=========================================="
echo "测试总结"
echo "=========================================="
echo ""
echo "总测试数: $TOTAL_TESTS"
echo -e "通过: ${GREEN}$PASSED_TESTS${NC}"
echo -e "失败: ${RED}$FAILED_TESTS${NC}"
echo ""

# 计算通过率
if [ $TOTAL_TESTS -gt 0 ]; then
    pass_rate=$((PASSED_TESTS * 100 / TOTAL_TESTS))
    echo "通过率: $pass_rate%"
else
    echo "通过率: 0%"
fi

echo ""
echo "测试结果已保存到: $RESULT_FILE"
echo ""

# 保存测试总结到文件
echo "" >> "$RESULT_FILE"
echo "==========================================" >> "$RESULT_FILE"
echo "测试总结" >> "$RESULT_FILE"
echo "==========================================" >> "$RESULT_FILE"
echo "总测试数: $TOTAL_TESTS" >> "$RESULT_FILE"
echo "通过: $PASSED_TESTS" >> "$RESULT_FILE"
echo "失败: $FAILED_TESTS" >> "$RESULT_FILE"
echo "通过率: $pass_rate%" >> "$RESULT_FILE"

# 返回退出码
if [ $FAILED_TESTS -gt 0 ]; then
    exit 1
else
    exit 0
fi
