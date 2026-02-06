#!/bin/bash

# Synapse Rust - 47 Core Client API Test Suite
# 测试日期: 2026-02-04
# 更新日期: 2026-02-06 (动态获取token)

SERVER_URL="http://localhost:8008"
ADMIN_USER="admin"
ADMIN_PASS="Wzc9890951!"

echo "=========================================="
echo "Synapse Rust - 47 Core Client API Tests"
echo "=========================================="
echo ""

# Step 1: Login as admin to get token dynamically
echo ">>> Step 1: 获取管理员Token..."
LOGIN_RESPONSE=$(curl -s -X POST "$SERVER_URL/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$ADMIN_USER\", \"password\": \"$ADMIN_PASS\"}")

# Extract token from response
ADMIN_TOKEN=$(echo "$LOGIN_RESPONSE" | python3 -c "import sys, json; print(json.load(sys.stdin).get('access_token', ''))" 2>/dev/null)

if [ -z "$ADMIN_TOKEN" ]; then
    echo "❌ 无法获取管理员Token，登录响应: $LOGIN_RESPONSE"
    echo "使用备用Token进行测试..."
    ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46Y2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAYWRtaW46Y2p5c3R4LnRvcCIsImFkbWluIjp0cnVlLCJleHAiOjE3NzAyMjMyMjAsImlhdCI6MTc3MDIxOTYyMCwiZGV2aWNlX2lkIjoiT2xhVEt3WThoczQ3ODc2Z2g2VHNiZyJ9.E9N59jTn53KhSkRKml8cAyvQUtFe92sDvPbAd804o8c"
else
    echo "✅ 成功获取管理员Token"
fi

# Initialize counters
TOTAL_TESTS=0
PASSED=0
FAILED=0
FAILED_TESTS=""

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
    else
        echo "❌ [$TOTAL_TESTS] $name ($method $endpoint) - 期望:$expected_status 实际:$http_code"
        echo "   响应: $response_body"
        FAILED=$((FAILED + 1))
        FAILED_TESTS="${FAILED_TESTS}\n- $name ($method $endpoint): HTTP $http_code"
    fi
}

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
test_api "检查用户名可用性" "GET" "/_matrix/client/r0/register/available?username=testuser1" "" "200" "检查用户名是否可用"
test_api "用户注册" "POST" "/_matrix/client/r0/register" '{"username":"testuser1","password":"TestPass123!","admin":false}' "200" "创建新用户账户"
test_api "用户登录" "POST" "/_matrix/client/r0/login" '{"type":"m.login.password","user":"testuser1","password":"TestPass123!"}' "200" "用户登录获取Token"
test_api "退出登录" "POST" "/_matrix/client/r0/logout" '{"refresh_token":"test"}' "200" "退出当前设备"
test_api "退出所有设备" "POST" "/_matrix/client/r0/logout/all" '{}' "200" "退出所有设备"

# 3. 邮箱验证
echo ""
echo "--- 3. 邮箱验证 ---"
test_api "请求邮箱验证" "POST" "/_matrix/client/r0/register/email/requestToken" '{"email":"test@example.com","client_secret":"test123"}' "200" "请求邮箱验证Token"

# 4. 用户账号管理
echo ""
echo "--- 4. 用户账号管理 ---"
test_api "获取当前用户信息" "GET" "/_matrix/client/r0/account/whoami" "" "200" "返回当前用户信息"
test_api "获取用户资料" "GET" "/_matrix/client/r0/account/profile/@admin:cjystx.top" "" "200" "返回用户资料"
test_api "更新显示名称" "PUT" "/_matrix/client/r0/account/profile/@admin:cjystx.top/displayname" '{"displayname":"Admin User"}' "200" "更新用户显示名"
test_api "更新头像" "PUT" "/_matrix/client/r0/account/profile/@admin:cjystx.top/avatar_url" '{"avatar_url":"mxc://example.com/avatar"}' "200" "更新用户头像"
test_api "修改密码" "POST" "/_matrix/client/r0/account/password" '{"new_password":"NewPass123!"}' "200" "修改用户密码"
test_api "停用账户" "POST" "/_matrix/client/r0/account/deactivate" '{}' "200" "停用当前账户"

# 5. 用户目录
echo ""
echo "--- 5. 用户目录 ---"
test_api "搜索用户" "POST" "/_matrix/client/r0/user_directory/search" '{"search_term":"test","limit":10}' "200" "搜索用户目录"
test_api "获取用户列表" "POST" "/_matrix/client/r0/user_directory/list" '{"limit":10}' "200" "获取用户目录列表"

