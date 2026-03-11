#!/bin/bash

# API 扩展测试脚本 - 详细功能测试
# 测试时间: 2026-03-10
# 测试环境: matrix.cjystx.top

BASE_URL="https://matrix.cjystx.top"
REPORT_FILE="/Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/api-test-extended.md"

TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

declare -a TEST_RESULTS=()

log_test() {
    local category=$1
    local endpoint=$2
    local method=$3
    local status=$4
    local response=$5
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ "$status" -eq 0 ]; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
        echo -e "${GREEN}[PASS]${NC} [$category] $method $endpoint"
        TEST_RESULTS+=("PASS|$category|$method $endpoint|")
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
        echo -e "${RED}[FAIL]${NC} [$category] $method $endpoint"
        TEST_RESULTS+=("FAIL|$category|$method $endpoint|$response")
    fi
}

test_get() {
    local category=$1
    local endpoint=$2
    local token=$3
    local expected_code=${4:-200}
    
    local response
    if [ -z "$token" ]; then
        response=$(curl -sk -w "\n%{http_code}" "$BASE_URL$endpoint" 2>/dev/null)
    else
        response=$(curl -sk -w "\n%{http_code}" -H "Authorization: Bearer $token" "$BASE_URL$endpoint" 2>/dev/null)
    fi
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$category" "$endpoint" "GET" 0 "$body"
        echo "$body"
        return 0
    else
        log_test "$category" "$endpoint" "GET" 1 "HTTP $http_code: ${body:0:200}"
        return 1
    fi
}

