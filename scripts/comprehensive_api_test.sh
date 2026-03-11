#!/bin/bash

# Matrix Synapse API 全面测试脚本
# 测试日期: 2026-03-10

BASE_URL="http://localhost:8008"
ADMIN_URL="http://localhost:8008/_synapse/admin/v1"

# 测试账户凭证
USER1_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXJfYXBpXzE6bG9jYWxob3N0IiwidXNlcl9pZCI6IkB0ZXN0dXNlcl9hcGlfMTpsb2NhbGhvc3QiLCJqdGkiOiIxNTA0MjdkNS1mN2NkLTQzNjUtOWU0NC00NmNkNzc4MGE1MWIiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MzEzODkxNiwiaWF0IjoxNzczMTM1MzE2LCJkZXZpY2VfaWQiOiJBUElfVEVTVF9ERVZJQ0VfMSJ9.GoSXrc3obROjBHYGN2fxjRdschmNOU9ojk0hu7gJrlM"
USER1_ID="@testuser_api_1:localhost"
USER2_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXJfYXBpXzI6bG9jYWxob3N0IiwidXNlcl9pZCI6IkB0ZXN0dXNlcl9hcGlfMjpsb2NhbGhvc3QiLCJqdGkiOiI4YzBhNWVhOC1mMTQ0LTRiOTMtYmUzYi0yMWU5ZDEwMzk4MzQiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MzEzODkxOCwiaWF0IjoxNzczMTM1MzE4LCJkZXZpY2VfaWQiOiJBUElfVEVTVF9ERVZJQ0VfMiJ9.bdPWNU0JXQGtej6rpsbKGSVPO0S1hfwCCZlvcL5Mz48"
USER2_ID="@testuser_api_2:localhost"
USER3_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXJfYXBpXzM6bG9jYWxob3N0IiwidXNlcl9pZCI6IkB0ZXN0dXNlcl9hcGlfMzpsb2NhbGhvc3QiLCJqdGkiOiJhMzVhNDBhYy0yMjk1LTQyMWMtYTlmNS1mNGExYjVmMWI1ZGEiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MzEzODkyMSwiaWF0IjoxNzczMTM1MzIxLCJkZXZpY2VfaWQiOiJBUElfVEVTVF9ERVZJQ0VfMyJ9.hDcjzbb_hPiDNG-3mFM_4hES1FhiaqZwOVVlXpYfEHQ"
USER3_ID="@testuser_api_3:localhost"

# 测试房间
ROOM_ID="!k50IQ3FK4blLjQg42uvUiAlF:localhost"

# 统计变量
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# 测试结果文件
RESULT_FILE="/tmp/api_test_results_$(date +%Y%m%d_%H%M%S).txt"
echo "Matrix Synapse API 测试结果 - $(date)" > "$RESULT_FILE"
echo "======================================" >> "$RESULT_FILE"

# 测试函数
test_api() {
    local name="$1"
    local method="$2"
    local url="$3"
    local token="$4"
    local data="$5"
    local expected_code="$6"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    echo -n "测试 $TOTAL_TESTS: $name ... "
    
    local response
    local http_code
    
    if [ -n "$token" ]; then
        if [ -n "$data" ]; then
            response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" \
                -H "Authorization: Bearer $token" \
                -H "Content-Type: application/json" \
                -d "$data")
        else
            response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" \
                -H "Authorization: Bearer $token")
        fi
    else
        if [ -n "$data" ]; then
            response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" \
                -H "Content-Type: application/json" \
                -d "$data")
        else
            response=$(curl -s -w "\n%{http_code}" -X "$method" "$url")
        fi
    fi
    
    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | head -n -1)
    
    if [ "$http_code" = "$expected_code" ]; then
        echo "✅ PASS ($http_code)"
        echo "✅ $name - PASS ($http_code)" >> "$RESULT_FILE"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo "❌ FAIL ($http_code, expected $expected_code)"
        echo "❌ $name - FAIL ($http_code, expected $expected_code)" >> "$RESULT_FILE"
        echo "   Response: $body" >> "$RESULT_FILE"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
}

echo "========================================="
echo "开始 Matrix Synapse API 全面测试"
echo "========================================="
echo ""

# ============================================
# 1. 基础服务 API
# ============================================
echo ""
echo "=== 1. 基础服务 API ==="
echo ""

