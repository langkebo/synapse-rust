#!/bin/bash

# API 全面系统性测试脚本 - cjystx.top 配置验证
# 测试时间: 2026-03-10
# 测试环境: matrix.cjystx.top

BASE_URL="https://matrix.cjystx.top"
DISCOVERY_URL="https://cjystx.top"
ERROR_FILE="/Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/api-error.md"

# 测试统计
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 失败端点记录
declare -a FAILED_ENDPOINTS=()

# 用户 Token (将在登录后设置)
USER_TOKEN=""
USER_ID=""
DEVICE_ID=""

log_test() {
    local endpoint=$1
    local method=$2
    local status=$3
    local response=$4
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ "$status" -eq 0 ]; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
        echo -e "${GREEN}[PASS]${NC} $method $endpoint"
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
        echo -e "${RED}[FAIL]${NC} $method $endpoint"
        FAILED_ENDPOINTS+=("$method $endpoint|$response")
    fi
}

test_get() {
    local endpoint=$1
    local token=$2
    local expected_code=${3:-200}
    
    local response
    if [ -z "$token" ]; then
        response=$(curl -sk -w "\n%{http_code}" "$BASE_URL$endpoint" 2>/dev/null)
    else
        response=$(curl -sk -w "\n%{http_code}" -H "Authorization: Bearer $token" "$BASE_URL$endpoint" 2>/dev/null)
    fi
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$endpoint" "GET" 0 "$body"
        return 0
    else
        log_test "$endpoint" "GET" 1 "HTTP $http_code: $body"
        return 1
    fi
}

