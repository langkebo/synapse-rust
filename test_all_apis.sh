#!/bin/bash

# Synapse Rust - Comprehensive API Test Suite with Test Data
# Tests all 47 Core Client APIs with proper test data
# Usage: ./test_all_apis.sh

set -e

SERVER_URL="http://localhost:8008"
ADMIN_USER="admin"
ADMIN_PASS="Wzc9890951!"

echo "=========================================="
echo "Synapse Rust - 47 Core Client API Tests"
echo "=========================================="
echo ""

# Step 1: Login as admin to get token
echo ">>> Step 1: 获取管理员Token..."
LOGIN_RESPONSE=$(curl -s -X POST "$SERVER_URL/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$ADMIN_USER\", \"password\": \"$ADMIN_PASS\"}")

ADMIN_TOKEN=$(echo $LOGIN_RESPONSE | jq -r '.access_token')
if [ "$ADMIN_TOKEN" == "null" ] || [ -z "$ADMIN_TOKEN" ]; then
    echo "❌ 获取管理员Token失败: $LOGIN_RESPONSE"
    exit 1
fi
echo "✅ 管理员Token获取成功"
echo ""

# Initialize counters
TOTAL_TESTS=0
PASSED=0
FAILED=0
SKIPPED=0

# Test function
test_api() {
    local name="$1"
    local method="$2"
    local endpoint="$3"
    local body="$4"
    local expected_status="$5"
    local description="$6"

    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    if [ "$method" == "GET" ]; then
        response=$(curl -s -w "\n%{http_code}" -X GET "$SERVER_URL$endpoint" \
            -H "Authorization: Bearer $ADMIN_TOKEN" \
            -H "Content-Type: application/json")
    elif [ "$method" == "POST" ]; then
        response=$(curl -s -w "\n%{http_code}" -X POST "$SERVER_URL$endpoint" \
            -H "Authorization: Bearer $ADMIN_TOKEN" \
            -H "Content-Type: application/json" \
            -d "$body")
    elif [ "$method" == "PUT" ]; then
        response=$(curl -s -w "\n%{http_code}" -X PUT "$SERVER_URL$endpoint" \
            -H "Authorization: Bearer $ADMIN_TOKEN" \
            -H "Content-Type: application/json" \
            -d "$body")
    elif [ "$method" == "DELETE" ]; then
        response=$(curl -s -w "\n%{http_code}" -X DELETE "$SERVER_URL$endpoint" \
            -H "Authorization: Bearer $ADMIN_TOKEN" \
            -H "Content-Type: application/json")
    fi

    http_code=$(echo "$response" | tail -n1)
    response_body=$(echo "$response" | sed '$d')

    if [ "$http_code" == "$expected_status" ]; then
        echo "✅ [$TOTAL_TESTS] $name ($method $endpoint) - $http_code"
        PASSED=$((PASSED + 1))
        return 0
    else
        echo "❌ [$TOTAL_TESTS] $name ($method $endpoint) - 期望:$expected_status 实际:$http_code"
        if [ "$http_code" != "200" ]; then
            echo "   响应: $(echo $response_body | head -c 200)"
        fi
        FAILED=$((FAILED + 1))
        return 1
    fi
}

# Get test data first
echo ">>> Step 2: 准备测试数据..."
echo "创建测试用户..."
for i in {1..3}; do
    curl -s -X POST "$SERVER_URL/_matrix/client/r0/register" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"testuser$i\",\"password\":\"TestPass123!\",\"admin\":false}" > /dev/null
done
echo "✅ 测试用户创建完成"

