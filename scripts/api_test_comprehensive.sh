#!/bin/bash

BASE_URL="https://matrix.cjystx.top"
USER_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYXBpdGVzdF91c2VyOmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFwaXRlc3RfdXNlcjpjanlzdHgudG9wIiwianRpIjoiMmFiZWY4ODEtNTVlZC00MDg2LWFlNmYtMjg2N2IwOTYzNWJkIiwiYWRtaW4iOmZhbHNlLCJleHAiOjE3NzMxNTYzMjYsImlhdCI6MTc3MzE1MjcyNiwiZGV2aWNlX2lkIjoiZVdaZVFhOGtjMTc2U0VPNUg3ZzU2dyJ9.KdB1THN6UjSoWd5VIdShDOSkB1OqHWRYH6FROSpCjrI"
USER_ID="@apitest_user:cjystx.top"
ROOM_ID="!K57yqce4-veKAurjbx4YufFN:cjystx.top"

PASS=0
FAIL=0
RESULTS=""
FAILED_TESTS=""

test_api() {
    local name="$1"
    local method="$2"
    local endpoint="$3"
    local token="$4"
    local data="$5"
    local expected="${6:-200}"
    
    local auth_header=""
    if [ -n "$token" ]; then
        auth_header="-H \"Authorization: Bearer $token\""
    fi
    
    local cmd=""
    if [ "$method" = "GET" ]; then
        cmd="curl -sk -o /dev/null -w \"%{http_code}\" -X GET \"$BASE_URL$endpoint\" $auth_header"
    elif [ "$method" = "POST" ]; then
        cmd="curl -sk -o /dev/null -w \"%{http_code}\" -X POST \"$BASE_URL$endpoint\" $auth_header -H \"Content-Type: application/json\" -d '$data'"
    elif [ "$method" = "PUT" ]; then
        cmd="curl -sk -o /dev/null -w \"%{http_code}\" -X PUT \"$BASE_URL$endpoint\" $auth_header -H \"Content-Type: application/json\" -d '$data'"
    elif [ "$method" = "DELETE" ]; then
        cmd="curl -sk -o /dev/null -w \"%{http_code}\" -X DELETE \"$BASE_URL$endpoint\" $auth_header"
    fi
    
    local code=$(eval $cmd 2>/dev/null)
    
    if [ "$code" = "$expected" ]; then
        echo "✅ PASS: $name -> $code"
        PASS=$((PASS + 1))
        RESULTS="${RESULTS}✅ $name: $code\n"
    else
        echo "❌ FAIL: $name -> $code (expected $expected)"
        FAIL=$((FAIL + 1))
        RESULTS="${RESULTS}❌ $name: $code (expected $expected)\n"
        FAILED_TESTS="${FAILED_TESTS}| $name | $method $endpoint | $code | $expected |\n"
    fi
}

echo "=========================================="
echo "API 全面测试 - 模块化测试"
echo "=========================================="
echo ""

echo "=== 1. 基础服务 API ==="
test_api "健康检查" "GET" "/health" "" "" "200"
test_api "版本信息" "GET" "/_matrix/client/versions" "" "" "200"
test_api "客户端能力" "GET" "/_matrix/client/v3/capabilities" "$USER_TOKEN" "" "200"
test_api "Well-Known服务器" "GET" "/.well-known/matrix/server" "" "" "200"
test_api "Well-Known客户端" "GET" "/.well-known/matrix/client" "" "" "200"

echo ""
echo "=== 2. 用户认证 API ==="
test_api "登录流程" "GET" "/_matrix/client/v3/login" "" "" "200"
test_api "当前用户" "GET" "/_matrix/client/v3/account/whoami" "$USER_TOKEN" "" "200"

echo ""
echo "=== 3. 媒体服务 API ==="
test_api "媒体配置" "GET" "/_matrix/media/v3/config" "$USER_TOKEN" "" "200"
test_api "上传媒体" "POST" "/_matrix/media/v3/upload?filename=test.txt" "$USER_TOKEN" "test content" "200"