test_api "健康检查" "GET" "$BASE_URL/health" "" "" "" "200"
test_api "获取客户端版本 (r0)" "GET" "$BASE_URL/_matrix/client/r0/versions" "" "" "" "200"
test_api "获取客户端版本 (v3)" "GET" "$BASE_URL/_matrix/client/v3/versions" "" "" "" "200"
test_api "获取服务器版本" "GET" "$BASE_URL/_matrix/server_version" "" "" "" "200"
test_api "获取客户端能力 (r0)" "GET" "$BASE_URL/_matrix/client/r0/capabilities" "$USER1_TOKEN" "" "" "200"
test_api "获取客户端能力 (v3)" "GET" "$BASE_URL/_matrix/client/v3/capabilities" "$USER1_TOKEN" "" "" "200"
test_api "服务器发现" "GET" "$BASE_URL/.well-known/matrix/server" "" "" "" "200"
test_api "客户端发现" "GET" "$BASE_URL/.well-known/matrix/client" "" "" "" "200"

# ============================================
# 2. 用户认证 API
# ============================================
echo ""
echo "=== 2. 用户认证 API ==="
echo ""

test_api "获取登录流程 (r0)" "GET" "$BASE_URL/_matrix/client/r0/login" "" "" "" "200"
test_api "获取登录流程 (v3)" "GET" "$BASE_URL/_matrix/client/v3/login" "" "" "" "200"
test_api "用户登录 (testuser_api_1)" "POST" "$BASE_URL/_matrix/client/v3/login" "" "" '{"type":"m.login.password","user":"testuser_api_1","password":"Test@123456","device_id":"API_TEST_DEVICE_1"}' "200"
test_api "获取当前用户信息 (r0)" "GET" "$BASE_URL/_matrix/client/r0/account/whoami" "$USER1_TOKEN" "" "" "200"
test_api "获取当前用户信息 (v3)" "GET" "$BASE_URL/_matrix/client/v3/account/whoami" "$USER1_TOKEN" "" "" "200"

# ============================================
# 3. 账户管理 API
# ============================================
echo ""
echo "=== 3. 账户管理 API ==="
echo ""

test_api "获取用户资料" "GET" "$BASE_URL/_matrix/client/v3/profile/$USER1_ID" "$USER1_TOKEN" "" "" "200"
test_api "获取显示名" "GET" "$BASE_URL/_matrix/client/v3/profile/$USER1_ID/displayname" "$USER1_TOKEN" "" "" "200"
test_api "设置显示名" "PUT" "$BASE_URL/_matrix/client/v3/profile/$USER1_ID/displayname" "$USER1_TOKEN" '{"displayname":"API Test User 1"}' "200"
test_api "获取头像URL" "GET" "$BASE_URL/_matrix/client/v3/profile/$USER1_ID/avatar_url" "$USER1_TOKEN" "" "" "200"
test_api "获取第三方ID列表" "GET" "$BASE_URL/_matrix/client/v3/account/3pid" "$USER1_TOKEN" "" "" "200"

# ============================================
# 4. 房间管理 API
# ============================================
echo ""
echo "=== 4. 房间管理 API ==="
echo ""

test_api "创建房间 (私有)" "POST" "$BASE_URL/_matrix/client/v3/createRoom" "$USER1_TOKEN" '{"name":"API Private Room","preset":"private_chat","visibility":"private"}' "200"
test_api "创建房间 (公开)" "POST" "$BASE_URL/_matrix/client/v3/createRoom" "$USER2_TOKEN" '{"name":"API Public Room","preset":"public_chat","visibility":"public"}' "200"
test_api "获取已加入房间列表 (r0)" "GET" "$BASE_URL/_matrix/client/r0/joined_rooms" "$USER1_TOKEN" "" "" "200"
test_api "获取已加入房间列表 (v3)" "GET" "$BASE_URL/_matrix/client/v3/joined_rooms" "$USER1_TOKEN" "" "" "200"
test_api "获取房间信息" "GET" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID" "$USER1_TOKEN" "" "" "200"
test_api "获取房间状态" "GET" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/state" "$USER1_TOKEN" "" "" "200"
test_api "获取房间成员" "GET" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/members" "$USER1_TOKEN" "" "" "200"
test_api "获取已加入成员" "GET" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" "$USER1_TOKEN" "" "" "200"
test_api "邀请用户到房间" "POST" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/invite" "$USER1_TOKEN" '{"user_id":"'$USER2_ID'"}' "200"
test_api "加入房间" "POST" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/join" "$USER2_TOKEN" "" "200"
test_api "离开房间" "POST" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/leave" "$USER2_TOKEN" "" "200"
test_api "获取公开房间列表 (GET)" "GET" "$BASE_URL/_matrix/client/v3/publicRooms" "$USER1_TOKEN" "" "" "200"
test_api "搜索公开房间 (POST)" "POST" "$BASE_URL/_matrix/client/v3/publicRooms" "$USER1_TOKEN" '{"limit":10}' "200"