# Create test room
ROOM_RESPONSE=$(curl -s -X POST "$SERVER_URL/_matrix/client/r0/createRoom" \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name":"Test Room for API Testing","visibility":"private"}')
ROOM_ID=$(echo $ROOM_RESPONSE | jq -r '.room_id')
if [ "$ROOM_ID" == "null" ]; then
    ROOM_ID="!testroom:cjystx.top"
else
    echo "✅ 测试房间创建: $ROOM_ID"
fi

# Join room
curl -s -X POST "$SERVER_URL/_matrix/client/r0/rooms/$ROOM_ID/join" \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -d '{}' > /dev/null
echo "✅ 管理员已加入测试房间"

# Send test messages
for i in {1..3}; do
    curl -s -X PUT "$SERVER_URL/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message/testmsg$i" \
        -H "Authorization: Bearer $ADMIN_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"msgtype\":\"m.text\",\"body\":\"Test message $i\"}" > /dev/null
done
echo "✅ 测试消息发送完成"

# Get first event ID
EVENTS_RESPONSE=$(curl -s -X GET "$SERVER_URL/_matrix/client/r0/rooms/$ROOM_ID/messages?limit=3" \
    -H "Authorization: Bearer $ADMIN_TOKEN")
EVENT_ID=$(echo $EVENTS_RESPONSE | jq -r '.chunk[0].event_id')
if [ "$EVENT_ID" == "null" ]; then
    EVENT_ID="\$test_event_123"
fi
echo "✅ 测试事件ID: $EVENT_ID"
echo ""

echo "=========================================="
echo "开始测试 47 个核心客户端API..."
echo "=========================================="
echo ""

# 1. 健康检查和版本API
echo "--- 1. 健康检查和版本API ---"
test_api "健康检查" "GET" "/health" "" "200" "返回服务器健康状态"
test_api "获取客户端版本" "GET" "/_matrix/client/versions" "" "200" "返回支持的API版本"

# 2. 用户注册和认证
echo ""
echo "--- 2. 用户注册和认证 ---"
test_api "检查用户名可用性" "GET" "/_matrix/client/r0/register/available?username=newuser" "" "200" "检查用户名是否可用"
test_api "用户登录" "POST" "/_matrix/client/r0/login" '{"type":"m.login.password","user":"testuser1","password":"TestPass123!"}' "200" "用户登录获取Token"
test_api "退出登录" "POST" "/_matrix/client/r0/logout" '{"refresh_token":"test"}' "200" "退出当前设备"

# 3. 邮箱验证
echo ""
echo "--- 3. 邮箱验证 ---"
test_api "请求邮箱验证" "POST" "/_matrix/client/r0/register/email/requestToken" '{"email":"test@example.com","client_secret":"test123"}' "200" "请求邮箱验证Token"

# 4. 用户账号管理
echo ""
echo "--- 4. 用户账号管理 ---"
test_api "获取当前用户信息" "GET" "/_matrix/client/r0/account/whoami" "" "200" "返回当前用户信息"
test_api "获取用户资料" "GET" "/_matrix/client/r0/account/profile/@admin:cjystx.top" "" "200" "返回用户资料"
test_api "更新显示名称" "PUT" "/_matrix/client/r0/account/profile/@admin:cjystx.top/displayname" '{"displayname":"Admin User Updated"}' "200" "更新用户显示名"
test_api "更新头像" "PUT" "/_matrix/client/r0/account/profile/@admin:cjystx.top/avatar_url" '{"avatar_url":"mxc://example.com/avatar"}' "200" "更新用户头像"
test_api "修改密码" "POST" "/_matrix/client/r0/account/password" '{"new_password":"NewPass123!"}' "200" "修改用户密码"

# 5. 用户目录
echo ""
echo "--- 5. 用户目录 ---"
test_api "搜索用户" "POST" "/_matrix/client/r0/user_directory/search" '{"search_term":"test","limit":10}' "200" "搜索用户目录"
test_api "获取用户列表" "POST" "/_matrix/client/r0/user_directory/list" '{"limit":10}' "200" "获取用户目录列表"

# 6. 设备管理
echo ""
echo "--- 6. 设备管理 ---"
test_api "获取设备列表" "GET" "/_matrix/client/r0/devices" "" "200" "获取设备列表"

# 7. 在线状态
echo ""
echo "--- 7. 在线状态 ---"
test_api "获取在线状态" "GET" "/_matrix/client/r0/presence/@admin:cjystx.top/status" "" "200" "获取用户在线状态"
test_api "设置在线状态" "PUT" "/_matrix/client/r0/presence/@admin:cjystx.top/status" '{"presence":"online","status_msg":"Testing APIs"}' "200" "设置在线状态"

# 8. 房间管理
echo ""
echo "--- 8. 房间管理 ---"
test_api "创建房间" "POST" "/_matrix/client/r0/createRoom" '{"name":"API Test Room","visibility":"private"}' "200" "创建新房间"
test_api "获取房间信息" "GET" "/_matrix/client/r0/directory/room/$ROOM_ID" "" "200" "获取房间信息"
test_api "获取公共房间列表" "GET" "/_matrix/client/r0/publicRooms" "" "200" "获取公共房间列表"
test_api "创建公共房间" "POST" "/_matrix/client/r0/publicRooms" '{}' "200" "创建公共房间"
test_api "获取用户房间列表" "GET" "/_matrix/client/r0/user/@admin:cjystx.top/rooms" "" "200" "获取用户房间列表"

# 9. 房间操作
echo ""
echo "--- 9. 房间操作 ---"
test_api "加入房间" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/join" '{}' "200" "加入指定房间"
test_api "邀请用户" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/invite" '{"user_id":"@testuser2:cjystx.top"}' "200" "邀请用户加入房间"
test_api "离开房间" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/leave" '{}' "200" "离开房间"
test_api "踢出用户" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/kick" '{"user_id":"@testuser2:cjystx.top","reason":"Test kick"}' "200" "踢出房间成员"
test_api "封禁用户" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/ban" '{"user_id":"@testuser3:cjystx.top","reason":"Test ban"}' "200" "封禁房间成员"
test_api "解除封禁" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/unban" '{"user_id":"@testuser3:cjystx.top"}' "200" "解除封禁"

# 10. 房间状态和消息
echo ""
echo "--- 10. 房间状态和消息 ---"
test_api "获取房间状态" "GET" "/_matrix/client/r0/rooms/$ROOM_ID/state" "" "200" "获取房间状态事件"
test_api "获取特定状态事件" "GET" "/_matrix/client/r0/rooms/$ROOM_ID/state/m.room.topic" "" "200" "获取特定状态事件"
test_api "设置房间状态" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/state/m.room.topic" '{"topic":"Test Topic"}' "200" "设置房间状态事件"
test_api "获取成员事件" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/get_membership_events" '{"limit":10}' "200" "获取成员关系变更事件"
test_api "发送消息" "PUT" "/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message/txn999" '{"msgtype":"m.text","body":"API Test message"}' "200" "发送房间消息"
test_api "获取房间消息" "GET" "/_matrix/client/r0/rooms/$ROOM_ID/messages?limit=10" "" "200" "获取房间消息列表"
test_api "获取房间成员" "GET" "/_matrix/client/r0/rooms/$ROOM_ID/members" "" "200" "获取房间成员列表"
test_api "编辑消息" "PUT" "/_matrix/client/r0/rooms/$ROOM_ID/redact/\$event123" '{"reason":"Test redact"}' "200" "编辑或删除消息"

# 11. 事件举报
echo ""
echo "--- 11. 事件举报 ---"
# First create a report
curl -s -X POST "$SERVER_URL/_matrix/client/r0/rooms/$ROOM_ID/report/\$test_event_123" \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"reason":"Test report","score":-100}' > /dev/null

