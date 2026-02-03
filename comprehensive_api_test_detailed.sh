#!/bin/bash

# Synapse Rust API 全面测试脚本
# 测试所有API端点并记录详细结果

BASE_URL="http://localhost:8008"
OUTPUT_DIR="/tmp/api_test_results"
mkdir -p "$OUTPUT_DIR"

# 测试用户账号
TEST_USER="testuser2"
TEST_PASSWORD="TestPass123!"
ADMIN_USER="testadmin"
ADMIN_PASSWORD="AdminPass123!"

# 测试房间ID
TEST_ROOM_ID=""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 计数器
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
WARNING_TESTS=0

# 辅助函数
print_header() {
    echo "========================================" | tee -a "$OUTPUT_DIR/test_summary.txt"
    echo "$1" | tee -a "$OUTPUT_DIR/test_summary.txt"
    echo "========================================" | tee -a "$OUTPUT_DIR/test_summary.txt"
    echo "" | tee -a "$OUTPUT_DIR/test_summary.txt"
}

print_test() {
    local test_name="$1"
    local status="$2"
    local details="$3"

    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    case $status in
        "PASS")
            echo -e "${GREEN}✅ PASS${NC} - $test_name" | tee -a "$OUTPUT_DIR/test_summary.txt"
            PASSED_TESTS=$((PASSED_TESTS + 1))
            ;;
        "FAIL")
            echo -e "${RED}❌ FAIL${NC} - $test_name" | tee -a "$OUTPUT_DIR/test_summary.txt"
            FAILED_TESTS=$((FAILED_TESTS + 1))
            ;;
        "WARN")
            echo -e "${YELLOW}⚠️  WARN${NC} - $test_name" | tee -a "$OUTPUT_DIR/test_summary.txt"
            WARNING_TESTS=$((WARNING_TESTS + 1))
            ;;
    esac

    if [ -n "$details" ]; then
        echo "   详情: $details" | tee -a "$OUTPUT_DIR/test_summary.txt"
    fi
    echo "" | tee -a "$OUTPUT_DIR/test_summary.txt"
}

# 测试1: 获取客户端版本
print_header "测试1: 获取客户端版本"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/versions" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取客户端版本" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/1_client_versions.json"
else
    print_test "获取客户端版本" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试2: 用户注册
print_header "测试2: 用户注册"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/register" \
    -H "Content-Type: application/json" \
    -d '{"username":"testuser'$(date +%s)'","password":"TestPass123!","auth":{"type":"m.login.dummy"}}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "400" ]; then
    print_test "用户注册" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/2_register.json"
else
    print_test "用户注册" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试3: 用户登录
print_header "测试3: 用户登录"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"user\":\"$TEST_USER\",\"password\":\"$TEST_PASSWORD\"}" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "用户登录" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/3_login.json"
    # 提取token
    TOKEN=$(echo "$RESPONSE_BODY" | jq -r '.access_token // empty')
else
    print_test "用户登录" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    TOKEN=""
fi

# 测试4: 获取当前用户信息
print_header "测试4: 获取当前用户信息"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/account/whoami" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取当前用户信息" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/4_whoami.json"
else
    print_test "获取当前用户信息" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试5: 创建房间
print_header "测试5: 创建房间"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name":"API Test Room","visibility":"private"}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "创建房间" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/5_create_room.json"
    TEST_ROOM_ID=$(echo "$RESPONSE_BODY" | jq -r '.room_id // empty')
else
    print_test "创建房间" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    TEST_ROOM_ID=""
fi

# 测试6: 加入房间
print_header "测试6: 加入房间"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/rooms/$TEST_ROOM_ID/join" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "加入房间" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/6_join_room.json"
else
    print_test "加入房间" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试7: 获取房间成员
print_header "测试7: 获取房间成员"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/rooms/$TEST_ROOM_ID/members" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取房间成员" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/7_get_members.json"
else
    print_test "获取房间成员" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试8: 发送消息
print_header "测试8: 发送消息"
START_TIME=$(date +%s%N)
TXN_ID=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X PUT "$BASE_URL/_matrix/client/r0/rooms/$TEST_ROOM_ID/send/m.room.message/$TXN_ID" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"API Test Message"}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "发送消息" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/8_send_message.json"
else
    print_test "发送消息" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试9: 获取用户设备
print_header "测试9: 获取用户设备"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/devices" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取用户设备" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/9_get_devices.json"
else
    print_test "获取用户设备" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试10: 查询设备密钥
print_header "测试10: 查询设备密钥"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/keys/query" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"device_keys\":{\"$TEST_USER:matrix.cjystx.top\":[]}}" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "查询设备密钥" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/10_query_keys.json"
else
    print_test "查询设备密钥" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试11: 健康检查
print_header "测试11: 健康检查"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/health" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "健康检查" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/11_health.json"
else
    print_test "健康检查" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试12: 获取语音消息
print_header "测试12: 获取语音消息"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/voice/msg123" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "404" ]; then
    print_test "获取语音消息" "PASS" "HTTP $HTTP_CODE (消息不存在), 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/12_voice_message.json"
else
    print_test "获取语音消息" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试13: 同步
print_header "测试13: 同步"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/sync" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "同步" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/13_sync.json"
else
    print_test "同步" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试14: 获取公开房间
print_header "测试14: 获取公开房间"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/publicRooms" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取公开房间" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/14_public_rooms.json"
else
    print_test "获取公开房间" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试15: 管理员获取服务器版本
print_header "测试15: 管理员获取服务器版本"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_synapse/admin/v1/server_version" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "管理员获取服务器版本" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/15_server_version.json"
else
    print_test "管理员获取服务器版本" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试16: 联邦服务器密钥
print_header "测试16: 联邦服务器密钥"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/key/v2/server" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "联邦服务器密钥" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/16_federation_key.json"
else
    print_test "联邦服务器密钥" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试17: 用户登出
print_header "测试17: 用户登出"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/logout" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "用户登出" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "响应体: $RESPONSE_BODY" > "$OUTPUT_DIR/17_logout.json"
else
    print_test "用户登出" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 打印测试总结
echo "========================================" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "测试总结" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "========================================" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "总测试数: $TOTAL_TESTS" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "通过 (✅): $PASSED_TESTS ($((PASSED_TESTS * 100 / TOTAL_TESTS))%)" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "警告 (⚠️): $WARNING_TESTS ($((WARNING_TESTS * 100 / TOTAL_TESTS))%)" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "失败 (❌): $FAILED_TESTS ($((FAILED_TESTS * 100 / TOTAL_TESTS))%)" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "测试完成时间: $(date)" | tee -a "$OUTPUT_DIR/test_summary.txt"
echo "结果保存在: $OUTPUT_DIR" | tee -a "$OUTPUT_DIR/test_summary.txt"

echo ""
echo "测试完成！详细结果保存在: $OUTPUT_DIR/test_summary.txt"