# ============================================
# 5. 消息发送 API
# ============================================
echo ""
echo "=== 5. 消息发送 API ==="
echo ""

TXN_ID="txn_$(date +%s%N)"
test_api "发送文本消息" "PUT" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/$TXN_ID" "$USER1_TOKEN" '{"msgtype":"m.text","body":"Hello from API test"}' "200"
test_api "获取房间消息" "GET" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=10" "$USER1_TOKEN" "" "" "200"
test_api "设置已读标记" "POST" "$BASE_URL/_matrix/client/v3/rooms/$ROOM_ID/receipt/m.read/$TXN_ID" "$USER1_TOKEN" "" "200"

# ============================================
# 6. 设备管理 API
# ============================================
echo ""
echo "=== 6. 设备管理 API ==="
echo ""

test_api "获取设备列表 (r0)" "GET" "$BASE_URL/_matrix/client/r0/devices" "$USER1_TOKEN" "" "" "200"
test_api "获取设备列表 (v3)" "GET" "$BASE_URL/_matrix/client/v3/devices" "$USER1_TOKEN" "" "" "200"

# ============================================
# 7. 推送通知 API
# ============================================
echo ""
echo "=== 7. 推送通知 API ==="
echo ""

test_api "获取推送规则" "GET" "$BASE_URL/_matrix/client/v3/pushrules" "$USER1_TOKEN" "" "" "200"
test_api "获取通知列表" "GET" "$BASE_URL/_matrix/client/v3/notifications" "$USER1_TOKEN" "" "" "200"

# ============================================
# 8. E2EE 加密 API
# ============================================
echo ""
echo "=== 8. E2EE 加密 API ==="
echo ""

test_api "上传设备密钥 (r0)" "POST" "$BASE_URL/_matrix/client/r0/keys/upload" "$USER1_TOKEN" '{"device_keys":{"'$USER1_ID'":{"'$USER1_ID'":{"algorithms":["m.olm.v1.curve25519-aes-sha2","m.megolm.v1.aes-sha2"],"keys":{"ed25519:ABCDEF":"ABCDEF"},"device_id":"'$USER1_ID'","user_id":"'$USER1_ID'"}}}' "200"
test_api "查询设备密钥 (r0)" "POST" "$BASE_URL/_matrix/client/r0/keys/query" "$USER1_TOKEN" '{"device_keys":{"'$USER1_ID':[]}}' "200"

# ============================================
# 9. 媒体服务 API
# ============================================
echo ""
echo "=== 9. 媒体服务 API ==="
echo ""

test_api "获取媒体配置" "GET" "$BASE_URL/_matrix/media/r0/config" "$USER1_TOKEN" "" "" "200"

# ============================================
# 10. 好友系统 API
# ============================================
echo ""
echo "=== 10. 好友系统 API ==="
echo ""

test_api "获取好友列表" "GET" "$BASE_URL/_matrix/client/v1/friends" "$USER1_TOKEN" "" "" "200"

# ============================================
# 11. 同步 API
# ============================================
echo ""
echo "=== 11. 同步 API ==="
echo ""

test_api "同步 (GET)" "GET" "$BASE_URL/_matrix/client/v3/sync" "$USER1_TOKEN" "" "" "200"

# ============================================
# 12. VoIP 服务 API
# ============================================
echo ""
echo "=== 12. VoIP 服务 API ==="
echo ""

test_api "获取TURN服务器 (GET)" "GET" "$BASE_URL/_matrix/client/v3/voip/turnServer" "$USER1_TOKEN" "" "" "200"
test_api "获取VoIP配置" "GET" "$BASE_URL/_matrix/client/v3/voip/config" "$USER1_TOKEN" "" "" "200"