echo ""
echo "=== 4. 好友系统 API ==="
test_api "获取好友列表" "GET" "/_matrix/client/v1/friends" "$USER_TOKEN" "" "200"
test_api "获取好友分组" "GET" "/_matrix/client/v1/friends/groups" "$USER_TOKEN" "" "200"

echo ""
echo "=== 5. Space 空间 API ==="
test_api "获取公开空间" "GET" "/_matrix/client/v1/spaces/public" "$USER_TOKEN" "" "200"
test_api "搜索空间" "GET" "/_matrix/client/v1/spaces/search?query=test" "$USER_TOKEN" "" "200"
test_api "获取用户空间" "GET" "/_matrix/client/v1/spaces/user" "$USER_TOKEN" "" "200"

echo ""
echo "=== 6. Thread 线程 API ==="
test_api "获取线程列表" "GET" "/_matrix/client/v1/threads" "$USER_TOKEN" "" "200"
test_api "获取订阅列表" "GET" "/_matrix/client/v1/threads/subscribed" "$USER_TOKEN" "" "200"
test_api "获取未读线程" "GET" "/_matrix/client/v1/threads/unread" "$USER_TOKEN" "" "200"

echo ""
echo "=== 7. 密钥备份 API ==="
test_api "获取备份版本" "GET" "/_matrix/client/v3/room_keys/version" "$USER_TOKEN" "" "200"
test_api "获取所有密钥" "GET" "/_matrix/client/v3/room_keys/keys" "$USER_TOKEN" "" "200"

echo ""
echo "=== 8. E2EE 加密 API ==="
test_api "上传设备密钥" "POST" "/_matrix/client/v3/keys/upload" "$USER_TOKEN" "{\"device_keys\":{}}" "200"
test_api "查询设备密钥" "POST" "/_matrix/client/v3/keys/query" "$USER_TOKEN" "{\"device_keys\":{\"$USER_ID\":[]}}" "200"
test_api "申领一次性密钥" "POST" "/_matrix/client/v3/keys/claim" "$USER_TOKEN" "{\"one_time_keys\":{\"$USER_ID\":{\"DEVICEID\":\"signed_curve25519\"}}}" "200"
test_api "获取密钥变更" "GET" "/_matrix/client/v3/keys/changes?from=s0&to=s100" "$USER_TOKEN" "" "200"

echo ""
echo "=== 9. To-Device 消息 API ==="
TXN_ID="txn_$(date +%s)"
test_api "发送ToDevice消息" "PUT" "/_matrix/client/v3/sendToDevice/m.room.encrypted/$TXN_ID" "$USER_TOKEN" "{\"messages\":{\"$USER_ID\":{\"DEVICEID\":{\"algorithm\":\"m.megolm.v1.aes-sha2\"}}}}" "200"

echo ""
echo "=== 10. 推送通知 API ==="
test_api "获取推送器列表" "GET" "/_matrix/client/v3/pushers" "$USER_TOKEN" "" "200"
test_api "获取推送规则" "GET" "/_matrix/client/v3/pushrules" "$USER_TOKEN" "" "200"
test_api "获取全局规则" "GET" "/_matrix/client/v3/pushrules/global" "$USER_TOKEN" "" "200"
test_api "获取通知列表" "GET" "/_matrix/client/v3/notifications" "$USER_TOKEN" "" "200"

echo ""
echo "=== 11. 房间管理 API ==="
test_api "公开房间列表" "GET" "/_matrix/client/v3/publicRooms" "" "" "200"
test_api "已加入房间" "GET" "/_matrix/client/v3/joined_rooms" "$USER_TOKEN" "" "200"
test_api "创建房间" "POST" "/_matrix/client/v3/createRoom" "$USER_TOKEN" "{\"name\":\"Test Room\"}" "200"
test_api "解析房间别名" "GET" "/_matrix/client/v3/directory/room/%23test:cjystx.top" "$USER_TOKEN" "" "404"

