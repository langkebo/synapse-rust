#!/bin/bash

# API测试脚本
BASE_URL="http://localhost:8008"
ACCESS_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdGxvZ2luOmxvY2FsaG9zdCIsInVzZXJfaWQiOiJAdGVzdGxvZ2luOmxvY2FsaG9zdCIsImFkbWluIjpmYWxzZSwiZXhwIjoxNzY5ODIxOTAwLCJpYXQiOjE3Njk3MzU1MDAsImRldmljZV9pZCI6IjV5ek5FQlNpSnpMdnNwQnQifQ.rNp9Ba0rnnZqHWO7rCu-5Hpc5MACOekm4wvC3Gzm-j4"

# 测试结果文件
TEST_RESULTS="/tmp/api_test_results.txt"
echo "API Test Results - $(date)" > "$TEST_RESULTS"
echo "======================================" >> "$TEST_RESULTS"

# 测试函数
test_api() {
    local name="$1"
    local method="$2"
    local url="$3"
    local data="$4"
    local auth="$5"
    
    echo -n "Testing $name... "
    
    if [ "$auth" = "true" ]; then
        response=$(curl -s -X "$method" "$BASE_URL$url" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer $ACCESS_TOKEN" \
            -d "$data" -w "\n%{http_code}" 2>/dev/null)
    else
        response=$(curl -s -X "$method" "$BASE_URL$url" \
            -H "Content-Type: application/json" \
            -d "$data" -w "\n%{http_code}" 2>/dev/null)
    fi
    
    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" = "200" ] || [ "$http_code" = "201" ]; then
        echo "✓ PASS (HTTP $http_code)"
        echo "$name: PASS (HTTP $http_code)" >> "$TEST_RESULTS"
        return 0
    else
        echo "✗ FAIL (HTTP $http_code)"
        echo "$name: FAIL (HTTP $http_code) - $body" >> "$TEST_RESULTS"
        return 1
    fi
}

echo "开始API测试..."
echo ""

# 1. 基础端点
test_api "Root Endpoint" "GET" "/" "" "false"
test_api "Client Versions" "GET" "/_matrix/client/versions" "" "false"

# 2. 认证相关
test_api "Register" "POST" "/_matrix/client/r0/register" '{"username":"testuser6","password":"TestPassword123!","auth":{"type":"m.login.dummy"}}' "false"
test_api "Login" "POST" "/_matrix/client/r0/login" '{"username":"testlogin","password":"TestPassword123!"}' "false"

# 3. 同步相关
test_api "Sync" "GET" "/_matrix/client/r0/sync" "" "true"

# 4. 房间相关
test_api "Create Room" "POST" "/_matrix/client/r0/createRoom" '{"name":"Test Room","preset":"public_chat"}' "true"
ROOM_ID='!testroom:localhost'
test_api "Join Room" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/join" '{}' "true"
test_api "Get Room Members" "GET" "/_matrix/client/r0/rooms/$ROOM_ID/members" "" "true"
test_api "Get Room State" "GET" "/_matrix/client/r0/rooms/$ROOM_ID/state" "" "true"

# 5. 设备相关
test_api "Get Devices" "GET" "/_matrix/client/r0/devices" "" "true"

# 6. 联邦API
test_api "Federation Version" "GET" "/_matrix/federation/v1/version" "" "false"

# 7. 好友管理API
test_api "Get Friends" "GET" "/_synapse/enhanced/friends/@testuser:localhost" "" "true"
test_api "Get Friend Requests" "GET" "/_synapse/enhanced/friend/requests/@testuser:localhost" "" "true"
test_api "Get Friend Categories" "GET" "/_synapse/enhanced/friend/categories/@testuser:localhost" "" "true"

# 8. 私聊管理API
test_api "Get Private Sessions" "GET" "/_synapse/enhanced/private/sessions" "" "true"
test_api "Get Unread Count" "GET" "/_synapse/enhanced/private/unread-count" "" "true"

# 9. 语音消息API
test_api "Get Voice Messages" "GET" "/_synapse/enhanced/voice/user/@testuser:localhost" "" "true"

# 10. Admin API
test_api "Admin Status" "GET" "/_synapse/admin/v1/status" "" "true"
test_api "Get Security Events" "GET" "/_synapse/admin/v1/security/events" "" "true"
test_api "Get IP Blocks" "GET" "/_synapse/admin/v1/security/ip/blocks" "" "true"

echo ""
echo "测试完成！详细结果保存在: $TEST_RESULTS"
cat "$TEST_RESULTS"