test_post() {
    local endpoint=$1
    local token=$2
    local data=$3
    local expected_code=${4:-200}
    
    local response
    if [ -z "$token" ]; then
        response=$(curl -sk -w "\n%{http_code}" -X POST -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    else
        response=$(curl -sk -w "\n%{http_code}" -X POST -H "Content-Type: application/json" -H "Authorization: Bearer $token" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    fi
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$endpoint" "POST" 0 "$body"
        echo "$body"
        return 0
    else
        log_test "$endpoint" "POST" 1 "HTTP $http_code: $body"
        return 1
    fi
}

test_discovery() {
    local endpoint=$1
    local expected=$2
    
    local response
    response=$(curl -sk -w "\n%{http_code}" "$DISCOVERY_URL$endpoint" 2>/dev/null)
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ "$http_code" -eq 200 ] && echo "$body" | grep -q "$expected"; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
        echo -e "${GREEN}[PASS]${NC} DISCOVERY $endpoint"
        echo "  Response: $body"
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
        echo -e "${RED}[FAIL]${NC} DISCOVERY $endpoint"
        echo "  HTTP $http_code: $body"
        FAILED_ENDPOINTS+=("DISCOVERY $endpoint|HTTP $http_code: $body")
    fi
}

echo "========================================"
echo "API 全面系统性测试 - cjystx.top"
echo "测试时间: $(date)"
echo "测试环境: $BASE_URL"
echo "========================================"
echo ""

# ========================================
# 0. 服务发现测试
# ========================================
echo -e "${YELLOW}[0/15] 服务发现测试${NC}"
echo "----------------------------------------"

test_discovery "/.well-known/matrix/server" "matrix.cjystx.top"
test_discovery "/.well-known/matrix/client" "matrix.cjystx.top"

echo ""

# ========================================
# 1. 基础服务 API 测试
# ========================================
echo -e "${YELLOW}[1/15] 基础服务 API 测试${NC}"
echo "----------------------------------------"

test_get "/health" "" 200
test_get "/_matrix/client/versions" "" 200
test_get "/_matrix/client/v3/capabilities" "" 200
test_get "/_matrix/federation/v1/version" "" 200

echo ""

# ========================================
# 2. 用户认证 API 测试
# ========================================
echo -e "${YELLOW}[2/15] 用户认证 API 测试${NC}"
echo "----------------------------------------"

# 测试登录流程
test_get "/_matrix/client/v3/login" "" 200

# 注册测试用户
echo "注册测试用户..."
REGISTER_RESPONSE=$(test_post "/_matrix/client/v3/register" "" '{"username":"testuser_'$(date +%s)'","password":"Test@123456","device_id":"TEST_DEVICE"}' 200)

if echo "$REGISTER_RESPONSE" | grep -q "access_token"; then
    USER_TOKEN=$(echo "$REGISTER_RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
    USER_ID=$(echo "$REGISTER_RESPONSE" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)
    DEVICE_ID=$(echo "$REGISTER_RESPONSE" | grep -o '"device_id":"[^"]*"' | cut -d'"' -f4)
    echo "用户注册成功: $USER_ID"
    echo "Token: ${USER_TOKEN:0:50}..."
else
    echo "用户注册失败，尝试登录现有用户..."
    LOGIN_RESPONSE=$(curl -sk -X POST "$BASE_URL/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d '{"type":"m.login.password","user":"testuser","password":"Test@123456"}' 2>/dev/null)
    USER_TOKEN=$(echo "$LOGIN_RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
    USER_ID=$(echo "$LOGIN_RESPONSE" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)
fi

if [ -n "$USER_TOKEN" ]; then
    echo -e "${GREEN}认证成功，继续测试...${NC}"
else
    echo -e "${RED}认证失败，部分测试将跳过${NC}"
fi

echo ""

# 测试 whoami
if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v3/account/whoami" "$USER_TOKEN" 200
fi

echo ""

# ========================================
# 3. 账户管理 API 测试
# ========================================
echo -e "${YELLOW}[3/15] 账户管理 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ] && [ -n "$USER_ID" ]; then
    test_get "/_matrix/client/v3/profile/$USER_ID" "$USER_TOKEN" 200
    test_get "/_matrix/client/v3/account/3pid" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 4. 房间管理 API 测试
# ========================================
echo -e "${YELLOW}[4/15] 房间管理 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/v3/publicRooms" "" 200

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v3/joined_rooms" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 5. 设备管理 API 测试
# ========================================
echo -e "${YELLOW}[5/15] 设备管理 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v3/devices" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 6. 推送通知 API 测试
# ========================================
echo -e "${YELLOW}[6/15] 推送通知 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v3/pushers" "$USER_TOKEN" 200
    test_get "/_matrix/client/v3/pushrules" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 7. E2EE 加密 API 测试
# ========================================
echo -e "${YELLOW}[7/15] E2EE 加密 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_post "/_matrix/client/v3/keys/upload" "$USER_TOKEN" '{"device_keys":{}}' 200
    test_post "/_matrix/client/v3/keys/query" "$USER_TOKEN" '{"device_keys":{}}' 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 8. 媒体服务 API 测试
# ========================================
echo -e "${YELLOW}[8/15] 媒体服务 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/media/v3/config" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 9. 好友系统 API 测试
# ========================================
echo -e "${YELLOW}[9/15] 好友系统 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v1/friends" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 10. 搜索服务 API 测试
# ========================================
echo -e "${YELLOW}[10/15] 搜索服务 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_post "/_matrix/client/v3/search" "$USER_TOKEN" '{"search_categories":{"room_events":{"search_term":"test"}}}' 200
    test_post "/_matrix/client/v3/user_directory/search" "$USER_TOKEN" '{"search_term":"test"}' 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 11. VoIP 服务 API 测试
# ========================================
echo -e "${YELLOW}[11/15] VoIP 服务 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v3/voip/turnServer" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 12. Space 空间 API 测试
# ========================================
echo -e "${YELLOW}[12/15] Space 空间 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v1/spaces/public" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 13. 联邦 API 测试
# ========================================
echo -e "${YELLOW}[13/15] 联邦 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/key/v2/server" "" 200

echo ""

# ========================================
# 14. 管理后台 API 测试
# ========================================
echo -e "${YELLOW}[14/15] 管理后台 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_synapse/admin/v1/server_version" "$USER_TOKEN" 200
    test_get "/_synapse/admin/v1/users" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 15. 其他 API 测试
# ========================================
echo -e "${YELLOW}[15/15] 其他 API 测试${NC}"
echo "----------------------------------------"

if [ -n "$USER_TOKEN" ]; then
    test_get "/_matrix/client/v3/notifications" "$USER_TOKEN" 200
else
    echo "[SKIP] 需要认证"
fi

echo ""

# ========================================
# 测试结果汇总
# ========================================
echo "========================================"
echo "测试结果汇总"
echo "========================================"
echo ""
echo "总测试数: $TOTAL_TESTS"
echo -e "通过: ${GREEN}$PASSED_TESTS${NC}"
echo -e "失败: ${RED}$FAILED_TESTS${NC}"
echo ""

if [ $FAILED_TESTS -gt 0 ]; then
    echo -e "${RED}失败的端点:${NC}"
    for item in "${FAILED_ENDPOINTS[@]}"; do
        IFS='|' read -r endpoint response <<< "$item"
        echo "  - $endpoint"
        echo "    响应: $response"
    done
fi

echo ""
echo "测试完成时间: $(date)"

# 计算通过率
if [ $TOTAL_TESTS -gt 0 ]; then
    PASS_RATE=$((PASSED_TESTS * 100 / TOTAL_TESTS))
    echo ""
    echo "通过率: ${PASS_RATE}%"
fi