echo ""
echo "=== 12. 用户资料 API ==="
test_api "获取用户资料" "GET" "/_matrix/client/v3/profile/$USER_ID" "$USER_TOKEN" "" "200"
test_api "获取显示名" "GET" "/_matrix/client/v3/profile/$USER_ID/displayname" "$USER_TOKEN" "" "200"
test_api "获取头像URL" "GET" "/_matrix/client/v3/profile/$USER_ID/avatar_url" "$USER_TOKEN" "" "200"
test_api "设置显示名" "PUT" "/_matrix/client/v3/profile/$USER_ID/displayname" "$USER_TOKEN" "{\"displayname\":\"API Test User\"}" "200"

echo ""
echo "=== 13. 账户管理 API ==="
test_api "获取绑定列表" "GET" "/_matrix/client/v3/account/3pid" "$USER_TOKEN" "" "200"

echo ""
echo "=== 14. 设备管理 API ==="
test_api "获取设备列表" "GET" "/_matrix/client/v3/devices" "$USER_TOKEN" "" "200"

echo ""
echo "=== 15. 过滤器 API ==="
test_api "创建过滤器" "POST" "/_matrix/client/v3/user/$USER_ID/filter" "$USER_TOKEN" "{\"room\":{\"timeline\":{\"limit\":50}}}" "200"

echo ""
echo "=== 16. 搜索服务 API ==="
test_api "搜索消息" "POST" "/_matrix/client/v3/search" "$USER_TOKEN" "{\"search_categories\":{\"room_events\":{\"search_term\":\"test\"}}}" "200"
test_api "搜索用户" "POST" "/_matrix/client/v3/user_directory/search" "$USER_TOKEN" "{\"search_term\":\"test\"}" "200"

echo ""
echo "=== 17. 同步 API ==="
test_api "同步" "GET" "/_matrix/client/v3/sync?timeout=0" "$USER_TOKEN" "" "200"

echo ""
echo "=== 18. 联邦 API ==="
test_api "获取服务器密钥" "GET" "/_matrix/key/v2/server" "" "" "200"
test_api "联邦版本" "GET" "/_matrix/federation/v1/version" "" "" "200"

echo ""
echo "=== 19. 语音消息 API ==="
test_api "获取语音配置" "GET" "/_matrix/client/r0/voice/config" "$USER_TOKEN" "" "200"
test_api "获取语音统计" "GET" "/_matrix/client/r0/voice/stats" "$USER_TOKEN" "" "200"

echo ""
echo "=== 20. VoIP 服务 API ==="
test_api "获取TURN服务器" "GET" "/_matrix/client/v3/voip/turnServer" "$USER_TOKEN" "" "404"
test_api "获取VoIP配置" "GET" "/_matrix/client/v3/voip/config" "$USER_TOKEN" "" "200"

echo ""
echo "=== 21. 账户数据 API ==="
test_api "获取账户数据" "GET" "/_matrix/client/v3/user/$USER_ID/account_data/m.direct" "$USER_TOKEN" "" "200"
test_api "设置账户数据" "PUT" "/_matrix/client/v3/user/$USER_ID/account_data/m.custom" "$USER_TOKEN" "{\"custom_key\":\"custom_value\"}" "200"

echo ""
echo "=== 22. 管理后台 API (需要管理员权限) ==="
test_api "服务器版本" "GET" "/_synapse/admin/v1/server_version" "$USER_TOKEN" "" "403"
test_api "用户列表" "GET" "/_synapse/admin/v1/users" "$USER_TOKEN" "" "403"

echo ""
echo "=========================================="
echo "测试结果汇总"
echo "=========================================="
echo "通过: $PASS"
echo "失败: $FAIL"
TOTAL=$((PASS + FAIL))
if [ $TOTAL -gt 0 ]; then
    RATE=$((PASS * 100 / TOTAL))
    echo "通过率: ${RATE}%"
fi
echo ""

if [ $FAIL -gt 0 ]; then
    echo "=========================================="
    echo "失败测试详情"
    echo "=========================================="
    echo "| 测试名称 | 端点 | 实际状态码 | 预期状态码 |"
    echo "|----------|------|------------|------------|"
    echo -e "$FAILED_TESTS"
fi
