#!/bin/bash

# Synapse Rust API 测试脚本
# 测试所有140个API端点

BASE_URL="http://localhost:8008"
TEST_USER_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXI6bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQHRlc3R1c2VyOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOmZhbHNlLCJleHAiOjE3NzAwOTI0MjEsImlhdCI6MTc3MDAwNjAyMSwiZGV2aWNlX2lkIjoibGNTOExhYXcwMWZHL1UrRW9SOHdIUT09In0.IMBfyvStKRfYvMB3bNM2-9UX1iHk1_qdsF-w4o7Ivpc"
TEST_ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0YWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMDkyNDI2LCJpYXQiOjE3NzAwMDYwMjYsImRldmljZV9pZCI6IkQwaTlPUzNHcnpuN0FsczNNbldPVWc9PSJ9.hQJjLomObejQQBA7y0FCU6ArZz7K7-lF_SZXRzkUKaA"
TEST_USER_ID="@testuser:matrix.cjystx.top"
TEST_ADMIN_ID="@testadmin:matrix.cjystx.top"

OUTPUT_FILE="/tmp/api_test_results.txt"

echo "=== Synapse Rust API 测试结果 ===" > "$OUTPUT_FILE"
echo "测试时间: $(date)" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# 测试计数器
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
WARN_TESTS=0

# 测试函数
test_api() {
    local name="$1"
    local method="$2"
    local url="$3"
    local token="$4"
    local data="$5"
    local description="$6"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    echo "测试 $TOTAL_TESTS: $name" >> "$OUTPUT_FILE"
    echo "  方法: $method" >> "$OUTPUT_FILE"
    echo "  URL: $url" >> "$OUTPUT_FILE"
    echo "  描述: $description" >> "$OUTPUT_FILE"
    
    if [ -n "$data" ]; then
        echo "  请求体: $data" >> "$OUTPUT_FILE"
    fi
    
    # 构建curl命令
    local curl_cmd="curl -s -w '\nHTTP_CODE:%{http_code}\nTIME_TOTAL:%{time_total}' -X $method '$url'"
    
    if [ -n "$token" ]; then
        curl_cmd="$curl_cmd -H 'Authorization: Bearer $token'"
    fi
    
    curl_cmd="$curl_cmd -H 'Content-Type: application/json'"
    
    if [ -n "$data" ]; then
        curl_cmd="$curl_cmd -d '$data'"
    fi
    
    # 执行测试
    local response=$(eval $curl_cmd)
    local http_code=$(echo "$response" | grep "HTTP_CODE:" | cut -d':' -f2)
    local time_total=$(echo "$response" | grep "TIME_TOTAL:" | cut -d':' -f2)
    local body=$(echo "$response" | grep -v "HTTP_CODE:" | grep -v "TIME_TOTAL:")
    
    echo "  HTTP状态码: $http_code" >> "$OUTPUT_FILE"
    echo "  响应时间: ${time_total}s" >> "$OUTPUT_FILE"
    echo "  响应体: $body" >> "$OUTPUT_FILE"
    
    # 判断测试结果
    local status="✅ OK"
    if [ "$http_code" = "200" ] || [ "$http_code" = "201" ] || [ "$http_code" = "204" ]; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
        status="✅ OK"
    elif [ "$http_code" = "400" ] || [ "$http_code" = "401" ] || [ "$http_code" = "403" ] || [ "$http_code" = "404" ] || [ "$http_code" = "405" ] || [ "$http_code" = "422" ]; then
        WARN_TESTS=$((WARN_TESTS + 1))
        status="⚠️ WARN"
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
        status="❌ FAIL"
    fi
    
    echo "  测试状态: $status" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    
    echo "$name: $status ($http_code)"
}

echo "开始测试..."
echo ""