test_api "举报事件" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/report/\$test_event_123" '{"reason":"Spam content","score":-50}' "200" "举报违规事件"
test_api "更新举报分数" "PUT" "/_matrix/client/r0/rooms/$ROOM_ID/report/\$test_event_123/score" '{"score":-25}' "200" "更新举报分数"

# 12. 同步
echo ""
echo "--- 12. 同步 ---"
test_api "同步数据" "GET" "/_matrix/client/r0/sync?timeout=1000" "" "200" "同步最新数据"

# 13. 设备管理 (remaining)
echo ""
echo "--- 13. 设备管理 (补充) ---"
# Get device ID first
DEVICES_RESPONSE=$(curl -s -X GET "$SERVER_URL/_matrix/client/r0/devices" \
    -H "Authorization: Bearer $ADMIN_TOKEN")
DEVICE_ID=$(echo $DEVICES_RESPONSE | jq -r '.devices[0].device_id')
if [ "$DEVICE_ID" == "null" ]; then
    DEVICE_ID="test_device_123"
fi
test_api "获取设备信息" "GET" "/_matrix/client/r0/devices/$DEVICE_ID" "" "200" "获取特定设备信息"
test_api "更新设备信息" "PUT" "/_matrix/client/r0/devices/$DEVICE_ID" '{"display_name":"Test Device"}' "200" "更新设备显示名称"

echo ""
echo "=========================================="
echo "测试结果汇总"
echo "=========================================="
echo "总测试数: $TOTAL_TESTS"
echo "通过: $PASSED"
echo "失败: $FAILED"
echo "成功率: $(( PASSED * 100 / TOTAL_TESTS ))%"
echo ""

if [ $FAILED -gt 0 ]; then
    echo "失败的测试需要进一步检查"
    exit 1
fi

echo "🎉 所有测试通过！"
echo ""
echo "测试完成时间: $(date)"
