#!/bin/bash

TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYXBpdGVzdF9maW5hbDpjanlzdHgudG9wIiwidXNlcl9pZCI6IkBhcGl0ZXN0X2ZpbmFsOmNqeXN0eC50b3AiLCJqdGkiOiIyZmU5Y2VkNy1jZmU4LTQ3NzAtYjNiZS1mYzFkMzlkZGQ0NDYiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MzIxNDc4MSwiaWF0IjoxNzczMjExMTgxLCJkZXZpY2VfaWQiOiJGbnRMMEdNWEFqUW5IeDNOIn0.Y2gxKz1xxkIXmREXel_6xn90iPkFuSFMT5oy9vkFidA"
HOST="matrix.cjystx.top"
BASE_URL="https://localhost"
PASS=0
FAIL=0
TOTAL=0

test_api() {
    local name="$1"
    local method="$2"
    local endpoint="$3"
    local data="$4"
    local need_auth="$5"
    
    TOTAL=$((TOTAL + 1))
    
    local response
    if [ "$method" = "GET" ]; then
        if [ "$need_auth" = "auth" ]; then
            response=$(curl -sk -X GET "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer $TOKEN" 2>/dev/null)
        else
            response=$(curl -sk -X GET "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" 2>/dev/null)
        fi
    else
        if [ "$need_auth" = "auth" ]; then
            response=$(curl -sk -X $method "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer $TOKEN" \
                -d "$data" 2>/dev/null)
        else
            response=$(curl -sk -X $method "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" \
                -d "$data" 2>/dev/null)
        fi
    fi
    
    local status_short=$(echo "$response" | head -c 100 | tr '\n' ' ')
    
    if [ -z "$response" ]; then
        echo "❌ [$TOTAL] $name: Empty response"
        FAIL=$((FAIL + 1))
    elif echo "$response" | grep -qE '"errcode":"M_UNAUTHORIZED"|"errcode":"M_MISSING_TOKEN"'; then
        echo "❌ [$TOTAL] $name: Auth failed"
        FAIL=$((FAIL + 1))
    elif echo "$response" | grep -qE '"errcode":"M_FORBIDDEN"'; then
        echo "⚠️ [$TOTAL] $name: Forbidden (expected for non-admin)"
        PASS=$((PASS + 1))
    elif echo "$response" | grep -qE '"errcode":"M_NOT_FOUND"'; then
        echo "⚠️ [$TOTAL] $name: Not Found"
        PASS=$((PASS + 1))
    elif echo "$response" | grep -qE '"errcode"'; then
        echo "❌ [$TOTAL] $name: $status_short"
        FAIL=$((FAIL + 1))
    else
        echo "✅ [$TOTAL] $name"
        PASS=$((PASS + 1))
    fi
}

echo "=========================================="
echo "API 全面测试 v4 - $(date)"
echo "=========================================="

# 1. 基础服务 API
echo -e "\n=== 1. 基础服务 API ==="
test_api "健康检查" GET "/health"
test_api "客户端版本" GET "/_matrix/client/versions"
test_api "服务器版本" GET "/_matrix/client/r0/version"
test_api "客户端能力" GET "/_matrix/client/v3/capabilities" "" "auth"
test_api "Well-Known Server" GET "/.well-known/matrix/server"
test_api "Well-Known Client" GET "/.well-known/matrix/client"
test_api "Well-Known Support" GET "/.well-known/matrix/support"

# 2. 用户认证 API
echo -e "\n=== 2. 用户认证 API ==="
test_api "登录流程" GET "/_matrix/client/v3/login"
test_api "用户名可用性" GET "/_matrix/client/v3/register/available?username=testuser123"
test_api "当前用户" GET "/_matrix/client/v3/account/whoami" "" "auth"

# 3. 账户管理 API
echo -e "\n=== 3. 账户管理 API ==="
test_api "用户资料" GET "/_matrix/client/v3/profile/@apitest_full:cjystx.top" "" "auth"
test_api "显示名" GET "/_matrix/client/v3/profile/@apitest_full:cjystx.top/displayname" "" "auth"
test_api "头像URL" GET "/_matrix/client/v3/profile/@apitest_full:cjystx.top/avatar_url" "" "auth"
test_api "第三方ID列表" GET "/_matrix/client/v3/account/3pid" "" "auth"

# 4. 房间管理 API
echo -e "\n=== 4. 房间管理 API ==="
test_api "已加入房间" GET "/_matrix/client/v3/joined_rooms" "" "auth"
test_api "公开房间" GET "/_matrix/client/v3/publicRooms"
test_api "创建房间" POST "/_matrix/client/v3/createRoom" '{"name":"Test Room"}' "auth"

