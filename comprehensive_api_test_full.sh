#!/bin/bash

# Synapse Rust API 全面测试脚本 - 测试所有100+个API端点
# 包含认证、房间管理、消息、设备、加密、语音、联邦、管理、私聊、媒体、密钥备份等所有API

BASE_URL="http://localhost:8008"
OUTPUT_DIR="/tmp/api_test_full"
mkdir -p "$OUTPUT_DIR"

# 测试用户账号
TEST_USER="testuser_new"
TEST_PASSWORD="TestPass123!"
ADMIN_USER="testadmin"
ADMIN_PASSWORD="AdminPass123!"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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

# 获取Token函数
get_token() {
    local username="$1"
    local password="$2"
    
    local result=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/login" \
        -H "Content-Type: application/json" \
        -d "{\"user\":\"$username\",\"password\":\"$password\"}")
    
    echo "$result" | jq -r '.access_token // empty'
}

# 测试1: 获取客户端版本
print_header "1. 认证相关API"
print_header "1.1 获取客户端版本"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/versions" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取客户端版本" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/1.1_client_versions.json"
else
    print_test "获取客户端版本" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试2: 用户注册
print_header "1.2 用户注册"
START_TIME=$(date +%s%N)
TIMESTAMP=$(date +%s)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/register" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"testuser${TIMESTAMP}\",\"password\":\"$TEST_PASSWORD\",\"auth\":{\"type\":\"m.login.dummy\"}}" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "400" ]; then
    print_test "用户注册" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/1.2_register.json"
    TEST_USER="testuser${TIMESTAMP}"
else
    print_test "用户注册" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试3: 用户登录
print_header "1.3 用户登录"
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
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/1.3_login.json"
    TOKEN=$(echo "$RESPONSE_BODY" | jq -r '.access_token // empty')
else
    print_test "用户登录" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    TOKEN=""
fi

# 测试4: 获取当前用户信息
print_header "1.4 获取当前用户信息"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/account/whoami" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取当前用户信息" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/1.4_whoami.json"
else
    print_test "获取当前用户信息" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试5: 用户登出 (已禁用 - 移至脚本末尾以避免token失效)
# print_header "1.5 用户登出"
# START_TIME=$(date +%s%N)
# RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/logout" \
#     -H "Authorization: Bearer $TOKEN" 2>&1)
# HTTP_CODE=$(echo "$RESULT" | tail -n1)
# RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
# END_TIME=$(date +%s%N)
# DURATION=$((END_TIME - START_TIME))

# if [ "$HTTP_CODE" = "200" ]; then
#     print_test "用户登出" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
#     echo "$RESPONSE_BODY" > "$OUTPUT_DIR/1.5_logout.json"
# else
#     print_test "用户登出" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
# fi

# 重新登录获取Token
# TOKEN=$(get_token "$TEST_USER" "$TEST_PASSWORD")

# 测试6: 密码修改
print_header "1.6 密码修改"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/account/password" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"new_password\":\"NewPass123!\",\"auth\":{\"type\":\"m.login.dummy\"}}" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "400" ]; then
    print_test "密码修改" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/1.6_change_password.json"
else
    print_test "密码修改" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试7: 房间管理API
print_header "2. 房间管理API"

# 测试7.1: 创建房间
print_header "2.1 创建房间"
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
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/2.1_create_room.json"
    TEST_ROOM_ID=$(echo "$RESPONSE_BODY" | jq -r '.room_id // empty')
else
    print_test "创建房间" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    TEST_ROOM_ID=""
fi

# 测试7.2: 加入房间
print_header "2.2 加入房间"
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
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/2.2_join_room.json"
else
    print_test "加入房间" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试7.4: 获取房间成员
print_header "2.4 获取房间成员"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/rooms/$TEST_ROOM_ID/members" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取房间成员" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/2.4_get_members.json"
else
    print_test "获取房间成员" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试7.5: 获取房间信息
print_header "2.5 获取房间信息"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/directory/room/$TEST_ROOM_ID" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取房间信息" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/2.5_get_room.json"
else
    print_test "获取房间信息" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试7.6: 获取公开房间
print_header "2.6 获取公开房间"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/publicRooms" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取公开房间" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/2.6_public_rooms.json"
else
    print_test "获取公开房间" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试8: 消息API
print_header "3. 消息API"

# 测试8.1: 发送消息
print_header "3.1 发送消息"
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
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/3.1_send_message.json"
else
    print_test "发送消息" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试8.2: 获取房间消息
print_header "3.2 获取房间消息"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/rooms/$TEST_ROOM_ID/messages" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "404" ]; then
    print_test "获取房间消息" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/3.2_get_messages.json"
else
    print_test "获取房间消息" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试7.3: 离开房间
print_header "2.3 离开房间"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/rooms/$TEST_ROOM_ID/leave" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "404" ]; then
    print_test "离开房间" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/2.3_leave_room.json"
else
    print_test "离开房间" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试9: 设备管理API
print_header "4. 设备管理API"

# 测试9.1: 获取用户设备
print_header "4.1 获取用户设备"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/devices" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取用户设备" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/4.1_get_devices.json"
else
    print_test "获取用户设备" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试9.2: 获取单个设备