# ============ 1. 基础端点（无需认证） ============
echo "=== 1. 基础端点 ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "根路径" "GET" "$BASE_URL/" "" "" "根路径测试"
test_api "健康检查" "GET" "$BASE_URL/health" "" "" "健康检查端点"
test_api "客户端版本" "GET" "$BASE_URL/_matrix/client/versions" "" "" "获取客户端支持的版本"
test_api "联邦版本" "GET" "$BASE_URL/_matrix/federation/v1/version" "" "" "获取联邦服务器版本"
test_api "联邦发现" "GET" "$BASE_URL/_matrix/federation/v1" "" "" "联邦服务器发现"

# ============ 2. 认证相关API ============
echo "=== 2. 认证相关API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "用户登录" "POST" "$BASE_URL/_matrix/client/r0/login" "" '{"user":"testuser","password":"TestPass123!"}' "用户登录"
test_api "检查用户名可用性" "GET" "$BASE_URL/_matrix/client/r0/register/available?username=newuser123" "" "" "检查用户名是否可用"
test_api "刷新令牌" "POST" "$BASE_URL/_matrix/client/r0/refresh" "" '{"refresh_token":"test_refresh_token"}' "刷新访问令牌"

# ============ 3. 用户账户API（需要认证） ============
echo "=== 3. 用户账户API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "获取当前用户信息" "GET" "$BASE_URL/_matrix/client/r0/account/whoami" "$TEST_USER_TOKEN" "" "获取当前登录用户信息"
test_api "获取用户资料" "GET" "$BASE_URL/_matrix/client/r0/account/profile/$TEST_USER_ID" "$TEST_USER_TOKEN" "" "获取用户资料"
test_api "更新显示名称" "PUT" "$BASE_URL/_matrix/client/r0/account/profile/$TEST_USER_ID/displayname" "$TEST_USER_TOKEN" '{"displayname":"Test User Updated"}' "更新用户显示名称"
test_api "修改密码" "POST" "$BASE_URL/_matrix/client/r0/account/password" "$TEST_USER_TOKEN" '{"new_password":"NewPass123!"}' "修改用户密码"
test_api "停用账户" "POST" "$BASE_URL/_matrix/client/r0/account/deactivate" "$TEST_USER_TOKEN" '{"erase":false}' "停用用户账户"

# ============ 4. 设备管理API ============
echo "=== 4. 设备管理API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "获取设备列表" "GET" "$BASE_URL/_matrix/client/r0/devices" "$TEST_USER_TOKEN" "" "获取用户的所有设备"
test_api "删除设备" "POST" "$BASE_URL/_matrix/client/r0/delete_devices" "$TEST_USER_TOKEN" '{"devices":[]}' "删除指定设备"
test_api "获取设备详情" "GET" "$BASE_URL/_matrix/client/r0/devices/lcS8Laaw01fG/U+EoR8wHQ==" "$TEST_USER_TOKEN" "" "获取指定设备的详细信息"
test_api "更新设备" "PUT" "$BASE_URL/_matrix/client/r0/devices/lcS8Laaw01fG/U+EoR8wHQ==" "$TEST_USER_TOKEN" '{"display_name":"Test Device"}' "更新设备信息"
test_api "删除单个设备" "DELETE" "$BASE_URL/_matrix/client/r0/devices/lcS8Laaw01fG/U+EoR8wHQ==" "$TEST_USER_TOKEN" "" "删除指定设备"

# ============ 5. 房间管理API ============
echo "=== 5. 房间管理API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# 先创建一个房间用于测试
ROOM_RESPONSE=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
  -H "Authorization: Bearer $TEST_USER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test Room","visibility":"private"}')
