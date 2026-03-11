#!/bin/bash

# API 全面系统性测试脚本 - 第二轮
# 使用有效 JWT Token 进行测试

BASE_URL="http://localhost:8008"
ERROR_FILE="/Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/api-error.md"

# 有效测试账户信息
USER_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYXBpdGVzdF91c2VyXzE6bG9jYWxob3N0IiwidXNlcl9pZCI6IkBhcGl0ZXN0X3VzZXJfMTpsb2NhbGhvc3QiLCJqdGkiOiJlMGE5ZjJiNS1jYTVmLTRjODEtOTVkYS04MDczMGM5NjhiZWIiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MzE0ODEyNSwiaWF0IjoxNzczMTQ0NTI1LCJkZXZpY2VfaWQiOiJ4TGliRWhiV2hrMnNOVXE2ZjlrUUFRIn0.wHhmmH2aySAeQ00eYOXSTjy6GHeDmnlYp7ZGTS0i5qI"
USER_ID="@apitest_user_1:localhost"
DEVICE_ID="xLibEhbWhk2sNUq6f9kQAQ"

# 测试统计
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 失败端点记录
declare -a FAILED_ENDPOINTS=()

log_test() {
    local endpoint=$1
    local method=$2
    local status=$3
    local response=$4
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ "$status" -eq 0 ]; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
        echo -e "${GREEN}[PASS]${NC} $method $endpoint"
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
        echo -e "${RED}[FAIL]${NC} $method $endpoint"
        FAILED_ENDPOINTS+=("$method $endpoint|$response")
    fi
}

test_get() {
    local endpoint=$1
    local token=$2
    local expected_code=${3:-200}
    
    local response
    if [ -z "$token" ]; then
        response=$(curl -s -w "\n%{http_code}" "$BASE_URL$endpoint" 2>/dev/null)
    else
        response=$(curl -s -w "\n%{http_code}" -H "Authorization: Bearer $token" "$BASE_URL$endpoint" 2>/dev/null)
    fi
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$endpoint" "GET" 0 "$body"
        return 0
    else
        log_test "$endpoint" "GET" 1 "HTTP $http_code: $body"
        return 1
    fi
}