test_post() {
    local category=$1
    local endpoint=$2
    local token=$3
    local data=$4
    local expected_code=${5:-200}
    
    local response
    if [ -z "$token" ]; then
        response=$(curl -sk -w "\n%{http_code}" -X POST -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    else
        response=$(curl -sk -w "\n%{http_code}" -X POST -H "Content-Type: application/json" -H "Authorization: Bearer $token" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    fi
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$category" "$endpoint" "POST" 0 "$body"
        echo "$body"
        return 0
    else
        log_test "$category" "$endpoint" "POST" 1 "HTTP $http_code: ${body:0:200}"
        return 1
    fi
}

test_put() {
    local category=$1
    local endpoint=$2
    local token=$3
    local data=$4
    local expected_code=${5:-200}
    
    local response
    response=$(curl -sk -w "\n%{http_code}" -X PUT -H "Content-Type: application/json" -H "Authorization: Bearer $token" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$category" "$endpoint" "PUT" 0 "$body"
        echo "$body"
        return 0
    else
        log_test "$category" "$endpoint" "PUT" 1 "HTTP $http_code: ${body:0:200}"
        return 1
    fi
}

echo "========================================"
echo "API 扩展测试 - 详细功能验证"
echo "测试时间: $(date)"
echo "测试环境: $BASE_URL"
echo "========================================"
echo ""

# ========================================
# 1. 用户认证与账户设置
# ========================================
echo -e "${BLUE}[1/10] 用户认证与账户设置${NC}"
echo "----------------------------------------"

REGISTER_RESPONSE=$(test_post "AUTH" "/_matrix/client/v3/register" "" '{"username":"ext_test_'$(date +%s)'","password":"Test@123456","device_id":"EXT_TEST_DEVICE"}' 200)

if echo "$REGISTER_RESPONSE" | grep -q "access_token"; then
    USER_TOKEN=$(echo "$REGISTER_RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
    USER_ID=$(echo "$REGISTER_RESPONSE" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)
    DEVICE_ID=$(echo "$REGISTER_RESPONSE" | grep -o '"device_id":"[^"]*"' | cut -d'"' -f4)
    echo -e "${GREEN}用户注册成功: $USER_ID${NC}"
else
    echo -e "${YELLOW}尝试登录现有用户...${NC}"
    LOGIN_RESPONSE=$(curl -sk -X POST "$BASE_URL/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d '{"type":"m.login.password","user":"testuser","password":"Test@123456"}' 2>/dev/null)
    USER_TOKEN=$(echo "$LOGIN_RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
    USER_ID=$(echo "$LOGIN_RESPONSE" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)
fi

if [ -z "$USER_TOKEN" ]; then
    echo -e "${RED}认证失败，无法继续测试${NC}"
    exit 1
fi

echo ""

# 测试账户数据
test_get "ACCOUNT" "/_matrix/client/v3/user/$USER_ID/account_data/m.direct" "$USER_TOKEN" 200
test_put "ACCOUNT" "/_matrix/client/v3/user/$USER_ID/account_data/m.custom" "$USER_TOKEN" '{"custom_key":"custom_value"}' 200

echo ""

# ========================================
# 2. 房间管理 - 创建房间
# ========================================
echo -e "${BLUE}[2/10] 房间管理 - 创建房间${NC}"
echo "----------------------------------------"

CREATE_ROOM_RESPONSE=$(test_post "ROOM" "/_matrix/client/v3/createRoom" "$USER_TOKEN" '{"name":"Test Room '$(date +%s)'","topic":"API Test Room","preset":"private_chat","visibility":"private"}' 200)

if echo "$CREATE_ROOM_RESPONSE" | grep -q "room_id"; then
    ROOM_ID=$(echo "$CREATE_ROOM_RESPONSE" | grep -o '"room_id":"[^"]*"' | cut -d'"' -f4)
    echo -e "${GREEN}房间创建成功: $ROOM_ID${NC}"
else
    echo -e "${YELLOW}房间创建失败，使用测试房间ID${NC}"
    ROOM_ID="!test:cjystx.top"
fi

echo ""

# ========================================
# 3. 房间管理 - 房间状态
# ========================================
echo -e "${BLUE}[3/10] 房间管理 - 房间状态${NC}"
echo "----------------------------------------"

if [ -n "$ROOM_ID" ]; then
    test_get "ROOM" "/_matrix/client/v3/rooms/$ROOM_ID/state" "$USER_TOKEN" 200
    test_get "ROOM" "/_matrix/client/v3/rooms/$ROOM_ID/members" "$USER_TOKEN" 200
    test_get "ROOM" "/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=10" "$USER_TOKEN" 200
fi

echo ""

# ========================================
# 4. 消息发送
# ========================================
echo -e "${BLUE}[4/10] 消息发送${NC}"
echo "----------------------------------------"

if [ -n "$ROOM_ID" ]; then
    TXN_ID="txn_$(date +%s)"
    SEND_RESPONSE=$(test_put "MESSAGE" "/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/$TXN_ID" "$USER_TOKEN" '{"msgtype":"m.text","body":"Test message from API test"}' 200)
    
    if echo "$SEND_RESPONSE" | grep -q "event_id"; then
        EVENT_ID=$(echo "$SEND_RESPONSE" | grep -o '"event_id":"[^"]*"' | cut -d'"' -f4)
        echo -e "${GREEN}消息发送成功: $EVENT_ID${NC}"
    fi
fi

echo ""

# ========================================
# 5. 同步 API
# ========================================
echo -e "${BLUE}[5/10] 同步 API${NC}"
echo "----------------------------------------"

SYNC_RESPONSE=$(test_get "SYNC" "/_matrix/client/v3/sync?timeout=5000" "$USER_TOKEN" 200)

if echo "$SYNC_RESPONSE" | grep -q "next_batch"; then
    NEXT_BATCH=$(echo "$SYNC_RESPONSE" | grep -o '"next_batch":"[^"]*"' | cut -d'"' -f4)
    echo -e "${GREEN}同步成功, next_batch: ${NEXT_BATCH:0:30}...${NC}"
fi

echo ""

# ========================================
# 6. 过滤器
# ========================================
echo -e "${BLUE}[6/10] 过滤器${NC}"
echo "----------------------------------------"

FILTER_RESPONSE=$(test_post "FILTER" "/_matrix/client/v3/user/$USER_ID/filter" "$USER_TOKEN" '{"room":{"timeline":{"limit":50}}}' 200)

if echo "$FILTER_RESPONSE" | grep -q "filter_id"; then
    FILTER_ID=$(echo "$FILTER_RESPONSE" | grep -o '"filter_id":"[^"]*"' | cut -d'"' -f4)
    echo -e "${GREEN}过滤器创建成功: $FILTER_ID${NC}"
    
    test_get "FILTER" "/_matrix/client/v3/user/$USER_ID/filter/$FILTER_ID" "$USER_TOKEN" 200
fi

echo ""

# ========================================
# 7. 已读标记
# ========================================
echo -e "${BLUE}[7/10] 已读标记${NC}"
echo "----------------------------------------"

if [ -n "$ROOM_ID" ] && [ -n "$EVENT_ID" ]; then
    test_post "RECEIPT" "/_matrix/client/v3/rooms/$ROOM_ID/receipt/m.read/$EVENT_ID" "$USER_TOKEN" '{}' 200
    test_post "RECEIPT" "/_matrix/client/v3/rooms/$ROOM_ID/read_markers" "$USER_TOKEN" '{"m.fully_read":"'"$EVENT_ID"'","m.read":"'"$EVENT_ID"'"}' 200
fi

echo ""

# ========================================
# 8. 输入状态
# ========================================
echo -e "${BLUE}[8/10] 输入状态${NC}"
echo "----------------------------------------"

if [ -n "$ROOM_ID" ]; then
    test_put "TYPING" "/_matrix/client/v3/rooms/$ROOM_ID/typing/$USER_ID" "$USER_TOKEN" '{"typing":true,"timeout":30000}' 200
fi

echo ""

# ========================================
# 9. 管理后台扩展测试
# ========================================
echo -e "${BLUE}[9/10] 管理后台扩展测试${NC}"
echo "----------------------------------------"

test_get "ADMIN" "/_synapse/admin/v1/status" "$USER_TOKEN" 200
test_get "ADMIN" "/_synapse/admin/v1/config" "$USER_TOKEN" 200
test_get "ADMIN" "/_synapse/admin/v1/server_stats" "$USER_TOKEN" 200
test_get "ADMIN" "/_synapse/admin/v1/statistics" "$USER_TOKEN" 200
test_get "ADMIN" "/_synapse/admin/v1/user_stats" "$USER_TOKEN" 200
test_get "ADMIN" "/_synapse/admin/v1/media_stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 10. 设备管理扩展测试
# ========================================
echo -e "${BLUE}[10/10] 设备管理扩展测试${NC}"
echo "----------------------------------------"

if [ -n "$DEVICE_ID" ]; then
    test_get "DEVICE" "/_matrix/client/v3/devices/$DEVICE_ID" "$USER_TOKEN" 200
    test_put "DEVICE" "/_matrix/client/v3/devices/$DEVICE_ID" "$USER_TOKEN" '{"display_name":"Extended Test Device"}' 200
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

if [ $TOTAL_TESTS -gt 0 ]; then
    PASS_RATE=$((PASSED_TESTS * 100 / TOTAL_TESTS))
    echo "通过率: ${PASS_RATE}%"
fi

echo ""

# 生成报告
echo "生成详细测试报告..."

cat > "$REPORT_FILE" << EOF
# API 扩展测试报告

> 测试时间: $(date)
> 测试环境: $BASE_URL
> 用户: $USER_ID

---

## 测试统计

| 指标 | 数值 |
|------|------|
| 总测试数 | $TOTAL_TESTS |
| 通过数 | $PASSED_TESTS |
| 失败数 | $FAILED_TESTS |
| 通过率 | ${PASS_RATE}% |

---

## 测试详情

| 状态 | 类别 | 端点 | 备注 |
|------|------|------|------|
EOF

for item in "${TEST_RESULTS[@]}"; do
    IFS='|' read -r status category endpoint note <<< "$item"
    if [ "$status" = "PASS" ]; then
        echo "| ✅ | $category | $endpoint | - |" >> "$REPORT_FILE"
    else
        echo "| ❌ | $category | $endpoint | ${note:0:50} |" >> "$REPORT_FILE"
    fi
done

cat >> "$REPORT_FILE" << EOF

---

## 测试环境信息

- 服务器: $BASE_URL
- 用户ID: $USER_ID
- 设备ID: $DEVICE_ID
- 房间ID: $ROOM_ID

---

*报告生成时间: $(date)*
EOF

echo -e "${GREEN}测试报告已保存到: $REPORT_FILE${NC}"