ROOM_ID=$(echo "$ROOM_RESPONSE" | grep -o '"room_id":"[^"]*"' | cut -d'"' -f4)
echo "创建的房间ID: $ROOM_ID" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "创建房间" "POST" "$BASE_URL/_matrix/client/r0/createRoom" "$TEST_USER_TOKEN" '{"name":"Another Test Room","visibility":"private"}' "创建新房间"
test_api "获取房间" "GET" "$BASE_URL/_matrix/client/r0/directory/room/$ROOM_ID" "$TEST_USER_TOKEN" "" "获取房间信息"
test_api "删除房间" "DELETE" "$BASE_URL/_matrix/client/r0/directory/room/$ROOM_ID" "$TEST_USER_TOKEN" "" "删除房间"
test_api "加入房间" "POST" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/join" "$TEST_USER_TOKEN" "" "加入房间"
test_api "离开房间" "POST" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/leave" "$TEST_USER_TOKEN" "" "离开房间"
test_api "获取房间成员" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/members" "$TEST_USER_TOKEN" "" "获取房间成员列表"
test_api "邀请用户" "POST" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/invite" "$TEST_USER_TOKEN" '{"user_id":"'$TEST_ADMIN_ID'"}' "邀请用户加入房间"
test_api "获取公开房间列表" "GET" "$BASE_URL/_matrix/client/r0/publicRooms" "$TEST_USER_TOKEN" "" "获取公开房间列表"
test_api "获取用户房间列表" "GET" "$BASE_URL/_matrix/client/r0/user/$TEST_USER_ID/rooms" "$TEST_USER_TOKEN" "" "获取用户加入的房间列表"

# ============ 6. 同步与消息API ============
echo "=== 6. 同步与消息API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "同步事件" "GET" "$BASE_URL/_matrix/client/r0/sync" "$TEST_USER_TOKEN" "" "同步服务器事件"
test_api "获取房间消息" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/messages?limit=10" "$TEST_USER_TOKEN" "" "获取房间消息历史"
test_api "发送消息" "POST" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message/$(date +%s)" "$TEST_USER_TOKEN" '{"msgtype":"m.text","body":"Test message from API test"}' "发送文本消息"

# ============ 7. 在线状态API ============
echo "=== 7. 在线状态API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "获取在线状态" "GET" "$BASE_URL/_matrix/client/r0/presence/$TEST_USER_ID/status" "$TEST_USER_TOKEN" "" "获取用户在线状态"
test_api "设置在线状态" "PUT" "$BASE_URL/_matrix/client/r0/presence/$TEST_USER_ID/status" "$TEST_USER_TOKEN" '{"presence":"online","status_msg":"Testing"}' "设置用户在线状态"

# ============ 8. 房间状态API ============
echo "=== 8. 房间状态API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "获取房间状态" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/state" "$TEST_USER_TOKEN" "" "获取房间所有状态事件"
test_api "按类型获取状态" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/state/m.room.name" "$TEST_USER_TOKEN" "" "按类型获取房间状态"
test_api "获取状态事件" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/state/m.room.name/" "$TEST_USER_TOKEN" "" "获取特定状态事件"
test_api "撤回事件" "PUT" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/redact/\$event_id" "$TEST_USER_TOKEN" '{"reason":"Test redaction"}' "撤回房间事件"
test_api "踢出用户" "POST" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/kick" "$TEST_USER_TOKEN" '{"user_id":"'$TEST_ADMIN_ID'","reason":"Test kick"}' "踢出房间用户"
test_api "封禁用户" "POST" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/ban" "$TEST_USER_TOKEN" '{"user_id":"'$TEST_ADMIN_ID'","reason":"Test ban"}' "封禁房间用户"
test_api "解封用户" "POST" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/unban" "$TEST_USER_TOKEN" '{"user_id":"'$TEST_ADMIN_ID'"}' "解封房间用户"