test_post() {
    local endpoint=$1
    local token=$2
    local data=$3
    local expected_code=${4:-200}
    
    local response
    if [ -z "$token" ]; then
        response=$(curl -s -w "\n%{http_code}" -X POST -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    else
        response=$(curl -s -w "\n%{http_code}" -X POST -H "Content-Type: application/json" -H "Authorization: Bearer $token" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    fi
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$endpoint" "POST" 0 "$body"
        return 0
    else
        log_test "$endpoint" "POST" 1 "HTTP $http_code: $body"
        return 1
    fi
}

test_put() {
    local endpoint=$1
    local token=$2
    local data=$3
    local expected_code=${4:-200}
    
    local response
    response=$(curl -s -w "\n%{http_code}" -X PUT -H "Content-Type: application/json" -H "Authorization: Bearer $token" -d "$data" "$BASE_URL$endpoint" 2>/dev/null)
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$endpoint" "PUT" 0 "$body"
        return 0
    else
        log_test "$endpoint" "PUT" 1 "HTTP $http_code: $body"
        return 1
    fi
}

test_delete() {
    local endpoint=$1
    local token=$2
    local expected_code=${3:-200}
    
    local response
    response=$(curl -s -w "\n%{http_code}" -X DELETE -H "Authorization: Bearer $token" "$BASE_URL$endpoint" 2>/dev/null)
    
    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -eq "$expected_code" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
        log_test "$endpoint" "DELETE" 0 "$body"
        return 0
    else
        log_test "$endpoint" "DELETE" 1 "HTTP $http_code: $body"
        return 1
    fi
}

echo "========================================"
echo "API 全面系统性测试 - 第二轮"
echo "测试时间: $(date)"
echo "测试环境: $BASE_URL"
echo "用户: $USER_ID"
echo "========================================"
echo ""

# ========================================
# 1. 基础服务 API 测试
# ========================================
echo -e "${YELLOW}[1/39] 基础服务 API 测试${NC}"
echo "----------------------------------------"

test_get "/health" "" 200
test_get "/_matrix/client/versions" "" 200
test_get "/_matrix/client/v3/versions" "" 200
test_get "/_matrix/client/r0/version" "" 200
test_get "/_matrix/server_version" "" 200
test_get "/_matrix/client/r0/capabilities" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/capabilities" "$USER_TOKEN" 200
test_get "/.well-known/matrix/server" "" 200
test_get "/.well-known/matrix/client" "" 200
test_get "/.well-known/matrix/support" "" 200

echo ""

# ========================================
# 2. 用户认证 API 测试
# ========================================
echo -e "${YELLOW}[2/39] 用户认证 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/login" "" 200
test_get "/_matrix/client/v3/login" "" 200
test_get "/_matrix/client/r0/register/available?username=testuser_check_123" "" 200
test_get "/_matrix/client/v3/register/available?username=testuser_check_456" "" 200
test_get "/_matrix/client/r0/account/whoami" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/account/whoami" "$USER_TOKEN" 200

echo ""

# ========================================
# 3. 账户管理 API 测试
# ========================================
echo -e "${YELLOW}[3/39] 账户管理 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/profile/$USER_ID" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/profile/$USER_ID" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/profile/$USER_ID/displayname" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/profile/$USER_ID/displayname" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/profile/$USER_ID/avatar_url" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/profile/$USER_ID/avatar_url" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/account/3pid" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/account/3pid" "$USER_TOKEN" 200

echo ""

# ========================================
# 4. 房间管理 API 测试
# ========================================
echo -e "${YELLOW}[4/39] 房间管理 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/joined_rooms" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/joined_rooms" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/publicRooms" "" 200
test_get "/_matrix/client/v3/publicRooms" "" 200

echo ""

# ========================================
# 5. 设备管理 API 测试
# ========================================
echo -e "${YELLOW}[5/39] 设备管理 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/devices" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/devices" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/devices/$DEVICE_ID" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/devices/$DEVICE_ID" "$USER_TOKEN" 200

echo ""

# ========================================
# 6. 推送通知 API 测试
# ========================================
echo -e "${YELLOW}[6/39] 推送通知 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/pushers" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/pushers" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/pushrules" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/pushrules" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/notifications" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/notifications" "$USER_TOKEN" 200

echo ""

# ========================================
# 7. E2EE 加密 API 测试
# ========================================
echo -e "${YELLOW}[7/39] E2EE 加密 API 测试${NC}"
echo "----------------------------------------"

test_post "/_matrix/client/r0/keys/upload" "$USER_TOKEN" '{"device_keys":{}}' 200
test_post "/_matrix/client/v3/keys/upload" "$USER_TOKEN" '{"device_keys":{}}' 200
test_post "/_matrix/client/r0/keys/query" "$USER_TOKEN" '{"device_keys":{}}' 200
test_post "/_matrix/client/v3/keys/query" "$USER_TOKEN" '{"device_keys":{}}' 200
test_get "/_matrix/client/r0/keys/changes" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/keys/changes" "$USER_TOKEN" 200

echo ""

# ========================================
# 8. 媒体服务 API 测试
# ========================================
echo -e "${YELLOW}[8/39] 媒体服务 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/media/r0/config" "$USER_TOKEN" 200
test_get "/_matrix/media/v3/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 9. 好友系统 API 测试
# ========================================
echo -e "${YELLOW}[9/39] 好友系统 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/v1/friends" "$USER_TOKEN" 200
test_get "/_matrix/client/v1/friends/groups" "$USER_TOKEN" 200

echo ""

# ========================================
# 10. Space 空间 API 测试
# ========================================
echo -e "${YELLOW}[10/39] Space 空间 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/v1/spaces/public" "$USER_TOKEN" 200
test_get "/_matrix/client/v1/spaces/user" "$USER_TOKEN" 200

echo ""

# ========================================
# 11. Thread 线程 API 测试
# ========================================
echo -e "${YELLOW}[11/39] Thread 线程 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/v1/threads" "$USER_TOKEN" 200
test_get "/_matrix/client/v1/threads/subscribed" "$USER_TOKEN" 200
test_get "/_matrix/client/v1/threads/unread" "$USER_TOKEN" 200

echo ""

# ========================================
# 12. 搜索服务 API 测试
# ========================================
echo -e "${YELLOW}[12/39] 搜索服务 API 测试${NC}"
echo "----------------------------------------"

test_post "/_matrix/client/r0/search" "$USER_TOKEN" '{"search_categories":{"room_events":{"search_term":"test"}}}' 200
test_post "/_matrix/client/v3/search" "$USER_TOKEN" '{"search_categories":{"room_events":{"search_term":"test"}}}' 200
test_post "/_matrix/client/r0/user_directory/search" "$USER_TOKEN" '{"search_term":"test"}' 200
test_post "/_matrix/client/v3/user_directory/search" "$USER_TOKEN" '{"search_term":"test"}' 200

echo ""

# ========================================
# 13. 管理后台 API 测试 (需要管理员权限)
# ========================================
echo -e "${YELLOW}[13/39] 管理后台 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/server_version" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/server_name" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/statistics" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/users" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/rooms" "$USER_TOKEN" 200

echo ""

# ========================================
# 14. 联邦 API 测试
# ========================================
echo -e "${YELLOW}[14/39] 联邦 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/federation/v1/version" "" 200
test_get "/_matrix/key/v2/server" "" 200
test_get "/_matrix/federation/v2/server" "" 200

echo ""

# ========================================
# 15. VoIP 服务 API 测试
# ========================================
echo -e "${YELLOW}[15/39] VoIP 服务 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/voip/turnServer" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/voip/turnServer" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/voip/config" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/voip/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 16. 验证码服务 API 测试
# ========================================
echo -e "${YELLOW}[16/39] 验证码服务 API 测试${NC}"
echo "----------------------------------------"

test_post "/_synapse/admin/v1/captcha/generate" "" '{}' 200

echo ""

# ========================================
# 17. 后台更新 API 测试
# ========================================
echo -e "${YELLOW}[17/39] 后台更新 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/background_updates" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/background_updates/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 18. 事件举报 API 测试
# ========================================
echo -e "${YELLOW}[18/39] 事件举报 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/event_reports" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/event_reports/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 19. 账户数据 API 测试
# ========================================
echo -e "${YELLOW}[19/39] 账户数据 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/user/$USER_ID/account_data/m.direct" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/user/$USER_ID/account_data/m.direct" "$USER_TOKEN" 200

echo ""

# ========================================
# 20. 密钥备份 API 测试
# ========================================
echo -e "${YELLOW}[20/39] 密钥备份 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/room_keys/version" "$USER_TOKEN" 200
test_get "/_matrix/client/v3/room_keys/version" "$USER_TOKEN" 200

echo ""

# ========================================
# 21. 保留策略 API 测试
# ========================================
echo -e "${YELLOW}[21/39] 保留策略 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/retention/policies" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/retention/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 22. 服务器通知 API 测试
# ========================================
echo -e "${YELLOW}[22/39] 服务器通知 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/server_notifications" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/server_notifications/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 23. 注册令牌 API 测试
# ========================================
echo -e "${YELLOW}[23/39] 注册令牌 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/registration_tokens" "$USER_TOKEN" 200

echo ""

# ========================================
# 24. 媒体配额 API 测试
# ========================================
echo -e "${YELLOW}[24/39] 媒体配额 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/media/quota" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/media/quota/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 25. CAS 认证 API 测试
# ========================================
echo -e "${YELLOW}[25/39] CAS 认证 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/cas/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 26. SAML 认证 API 测试
# ========================================
echo -e "${YELLOW}[26/39] SAML 认证 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/saml/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 27. OIDC 认证 API 测试
# ========================================
echo -e "${YELLOW}[27/39] OIDC 认证 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/oidc/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 28. Rendezvous API 测试
# ========================================
echo -e "${YELLOW}[28/39] Rendezvous API 测试${NC}"
echo "----------------------------------------"

test_post "/_matrix/client/v1/rendezvous" "" '{}' 200

echo ""

# ========================================
# 29. Worker API 测试
# ========================================
echo -e "${YELLOW}[29/39] Worker API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/worker/v1/health" "" 200
test_get "/_synapse/worker/v1/stats" "" 200
test_get "/_synapse/worker/v1/config" "" 200
test_get "/_synapse/worker/v1/tasks" "" 200

echo ""

# ========================================
# 30. 联邦黑名单 API 测试
# ========================================
echo -e "${YELLOW}[30/39] 联邦黑名单 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/federation/blacklist" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/federation/blacklist/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 31. 联邦缓存 API 测试
# ========================================
echo -e "${YELLOW}[31/39] 联邦缓存 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/federation/cache" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/federation/cache/stats" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/federation/cache/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 32. 刷新令牌 API 测试
# ========================================
echo -e "${YELLOW}[32/39] 刷新令牌 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/refresh_tokens" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/refresh_tokens/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 33. 推送通知管理 API 测试
# ========================================
echo -e "${YELLOW}[33/39] 推送通知管理 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/push_notifications" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/push_notifications/stats" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/push_notifications/queue" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/push_notifications/providers" "$USER_TOKEN" 200

echo ""

# ========================================
# 34. 速率限制管理 API 测试
# ========================================
echo -e "${YELLOW}[34/39] 速率限制管理 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/rate_limits" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/rate_limits/stats" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/rate_limits/blocked" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/rate_limits/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 35. Sliding Sync API 测试
# ========================================
echo -e "${YELLOW}[35/39] Sliding Sync API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/unstable/org.matrix.msc3575/sync" "$USER_TOKEN" 200

echo ""

# ========================================
# 36. 遥测 API 测试
# ========================================
echo -e "${YELLOW}[36/39] 遥测 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/telemetry" "$USER_TOKEN" 200
test_get "/_synapse/admin/v1/telemetry/config" "$USER_TOKEN" 200

echo ""

# ========================================
# 37. 语音消息 API 测试
# ========================================
echo -e "${YELLOW}[37/39] 语音消息 API 测试${NC}"
echo "----------------------------------------"

test_get "/_matrix/client/r0/voice/config" "$USER_TOKEN" 200
test_get "/_matrix/client/r0/voice/stats" "$USER_TOKEN" 200

echo ""

# ========================================
# 38. 应用服务 API 测试
# ========================================
echo -e "${YELLOW}[38/39] 应用服务 API 测试${NC}"
echo "----------------------------------------"

echo "[SKIP] 应用服务 API 需要特定配置"

echo ""

# ========================================
# 39. 安全管理 API 测试
# ========================================
echo -e "${YELLOW}[39/39] 安全管理 API 测试${NC}"
echo "----------------------------------------"

test_get "/_synapse/admin/v1/security/ip/blocks" "$USER_TOKEN" 200

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

if [ $FAILED_TESTS -gt 0 ]; then
    echo -e "${RED}失败的端点:${NC}"
    for item in "${FAILED_ENDPOINTS[@]}"; do
        IFS='|' read -r endpoint response <<< "$item"
        echo "  - $endpoint"
        echo "    响应: $response"
    done
fi

echo ""
echo "测试完成时间: $(date)"