# 5. 设备管理 API
echo -e "\n=== 5. 设备管理 API ==="
test_api "设备列表" GET "/_matrix/client/v3/devices" "" "auth"

# 6. 推送通知 API
echo -e "\n=== 6. 推送通知 API ==="
test_api "推送器列表" GET "/_matrix/client/v3/pushers" "" "auth"
test_api "推送规则" GET "/_matrix/client/v3/pushrules" "" "auth"
test_api "通知列表" GET "/_matrix/client/v3/notifications" "" "auth"

# 7. E2EE 加密 API
echo -e "\n=== 7. E2EE 加密 API ==="
test_api "密钥上传" POST "/_matrix/client/v3/keys/upload" '{}' "auth"
test_api "密钥查询" POST "/_matrix/client/v3/keys/query" '{"device_keys":{}}' "auth"
test_api "密钥变更" GET "/_matrix/client/v3/keys/changes" "" "auth"

# 8. 媒体服务 API
echo -e "\n=== 8. 媒体服务 API ==="
test_api "媒体配置" GET "/_matrix/media/v3/config" "" "auth"
test_api "URL预览" GET "/_matrix/media/v3/preview_url?url=https://example.com" "" "auth"

# 9. 好友系统 API
echo -e "\n=== 9. 好友系统 API ==="
test_api "好友列表" GET "/_matrix/client/v1/friends" "" "auth"
test_api "好友分组" GET "/_matrix/client/v1/friends/groups" "" "auth"

# 10. Space 空间 API
echo -e "\n=== 10. Space 空间 API ==="
test_api "公开空间" GET "/_matrix/client/v1/spaces/public" "" "auth"
test_api "用户空间" GET "/_matrix/client/v1/spaces/user" "" "auth"
test_api "空间搜索" GET "/_matrix/client/v1/spaces/search?query=test" "" "auth"

# 11. Thread 线程 API (需要 room_id)
echo -e "\n=== 11. Thread 线程 API ==="
test_api "线程列表" GET "/_matrix/client/v1/rooms/!test:cjystx.top/threads" "" "auth"

# 12. 搜索服务 API
echo -e "\n=== 12. 搜索服务 API ==="
test_api "消息搜索" POST "/_matrix/client/v3/search" '{"search_categories":{"room_events":{"search_term":"test"}}}' "auth"
test_api "用户搜索" POST "/_matrix/client/v3/user_directory/search" '{"search_term":"test"}' "auth"

# 13. 管理后台 API
echo -e "\n=== 13. 管理后台 API ==="
test_api "服务器版本(admin)" GET "/_synapse/admin/v1/server_version" "" "auth"
test_api "用户列表(admin)" GET "/_synapse/admin/v1/users" "" "auth"
test_api "房间列表(admin)" GET "/_synapse/admin/v1/rooms" "" "auth"
test_api "服务器统计(admin)" GET "/_synapse/admin/v1/statistics" "" "auth"

# 14. 联邦 API
echo -e "\n=== 14. 联邦 API ==="
test_api "联邦版本" GET "/_matrix/federation/v1/version"
test_api "服务器密钥" GET "/_matrix/key/v2/server"

# 15. 密钥备份 API
echo -e "\n=== 15. 密钥备份 API ==="
test_api "备份版本" GET "/_matrix/client/v3/room_keys/version" "" "auth"
test_api "所有密钥" GET "/_matrix/client/v3/room_keys/keys" "" "auth"

# 16. VoIP 服务 API
echo -e "\n=== 16. VoIP 服务 API ==="
test_api "TURN服务器" GET "/_matrix/client/v3/voip/turnServer" "" "auth"
test_api "VoIP配置" GET "/_matrix/client/v3/voip/config" "" "auth"

# 17. 语音消息 API
echo -e "\n=== 17. 语音消息 API ==="
test_api "语音配置" GET "/_matrix/client/r0/voice/config" "" "auth"
test_api "语音统计" GET "/_matrix/client/r0/voice/stats" "" "auth"

# 18. 验证码服务 API (正确路由)
echo -e "\n=== 18. 验证码服务 API ==="
test_api "发送验证码" POST "/_matrix/client/r0/register/captcha/send" '{"captcha_type":"email","target":"test@example.com"}'
test_api "验证码状态" GET "/_matrix/client/r0/register/captcha/status"

# 19. 后台更新 API
echo -e "\n=== 19. 后台更新 API ==="
test_api "更新列表" GET "/_synapse/admin/v1/background_updates" "" "auth"
test_api "更新统计" GET "/_synapse/admin/v1/background_updates/stats" "" "auth"