# ============ 9. 管理API（需要管理员权限） ============
echo "=== 9. 管理API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "获取服务器版本" "GET" "$BASE_URL/_synapse/admin/v1/server_version" "$TEST_ADMIN_TOKEN" "" "获取服务器版本信息"
test_api "获取服务器状态" "GET" "$BASE_URL/_synapse/admin/v1/status" "$TEST_ADMIN_TOKEN" "" "获取服务器状态"
test_api "获取用户列表" "GET" "$BASE_URL/_synapse/admin/v1/users" "$TEST_ADMIN_TOKEN" "" "获取所有用户列表"
test_api "获取用户信息" "GET" "$BASE_URL/_synapse/admin/v1/users/$TEST_USER_ID" "$TEST_ADMIN_TOKEN" "" "获取指定用户信息"
test_api "设置管理员" "PUT" "$BASE_URL/_synapse/admin/v1/users/$TEST_USER_ID/admin" "$TEST_ADMIN_TOKEN" '{"admin":false}' "设置用户管理员权限"
test_api "停用用户（管理员）" "POST" "$BASE_URL/_synapse/admin/v1/users/$TEST_USER_ID/deactivate" "$TEST_ADMIN_TOKEN" '{"erase":false}' "管理员停用用户"
test_api "获取用户房间（管理员）" "GET" "$BASE_URL/_synapse/admin/v1/users/$TEST_USER_ID/rooms" "$TEST_ADMIN_TOKEN" "" "管理员获取用户房间列表"
test_api "获取房间列表（管理员）" "GET" "$BASE_URL/_synapse/admin/v1/rooms" "$TEST_ADMIN_TOKEN" "" "管理员获取所有房间列表"
test_api "获取房间信息（管理员）" "GET" "$BASE_URL/_synapse/admin/v1/rooms/$ROOM_ID" "$TEST_ADMIN_TOKEN" "" "管理员获取房间信息"
test_api "删除房间（管理员）" "POST" "$BASE_URL/_synapse/admin/v1/rooms/$ROOM_ID/delete" "$TEST_ADMIN_TOKEN" '{"purge":true}' "管理员删除房间"
test_api "清理历史" "POST" "$BASE_URL/_synapse/admin/v1/purge_history" "$TEST_ADMIN_TOKEN" '{"room_id":"'$ROOM_ID'","before_ts":1000000000}' "清理房间历史消息"
test_api "关闭房间" "POST" "$BASE_URL/_synapse/admin/v1/shutdown_room" "$TEST_ADMIN_TOKEN" '{"room_id":"'$ROOM_ID'","new_room_id":"!newroom:server"}' "关闭房间"
test_api "获取安全事件" "GET" "$BASE_URL/_synapse/admin/v1/security/events" "$TEST_ADMIN_TOKEN" "" "获取安全事件日志"
test_api "获取IP封禁列表" "GET" "$BASE_URL/_synapse/admin/v1/security/ip/blocks" "$TEST_ADMIN_TOKEN" "" "获取IP封禁列表"
test_api "封禁IP" "POST" "$BASE_URL/_synapse/admin/v1/security/ip/block" "$TEST_ADMIN_TOKEN" '{"ip":"1.2.3.4","reason":"Test block"}' "封禁IP地址"
test_api "解封IP" "POST" "$BASE_URL/_synapse/admin/v1/security/ip/unblock" "$TEST_ADMIN_TOKEN" '{"ip":"1.2.3.4"}' "解封IP地址"
test_api "获取IP信誉" "GET" "$BASE_URL/_synapse/admin/v1/security/ip/reputation/1.2.3.4" "$TEST_ADMIN_TOKEN" "" "获取IP地址信誉"