# ============================================
# 13. 搜索服务 API
# ============================================
echo ""
echo "=== 13. 搜索服务 API ==="
echo ""

test_api "搜索消息" "POST" "$BASE_URL/_matrix/client/v3/search" "$USER1_TOKEN" '{"search_categories":{"room_events":{"search_term":"hello"}}}' "200"

# ============================================
# 14. 管理后台 API
# ============================================
echo ""
echo "=== 14. 管理后台 API ==="
echo ""

# 注意：需要管理员token，这里使用普通token测试，预期403
test_api "获取服务器版本" "GET" "$ADMIN_URL/server_version" "$USER1_TOKEN" "" "" "403"
test_api "获取服务器统计" "GET" "$ADMIN_URL/server_stats" "$USER1_TOKEN" "" "" "403"
test_api "获取用户列表" "GET" "$ADMIN_URL/users?limit=10" "$USER1_TOKEN" "" "" "403"
test_api "获取房间列表" "GET" "$ADMIN_URL/rooms?limit=10" "$USER1_TOKEN" "" "" "403"

# ============================================
# 15. 联邦 API
# ============================================
echo ""
echo "=== 15. 联邦 API ==="
echo ""

test_api "获取服务器密钥" "GET" "$BASE_URL/_matrix/federation/v2/server" "" "" "" "200"
test_api "获取服务器版本" "GET" "$BASE_URL/_matrix/federation/v1/version" "" "" "" "200"

# ============================================
# 16. 账户数据 API
# ============================================
echo ""
echo "=== 16. 账户数据 API ==="
echo ""

test_api "获取账户数据" "GET" "$BASE_URL/_matrix/client/v3/user/$USER1_ID/account_data/m.fully_read" "$USER1_TOKEN" "" "" "200"
test_api "设置账户数据" "PUT" "$BASE_URL/_matrix/client/v3/user/$USER1_ID/account_data/m.fully_read" "$USER1_TOKEN" '{"version":"1"}' "200"

# ============================================
# 17. 密钥备份 API
# ============================================
echo ""
echo "=== 17. 密钥备份 API ==="
echo ""

test_api "获取当前备份版本" "GET" "$BASE_URL/_matrix/client/v3/room_keys/version" "$USER1_TOKEN" "" "" "200"

# ============================================
# 18. 注册令牌 API
# ============================================
echo ""
echo "=== 18. 注册令牌 API ==="
echo ""

test_api "验证注册令牌" "POST" "$BASE_URL/_synapse/admin/v1/registration_tokens/validate" "$USER1_TOKEN" '{"token":"test_token"}' "400"

# ============================================
# 19. 速率限制管理 API
# ============================================
echo ""
echo "=== 19. 速率限制管理 API ==="
echo ""

test_api "获取速率限制" "GET" "$BASE_URL/_synapse/admin/v1/rate_limits" "$USER1_TOKEN" "" "" "403"

# ============================================
# 20. Sliding Sync API
# ============================================
echo ""
echo "=== 20. Sliding Sync API ==="
echo ""

test_api "Sliding Sync" "GET" "$BASE_URL/_matrix/client/unstable/org.matrix.msc3575/sync" "$USER1_TOKEN" "" "" "200"

# ============================================
# 测试总结
# ============================================
echo ""
echo "========================================="
echo "测试总结"
echo "========================================="
echo "总测试数: $TOTAL_TESTS"
echo "通过: $PASSED_TESTS"
echo "失败: $FAILED_TESTS"
echo "通过率: $(awk "BEGIN {printf \"%.2f%%\", ($PASSED_TESTS/$TOTAL_TESTS)*100}")"
echo ""
echo "详细结果已保存到: $RESULT_FILE"

# 保存总结到结果文件
echo "" >> "$RESULT_FILE"
echo "=========================================" >> "$RESULT_FILE"
echo "测试总结" >> "$RESULT_FILE"
echo "=========================================" >> "$RESULT_FILE"
echo "总测试数: $TOTAL_TESTS" >> "$RESULT_FILE"
echo "通过: $PASSED_TESTS" >> "$RESULT_FILE"
echo "失败: $FAILED_TESTS" >> "$RESULT_FILE"
echo "通过率: $(awk "BEGIN {printf \"%.2f%%\", ($PASSED_TESTS/$TOTAL_TESTS)*100}")" >> "$RESULT_FILE"

exit 0