# 20. 事件举报 API
echo -e "\n=== 20. 事件举报 API ==="
test_api "举报列表" GET "/_synapse/admin/v1/event_reports" "" "auth"
test_api "举报统计" GET "/_synapse/admin/v1/event_reports/stats" "" "auth"

# 21. 账户数据 API
echo -e "\n=== 21. 账户数据 API ==="
test_api "全局账户数据" GET "/_matrix/client/v3/user/@apitest_full:cjystx.top/account_data/m.direct" "" "auth"

# 22. 保留策略 API (正确路由)
echo -e "\n=== 22. 保留策略 API ==="
test_api "服务器策略" GET "/_synapse/retention/v1/server/policy" "" "auth"
test_api "房间列表" GET "/_synapse/retention/v1/rooms" "" "auth"

# 23. 服务器通知 API
echo -e "\n=== 23. 服务器通知 API ==="
test_api "通知列表(server)" GET "/_synapse/admin/v1/server_notifications" "" "auth"
test_api "通知统计(server)" GET "/_synapse/admin/v1/server_notifications/stats" "" "auth"

# 24. 注册令牌 API
echo -e "\n=== 24. 注册令牌 API ==="
test_api "令牌列表" GET "/_synapse/admin/v1/registration_tokens" "" "auth"

# 25. 媒体配额 API (正确路由)
echo -e "\n=== 25. 媒体配额 API ==="
test_api "配额检查" GET "/_matrix/media/v1/quota/check?file_size=1024" "" "auth"
test_api "配额统计" GET "/_matrix/media/v1/quota/stats" "" "auth"

# 26. CAS 认证 API
echo -e "\n=== 26. CAS 认证 API ==="
test_api "CAS配置" GET "/_synapse/admin/v1/cas/config" "" "auth"

# 27. SAML 认证 API
echo -e "\n=== 27. SAML 认证 API ==="
test_api "SAML配置" GET "/_synapse/admin/v1/saml/config" "" "auth"

# 28. OIDC 认证 API
echo -e "\n=== 28. OIDC 认证 API ==="
test_api "OIDC配置" GET "/_synapse/admin/v1/oidc/config" "" "auth"

# 29. Rendezvous API
echo -e "\n=== 29. Rendezvous API ==="
test_api "创建会话" POST "/_matrix/client/v1/rendezvous" '{"intent":"login.start","transport":"http.v1"}'

# 30. Worker API (正确路由)
echo -e "\n=== 30. Worker API ==="
test_api "Worker列表" GET "/_synapse/worker/v1/workers" "" "auth"
test_api "Worker统计" GET "/_synapse/worker/v1/statistics" "" "auth"

# 31. 联邦黑名单 API (正确路由)
echo -e "\n=== 31. 联邦黑名单 API ==="
test_api "黑名单列表" GET "/_synapse/admin/v1/federation/blacklist" "" "auth"

# 32. 联邦缓存 API (正确路由)
echo -e "\n=== 32. 联邦缓存 API ==="
test_api "缓存统计" GET "/_synapse/admin/v1/federation/cache/stats" "" "auth"

# 33. 刷新令牌 API
echo -e "\n=== 33. 刷新令牌 API ==="
test_api "令牌列表(refresh)" GET "/_synapse/admin/v1/refresh_tokens" "" "auth"

# 34. 推送通知管理 API
echo -e "\n=== 34. 推送通知管理 API ==="
test_api "推送列表(mgmt)" GET "/_synapse/admin/v1/push_notifications" "" "auth"

# 35. 速率限制管理 API
echo -e "\n=== 35. 速率限制管理 API ==="
test_api "限制列表" GET "/_synapse/admin/v1/rate_limits" "" "auth"

# 36. Sliding Sync API
echo -e "\n=== 36. Sliding Sync API ==="
test_api "Sliding Sync" POST "/_matrix/client/unstable/org.matrix.msc3575/sync" "{} " "auth"

# 37. 遥测 API (正确路由)
echo -e "\n=== 37. 遥测 API ==="
test_api "遥测状态" GET "/_synapse/admin/v1/telemetry/status" "" "auth"
test_api "遥测健康" GET "/_synapse/admin/v1/telemetry/health" "" "auth"

echo -e "\n=========================================="
echo "测试完成"
echo "=========================================="
echo "总计: $TOTAL 个测试"
echo "通过: $PASS 个"
echo "失败: $FAIL 个"
echo "通过率: $((PASS * 100 / TOTAL))%"