# ============ 10. 联邦API ============
echo "=== 10. 联邦API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "获取服务器密钥" "GET" "$BASE_URL/_matrix/federation/v2/server" "" "" "获取联邦服务器密钥"
test_api "获取服务器密钥（v1）" "GET" "$BASE_URL/_matrix/key/v2/server" "" "" "获取服务器密钥（旧版本）"
test_api "密钥查询" "GET" "$BASE_URL/_matrix/federation/v2/query/matrix.cjystx.top/ed25519:abc123" "" "" "查询服务器密钥"
test_api "密钥查询（v1）" "GET" "$BASE_URL/_matrix/key/v2/query/matrix.cjystx.top/ed25519:abc123" "" "" "查询服务器密钥（旧版本）"
test_api "发送事务" "PUT" "$BASE_URL/_matrix/federation/v1/send/txn123" "$TEST_USER_TOKEN" '{"events":[]}' "发送联邦事务"
test_api "创建加入事件" "GET" "$BASE_URL/_matrix/federation/v1/make_join/$ROOM_ID/$TEST_USER_ID" "$TEST_USER_TOKEN" "" "创建房间加入事件"
test_api "创建离开事件" "GET" "$BASE_URL/_matrix/federation/v1/make_leave/$ROOM_ID/$TEST_USER_ID" "$TEST_USER_TOKEN" "" "创建房间离开事件"
test_api "发送加入事件" "PUT" "$BASE_URL/_matrix/federation/v1/send_join/$ROOM_ID/\$event123" "$TEST_USER_TOKEN" '{"event":{}}' "发送房间加入事件"
test_api "发送离开事件" "PUT" "$BASE_URL/_matrix/federation/v1/send_leave/$ROOM_ID/\$event123" "$TEST_USER_TOKEN" '{"event":{}}' "发送房间离开事件"
test_api "邀请用户（联邦）" "PUT" "$BASE_URL/_matrix/federation/v1/invite/$ROOM_ID/\$event123" "$TEST_USER_TOKEN" '{"event":{}}' "联邦邀请用户"
test_api "获取缺失事件" "POST" "$BASE_URL/_matrix/federation/v1/get_missing_events/$ROOM_ID" "$TEST_USER_TOKEN" '{"earliest_events":[],"latest_events":[],"limit":10}' "获取缺失的事件"
test_api "获取事件授权" "GET" "$BASE_URL/_matrix/federation/v1/get_event_auth/$ROOM_ID/\$event123" "$TEST_USER_TOKEN" "" "获取事件授权信息"
test_api "获取房间状态（联邦）" "GET" "$BASE_URL/_matrix/federation/v1/state/$ROOM_ID" "$TEST_USER_TOKEN" "" "获取房间状态（联邦）"
test_api "获取事件" "GET" "$BASE_URL/_matrix/federation/v1/event/\$event123" "$TEST_USER_TOKEN" "" "获取指定事件"
test_api "获取状态ID" "GET" "$BASE_URL/_matrix/federation/v1/state_ids/$ROOM_ID" "$TEST_USER_TOKEN" "" "获取房间状态ID列表"
test_api "房间目录查询" "GET" "$BASE_URL/_matrix/federation/v1/query/directory/room/$ROOM_ID" "$TEST_USER_TOKEN" "" "查询房间目录"
test_api "用户资料查询" "GET" "$BASE_URL/_matrix/federation/v1/query/profile/$TEST_USER_ID" "$TEST_USER_TOKEN" "" "查询用户资料"
test_api "回填事件" "GET" "$BASE_URL/_matrix/federation/v1/backfill/$ROOM_ID" "$TEST_USER_TOKEN" "" "回填历史事件"
test_api "声明密钥" "POST" "$BASE_URL/_matrix/federation/v1/keys/claim" "$TEST_USER_TOKEN" '{"one_time_keys":{}}' "声明一次性密钥"
test_api "上传密钥" "POST" "$BASE_URL/_matrix/federation/v1/keys/upload" "$TEST_USER_TOKEN" '{"device_keys":{}}' "上传设备密钥"
test_api "克隆密钥" "POST" "$BASE_URL/_matrix/federation/v2/key/clone" "$TEST_USER_TOKEN" '{"version":"1"}' "克隆密钥备份"
test_api "用户密钥查询" "POST" "$BASE_URL/_matrix/federation/v2/user/keys/query" "$TEST_USER_TOKEN" '{"device_keys":{}}' "查询用户密钥"