# 6. 设备管理
echo ""
echo "--- 6. 设备管理 ---"
test_api "获取设备列表" "GET" "/_matrix/client/r0/devices" "" "200" "获取设备列表"
test_api "获取设备信息" "GET" "/_matrix/client/r0/devices/test_device" "" "200" "获取特定设备信息"
test_api "更新设备信息" "PUT" "/_matrix/client/r0/devices/test_device" '{"display_name":"Test Device"}' "200" "更新设备显示名称"
test_api "删除设备" "DELETE" "/_matrix/client/r0/devices/test_device" "" "200" "删除设备"
test_api "批量删除设备" "POST" "/_matrix/client/r0/delete_devices" '{"devices":["dev1","dev2"]}' "200" "批量删除设备"

# 7. 在线状态
echo ""
echo "--- 7. 在线状态 ---"
test_api "获取在线状态" "GET" "/_matrix/client/r0/presence/@admin:cjystx.top/status" "" "200" "获取用户在线状态"
test_api "设置在线状态" "PUT" "/_matrix/client/r0/presence/@admin:cjystx.top/status" '{"presence":"online","status_msg":"测试中"}' "200" "设置在线状态"

# 8. 房间管理
echo ""
echo "--- 8. 房间管理 ---"
test_api "创建房间" "POST" "/_matrix/client/r0/createRoom" '{"name":"测试房间","visibility":"private"}' "200" "创建新房间"
test_api "获取房间信息" "GET" "/_matrix/client/r0/directory/room/!testroom:cjystx.top" "" "200" "获取房间信息"
test_api "获取公共房间列表" "GET" "/_matrix/client/r0/publicRooms" "" "200" "获取公共房间列表"
test_api "创建公共房间" "POST" "/_matrix/client/r0/publicRooms" '{}' "200" "创建公共房间"
test_api "获取用户房间列表" "GET" "/_matrix/client/r0/user/@admin:cjystx.top/rooms" "" "200" "获取用户房间列表"

# 9. 房间操作
echo ""
echo "--- 9. 房间操作 ---"
test_api "加入房间" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/join" '{}' "200" "加入指定房间"
test_api "离开房间" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/leave" '{}' "200" "离开房间"
test_api "邀请用户" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/invite" '{"user_id":"@testuser2:cjystx.top"}' "200" "邀请用户加入房间"
test_api "踢出用户" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/kick" '{"user_id":"@testuser2:cjystx.top","reason":"测试"}' "200" "踢出房间成员"
test_api "封禁用户" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/ban" '{"user_id":"@testuser3:cjystx.top","reason":"测试"}' "200" "封禁房间成员"
test_api "解除封禁" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/unban" '{"user_id":"@testuser3:cjystx.top"}' "200" "解除封禁"

# 10. 房间状态和消息
echo ""
echo "--- 10. 房间状态和消息 ---"
test_api "获取房间状态" "GET" "/_matrix/client/r0/rooms/!testroom:cjystx.top/state" "" "200" "获取房间状态事件"
test_api "获取特定状态事件" "GET" "/_matrix/client/r0/rooms/!testroom:cjystx.top/state/m.room.name" "" "200" "获取特定状态事件"
test_api "设置房间状态" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/state/m.room.topic" '{"topic":"测试主题"}' "200" "设置房间状态事件"
test_api "获取成员事件" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/get_membership_events" '{"limit":10}' "200" "获取成员关系变更事件"
test_api "发送消息" "PUT" "/_matrix/client/r0/rooms/!testroom:cjystx.top/send/m.room.message/txn123" '{"msgtype":"m.text","body":"测试消息"}' "200" "发送房间消息"
test_api "获取房间消息" "GET" "/_matrix/client/r0/rooms/!testroom:cjystx.top/messages?limit=10" "" "200" "获取房间消息列表"
test_api "获取房间成员" "GET" "/_matrix/client/r0/rooms/!testroom:cjystx.top/members" "" "200" "获取房间成员列表"
test_api "编辑消息" "PUT" "/_matrix/client/r0/rooms/!testroom:cjystx.top/redact/\$event123" '{"reason":"编辑"}' "200" "编辑或删除消息"

# 11. 事件举报
echo ""
echo "--- 11. 事件举报 ---"
test_api "举报事件" "POST" "/_matrix/client/r0/rooms/!testroom:cjystx.top/report/\$event123" '{"reason":"垃圾内容","score":-100}' "200" "举报违规事件"
test_api "更新举报分数" "PUT" "/_matrix/client/r0/rooms/!testroom:cjystx.top/report/\$event123/score" '{"score":-50}' "200" "更新举报分数"

# 12. 同步
echo ""
echo "--- 12. 同步 ---"
test_api "同步数据" "GET" "/_matrix/client/r0/sync?timeout=1000" "" "200" "同步最新数据"

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
    echo "失败的测试:"
    echo -e "$FAILED_TESTS"
fi

echo ""
echo "测试完成时间: $(date)"