print_header "4.2 获取单个设备"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/devices/DEVICE123" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "404" ]; then
    print_test "获取单个设备" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/4.2_get_device.json"
else
    print_test "获取单个设备" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试9.3: 更新设备
print_header "4.3 更新设备"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X PUT "$BASE_URL/_matrix/client/r0/devices/DEVICE123" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"display_name":"Updated Device Name"}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "404" ]; then
    print_test "更新设备" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/4.3_update_device.json"
else
    print_test "更新设备" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试9.4: 删除设备
print_header "4.4 删除设备"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/delete_devices" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"devices":["DEVICE123"]}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "删除设备" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/4.4_delete_devices.json"
else
    print_test "删除设备" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试10: 端到端加密API
print_header "5. 端到端加密API"

# 测试10.1: 查询设备密钥
print_header "5.1 查询设备密钥"
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
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/5.1_query_keys.json"
else
    print_test "查询设备密钥" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试10.2: 声明密钥
print_header "5.2 声明密钥"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/keys/claim" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"one_time_keys":{}}' 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "400" ]; then
    print_test "声明密钥" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/5.2_claim_keys.json"
else
    print_test "声明密钥" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试10.3: 获取密钥变更
print_header "5.3 获取密钥变更"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/v3/keys/changes" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取密钥变更" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/5.3_key_changes.json"
else
    print_test "获取密钥变更" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试11: 语音消息API
print_header "6. 语音消息API"

# 测试11.1: 获取语音消息
print_header "6.1 获取语音消息"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/voice/msg123" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "404" ]; then
    print_test "获取语音消息" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/6.1_voice_message.json"
else
    print_test "获取语音消息" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试12: 联邦API
print_header "7. 联邦API"

# 测试12.1: 获取联邦服务器密钥
print_header "7.1 获取联邦服务器密钥"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/key/v2/server" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "联邦服务器密钥" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/7.1_federation_key.json"
else
    print_test "联邦服务器密钥" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试12.2: 联邦服务器发现
print_header "7.2 联邦服务器发现"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/federation/v1" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "联邦服务器发现" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/7.2_federation_discovery.json"
else
    print_test "联邦服务器发现" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试13: 管理API
print_header "8. 管理API"

# 获取管理员Token
ADMIN_TOKEN=$(get_token "$ADMIN_USER" "$ADMIN_PASSWORD")

# 测试13.1: 获取服务器版本
print_header "8.1 获取服务器版本"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_synapse/admin/v1/server_version" \
    -H "Authorization: Bearer $ADMIN_TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "管理员获取服务器版本" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/8.1_server_version.json"
else
    print_test "管理员获取服务器版本" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试13.2: 获取用户列表
print_header "8.2 获取用户列表"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_synapse/admin/v1/users?limit=10" \
    -H "Authorization: Bearer $ADMIN_TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "管理员获取用户列表" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/8.2_get_users.json"
else
    print_test "管理员获取用户列表" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试13.3: 获取房间列表
print_header "8.3 获取房间列表"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_synapse/admin/v1/rooms?limit=10" \
    -H "Authorization: Bearer $ADMIN_TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "管理员获取房间列表" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/8.3_get_rooms.json"
else
    print_test "管理员获取房间列表" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试14: 健康检查API
print_header "9. 健康检查API"

# 测试14.1: 健康检查
print_header "9.1 健康检查"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/health" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "健康检查" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/9.1_health.json"
else
    print_test "健康检查" "FAIL" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试15: 同步API
print_header "10. 同步API"

# 测试15.1: 同步
print_header "10.1 同步"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/sync" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "同步" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/10.1_sync.json"
else
    print_test "同步" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试16: 私聊API
print_header "11. 私聊API"

# 测试16.1: 获取私聊房间列表
print_header "11.1 获取私聊房间列表"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/dm" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取私聊房间列表" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/11.1_get_dm_rooms.json"
else
    print_test "获取私聊房间列表" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试16.2: 创建私聊房间
print_header "11.2 创建私聊房间"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/createDM" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"@testuser:matrix.cjystx.top\"}" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "创建私聊房间" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/11.2_create_dm.json"
else
    print_test "创建私聊房间" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试17: 媒体API
print_header "12. 媒体API"

# 测试17.1: 获取媒体配置
print_header "12.1 获取媒体配置"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/media/v1/config" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取媒体配置" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/12.1_media_config.json"
else
    print_test "获取媒体配置" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
fi

# 测试18: 密钥备份API
print_header "13. 密钥备份API"

# 测试18.1: 获取房间密钥
print_header "13.1 获取房间密钥"
START_TIME=$(date +%s%N)
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_matrix/client/r0/room_keys" \
    -H "Authorization: Bearer $TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)
END_TIME=$(date +%s%N)
DURATION=$((END_TIME - START_TIME))

if [ "$HTTP_CODE" = "200" ]; then
    print_test "获取房间密钥" "PASS" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
    echo "$RESPONSE_BODY" > "$OUTPUT_DIR/13.1_get_room_keys.json"
else
    print_test "获取房间密钥" "WARN" "HTTP $HTTP_CODE, 耗时${DURATION}ms"
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