# ============ 11. 好友系统API ============
echo "=== 11. 好友系统API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "搜索用户" "GET" "$BASE_URL/_synapse/enhanced/friends/search?search_term=test" "$TEST_USER_TOKEN" "" "搜索用户"
test_api "获取好友列表" "GET" "$BASE_URL/_synapse/enhanced/friends" "$TEST_USER_TOKEN" "" "获取好友列表"
test_api "发送好友请求" "POST" "$BASE_URL/_synapse/enhanced/friend/request" "$TEST_USER_TOKEN" '{"user_id":"'$TEST_ADMIN_ID'","message":"Let's be friends"}' "发送好友请求"
test_api "获取好友请求" "GET" "$BASE_URL/_synapse/enhanced/friend/requests" "$TEST_USER_TOKEN" "" "获取好友请求列表"
test_api "接受好友请求" "POST" "$BASE_URL/_synapse/enhanced/friend/request/req123/accept" "$TEST_USER_TOKEN" "" "接受好友请求"
test_api "拒绝好友请求" "POST" "$BASE_URL/_synapse/enhanced/friend/request/req123/decline" "$TEST_USER_TOKEN" "" "拒绝好友请求"
test_api "获取封禁用户" "GET" "$BASE_URL/_synapse/enhanced/friend/blocks/$TEST_USER_ID" "$TEST_USER_TOKEN" "" "获取封禁用户列表"
test_api "封禁用户" "POST" "$BASE_URL/_synapse/enhanced/friend/blocks/$TEST_USER_ID" "$TEST_USER_TOKEN" '{"blocked_user_id":"'$TEST_ADMIN_ID'","reason":"Test block"}' "封禁用户"
test_api "解封用户" "DELETE" "$BASE_URL/_synapse/enhanced/friend/blocks/$TEST_USER_ID/$TEST_ADMIN_ID" "$TEST_USER_TOKEN" "" "解封用户"
test_api "获取好友分类" "GET" "$BASE_URL/_synapse/enhanced/friend/categories/$TEST_USER_ID" "$TEST_USER_TOKEN" "" "获取好友分类列表"
test_api "创建好友分类" "POST" "$BASE_URL/_synapse/enhanced/friend/categories/$TEST_USER_ID" "$TEST_USER_TOKEN" '{"name":"Family","description":"Family members"}' "创建好友分类"
test_api "更新好友分类" "PUT" "$BASE_URL/_synapse/enhanced/friend/categories/$TEST_USER_ID/Family" "$TEST_USER_TOKEN" '{"name":"Family Updated","description":"Updated description"}' "更新好友分类"
test_api "删除好友分类" "DELETE" "$BASE_URL/_synapse/enhanced/friend/categories/$TEST_USER_ID/Family" "$TEST_USER_TOKEN" "" "删除好友分类"
test_api "获取好友推荐" "GET" "$BASE_URL/_synapse/enhanced/friend/recommendations/$TEST_USER_ID" "$TEST_USER_TOKEN" "" "获取好友推荐"

# ============ 12. 语音消息API ============
echo "=== 12. 语音消息API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "上传语音消息" "POST" "$BASE_URL/_matrix/client/r0/voice/upload" "$TEST_USER_TOKEN" '{"content":"base64data","content_type":"audio/ogg","duration_ms":5000,"room_id":"'$ROOM_ID'"}' "上传语音消息"
test_api "获取当前用户语音统计" "GET" "$BASE_URL/_matrix/client/r0/voice/stats" "$TEST_USER_TOKEN" "" "获取当前用户语音统计"
test_api "获取语音消息" "GET" "$BASE_URL/_matrix/client/r0/voice/msg123" "" "" "获取语音消息"
test_api "删除语音消息" "DELETE" "$BASE_URL/_matrix/client/r0/voice/msg123" "$TEST_USER_TOKEN" "" "删除语音消息"
test_api "获取用户语音消息" "GET" "$BASE_URL/_matrix/client/r0/voice/user/$TEST_USER_ID" "" "" "获取用户语音消息列表"
test_api "获取房间语音消息" "GET" "$BASE_URL/_matrix/client/r0/voice/room/$ROOM_ID" "" "" "获取房间语音消息列表"
test_api "获取用户语音统计" "GET" "$BASE_URL/_matrix/client/r0/voice/user/$TEST_USER_ID/stats" "" "" "获取用户语音统计"

# ============ 13. E2EE API ============
echo "=== 13. E2EE API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "上传密钥" "POST" "$BASE_URL/_matrix/client/r0/keys/upload/lcS8Laaw01fG/U+EoR8wHQ==" "$TEST_USER_TOKEN" '{"device_keys":{"user_id":"'$TEST_USER_ID'","device_id":"lcS8Laaw01fG/U+EoR8wHQ==","algorithms":["m.olm.v1.curve25519-aes-sha2"],"keys":{},"signatures":{}},"one_time_keys":{}}' "上传设备密钥"
test_api "查询密钥" "POST" "$BASE_URL/_matrix/client/r0/keys/query" "$TEST_USER_TOKEN" '{"device_keys":{"'$TEST_USER_ID':[]}}' "查询设备密钥"
test_api "声明密钥" "POST" "$BASE_URL/_matrix/client/r0/keys/claim" "$TEST_USER_TOKEN" '{"one_time_keys":{"'$TEST_USER_ID':{}}}' "声明一次性密钥"
test_api "密钥变更" "GET" "$BASE_URL/_matrix/client/v3/keys/changes" "$TEST_USER_TOKEN" "" "获取密钥变更"
test_api "房间密钥分发" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/keys/distribution" "$TEST_USER_TOKEN" "" "获取房间密钥分发信息"
test_api "发送到设备" "PUT" "$BASE_URL/_matrix/client/v3/sendToDevice/m.room.encrypted/txn123" "$TEST_USER_TOKEN" '{"messages":{}}' "发送加密消息到设备"

# ============ 14. 媒体API ============
echo "=== 14. 媒体API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "上传媒体（v3）" "POST" "$BASE_URL/_matrix/media/v3/upload" "$TEST_USER_TOKEN" '' "上传媒体文件"
test_api "下载媒体" "GET" "$BASE_URL/_matrix/media/v3/download/matrix.cjystx.top/media123" "" "" "下载媒体文件"
test_api "获取缩略图" "GET" "$BASE_URL/_matrix/media/v3/thumbnail/matrix.cjystx.top/media123" "" "" "获取媒体缩略图"
test_api "上传媒体（v1）" "POST" "$BASE_URL/_matrix/media/v1/upload" "$TEST_USER_TOKEN" '' "上传媒体文件（v1）"
test_api "下载媒体（v1）" "GET" "$BASE_URL/_matrix/media/v1/download/matrix.cjystx.top/media123" "" "" "下载媒体文件（v1）"
test_api "下载媒体（r1）" "GET" "$BASE_URL/_matrix/media/r1/download/matrix.cjystx.top/media123" "" "" "下载媒体文件（r1）"
test_api "媒体配置" "GET" "$BASE_URL/_matrix/media/v1/config" "" "" "获取媒体配置"
test_api "上传媒体（带参数）" "POST" "$BASE_URL/_matrix/media/v3/upload/matrix.cjystx.top/media123" "$TEST_USER_TOKEN" '' "上传媒体文件（带服务器名）"

# ============ 15. 私聊API ============
echo "=== 15. 私聊API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "获取私聊房间" "GET" "$BASE_URL/_matrix/client/r0/dm" "$TEST_USER_TOKEN" "" "获取私聊房间列表"
test_api "创建私聊房间" "POST" "$BASE_URL/_matrix/client/r0/createDM" "$TEST_USER_TOKEN" '{"user_id":"'$TEST_ADMIN_ID'"}' "创建私聊房间"
test_api "获取私聊房间详情" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/dm" "$TEST_USER_TOKEN" "" "获取私聊房间详情"
test_api "获取未读通知" "GET" "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/unread" "$TEST_USER_TOKEN" "" "获取未读通知"
test_api "获取会话列表" "GET" "$BASE_URL/_synapse/enhanced/private/sessions" "$TEST_USER_TOKEN" "" "获取私聊会话列表"
test_api "创建会话" "POST" "$BASE_URL/_synapse/enhanced/private/sessions" "$TEST_USER_TOKEN" '{"user_id":"'$TEST_ADMIN_ID'","title":"Private Chat"}' "创建私聊会话"
test_api "获取会话详情" "GET" "$BASE_URL/_synapse/enhanced/private/sessions/session123" "$TEST_USER_TOKEN" "" "获取会话详情"
test_api "删除会话" "DELETE" "$BASE_URL/_synapse/enhanced/private/sessions/session123" "$TEST_USER_TOKEN" "" "删除私聊会话"
test_api "获取会话消息" "GET" "$BASE_URL/_synapse/enhanced/private/sessions/session123/messages" "$TEST_USER_TOKEN" "" "获取会话消息列表"
test_api "发送会话消息" "POST" "$BASE_URL/_synapse/enhanced/private/sessions/session123/messages" "$TEST_USER_TOKEN" '{"content":"Test private message"}' "发送私聊消息"
test_api "删除消息" "DELETE" "$BASE_URL/_synapse/enhanced/private/messages/msg123" "$TEST_USER_TOKEN" "" "删除私聊消息"
test_api "标记消息已读" "POST" "$BASE_URL/_synapse/enhanced/private/messages/msg123/read" "$TEST_USER_TOKEN" "" "标记消息为已读"
test_api "获取未读数量" "GET" "$BASE_URL/_synapse/enhanced/private/unread-count" "$TEST_USER_TOKEN" "" "获取未读消息数量"
test_api "搜索消息" "POST" "$BASE_URL/_synapse/enhanced/private/search" "$TEST_USER_TOKEN" '{"query":"test","limit":10}' "搜索私聊消息"

# ============ 16. 密钥备份API ============
echo "=== 16. 密钥备份API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "创建备份版本" "POST" "$BASE_URL/_matrix/client/r0/room_keys/version" "$TEST_USER_TOKEN" '{"algorithm":"m.megolm_backup.v1","auth_data":{}}' "创建密钥备份版本"
test_api "获取备份版本" "GET" "$BASE_URL/_matrix/client/r0/room_keys/version/1" "$TEST_USER_TOKEN" "" "获取密钥备份版本"
test_api "更新备份版本" "PUT" "$BASE_URL/_matrix/client/r0/room_keys/version/1" "$TEST_USER_TOKEN" '{"algorithm":"m.megolm_backup.v1","auth_data":{}}' "更新密钥备份版本"
test_api "删除备份版本" "DELETE" "$BASE_URL/_matrix/client/r0/room_keys/version/1" "$TEST_USER_TOKEN" "" "删除密钥备份版本"
test_api "获取房间密钥" "GET" "$BASE_URL/_matrix/client/r0/room_keys/1" "$TEST_USER_TOKEN" "" "获取所有房间密钥"
test_api "上传房间密钥" "PUT" "$BASE_URL/_matrix/client/r0/room_keys/1" "$TEST_USER_TOKEN" '{"rooms":{}}' "上传房间密钥"
test_api "批量上传密钥" "POST" "$BASE_URL/_matrix/client/r0/room_keys/1/keys" "$TEST_USER_TOKEN" '{"rooms":{}}' "批量上传房间密钥"
test_api "获取房间密钥（按ID）" "GET" "$BASE_URL/_matrix/client/r0/room_keys/1/keys/$ROOM_ID" "$TEST_USER_TOKEN" "" "获取指定房间的密钥"
test_api "获取会话密钥" "GET" "$BASE_URL/_matrix/client/r0/room_keys/1/keys/$ROOM_ID/session123" "$TEST_USER_TOKEN" "" "获取指定会话的密钥"

# ============ 17. 登出API ============
echo "=== 17. 登出API ===" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api "登出" "POST" "$BASE_URL/_matrix/client/r0/logout" "$TEST_USER_TOKEN" "" "用户登出"
test_api "全部登出" "POST" "$BASE_URL/_matrix/client/r0/logout/all" "$TEST_USER_TOKEN" "" "用户从所有设备登出"

# ============ 测试总结 ============
echo "" >> "$OUTPUT_FILE"
echo "=== 测试总结 ===" >> "$OUTPUT_FILE"
echo "总测试数: $TOTAL_TESTS" >> "$OUTPUT_FILE"
echo "通过: $PASSED_TESTS" >> "$OUTPUT_FILE"
echo "警告: $WARN_TESTS" >> "$OUTPUT_FILE"
echo "失败: $FAILED_TESTS" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

echo ""
echo "=== 测试完成 ==="
echo "总测试数: $TOTAL_TESTS"
echo "通过: $PASSED_TESTS"
echo "警告: $WARN_TESTS"
echo "失败: $FAILED_TESTS"
echo ""
echo "详细结果已保存到: $OUTPUT_FILE"
