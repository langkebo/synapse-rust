#!/bin/bash

set -e

HOST="cjystx.top"
BASE_URL="http://localhost:15808"

PASS=0
FAIL=0
SKIP=0
TOTAL=0

ADMIN_USER="@admin11:cjystx.top"
ADMIN_PASS="Wzc9890951!"
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW4xMTpjanlzdHgudG9wIiwidXNlcl9pZCI6IkBhZG1pbjExOmNqeXN0eC50b3AiLCJqdGkiOiJkYTJlNWU4OS1lMGQxLTRhZTktOTY1ZC1kZDc1YjY1YmI0OWUiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzc0Njg1MzMxLCJpYXQiOjE3NzQ1OTg5MzEsImRldmljZV9pZCI6IllfajlyVm04ZG9PTXdEQmprRmUyYncifQ.MDN727a8WQznSG2Fb-X7slxYrPG-YpreGDIVtwAlR2U"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

check_admin_token() {
    if [ -z "$ADMIN_TOKEN" ]; then
        echo -e "${YELLOW}警告: 未设置 ADMIN_ACCESS_TOKEN 环境变量${NC}"
        echo -e "${YELLOW}部分需要认证的测试将被跳过${NC}"
        return 1
    fi

    local response=$(curl -sk -X GET "$BASE_URL/_matrix/client/v3/account/whoami" \
        -H "Host: $HOST" \
        -H "Authorization: Bearer $ADMIN_TOKEN" 2>/dev/null)

    if echo "$response" | grep -q "\"user_id\""; then
        echo -e "${GREEN}✅ 管理员Token有效${NC}"
        return 0
    else
        echo -e "${RED}❌ 管理员Token无效或已过期${NC}"
        echo -e "${YELLOW}请更新 ADMIN_ACCESS_TOKEN 环境变量${NC}"
        return 1
    fi
}

refresh_admin_token() {
    echo -e "${CYAN}尝试刷新管理员Token...${NC}"

    local response=$(curl -sk -X POST "$BASE_URL/_matrix/client/v3/login" \
        -H "Host: $HOST" \
        -H "Content-Type: application/json" \
        -d "{\"type\":\"m.login.password\",\"user\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\"}" 2>/dev/null)

    ADMIN_TOKEN=$(echo "$response" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)

    if [ -n "$ADMIN_TOKEN" ]; then
        echo -e "${GREEN}✅ 新Token获取成功${NC}"
        echo -e "${CYAN}Token: ${ADMIN_TOKEN:0:50}...${NC}"
        return 0
    else
        echo -e "${RED}❌ Token刷新失败${NC}"
        echo -e "${YELLOW}响应: $response${NC}"
        return 1
    fi
}

echo -e "${CYAN}=========================================="
echo -e "  API 全面测试 v6 - $(date)"
echo -e "==========================================${NC}"
echo -e "${BLUE}管理员账号: ${ADMIN_USER}${NC}"
echo ""

if ! check_admin_token; then
    refresh_admin_token || true
fi
echo ""

test_api() {
    local name="$1"
    local method="$2"
    local endpoint="$3"
    local data="$4"
    local need_auth="$5"
    local is_admin="$6"

    TOTAL=$((TOTAL + 1))

    local token=""
    if [ "$is_admin" = "admin" ] || [ "$need_auth" = "auth" ] || [ "$need_auth" = "admin" ]; then
        token="$ADMIN_TOKEN"
    fi

    if [ -z "$token" ] && { [ "$need_auth" = "auth" ] || [ "$is_admin" = "admin" ] || [ "$need_auth" = "admin" ]; }; then
        echo -e "${YELLOW}⏭️ [$TOTAL] $name: 跳过 (无Token)${NC}"
        SKIP=$((SKIP + 1))
        return
    fi

    local response
    local http_code

    if [ "$method" = "GET" ]; then
        if [ -n "$token" ]; then
            response=$(curl -sk -w "\n%{http_code}" -X GET "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer $token" 2>/dev/null)
        else
            response=$(curl -sk -w "\n%{http_code}" -X GET "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" 2>/dev/null)
        fi
    else
        if [ -n "$token" ]; then
            response=$(curl -sk -w "\n%{http_code}" -X $method "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer $token" \
                -d "$data" 2>/dev/null)
        else
            response=$(curl -sk -w "\n%{http_code}" -X $method "$BASE_URL$endpoint" \
                -H "Host: $HOST" \
                -H "Content-Type: application/json" \
                -d "$data" 2>/dev/null)
        fi
    fi

    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | sed '$d')

    local status_short=$(echo "$body" | head -c 60 | tr '\n' ' ')

    if [ "$http_code" = "000" ]; then
        echo -e "${RED}❌ [$TOTAL] $name: 连接失败${NC}"
        FAIL=$((FAIL + 1))
    elif echo "$body" | grep -qE '"errcode":"M_UNAUTHORIZED"|"errcode":"M_MISSING_TOKEN"'; then
        echo -e "${RED}❌ [$TOTAL] $name: 认证失败 (401)${NC}"
        FAIL=$((FAIL + 1))
    elif echo "$body" | grep -qE '"errcode":"M_FORBIDDEN"'; then
        echo -e "${YELLOW}⚠️ [$TOTAL] $name: 禁止访问 (403)${NC}"
        SKIP=$((SKIP + 1))
    elif echo "$body" | grep -qE '"errcode":"M_NOT_FOUND"'; then
        echo -e "${YELLOW}⚠️ [$TOTAL] $name: 未找到 (404)${NC}"
        SKIP=$((SKIP + 1))
    elif echo "$body" | grep -qE '"errcode":"M_UNKNOWN"'; then
        echo -e "${RED}❌ [$TOTAL] $name: 未知错误${NC}"
        FAIL=$((FAIL + 1))
    elif echo "$body" | grep -qE '"errcode"'; then
        echo -e "${RED}❌ [$TOTAL] $name: $status_short${NC}"
        FAIL=$((FAIL + 1))
    elif [ "$http_code" -ge 400 ]; then
        echo -e "${RED}❌ [$TOTAL] $name: HTTP $http_code${NC}"
        FAIL=$((FAIL + 1))
    else
        echo -e "${GREEN}✅ [$TOTAL] $name (HTTP $http_code)${NC}"
        PASS=$((PASS + 1))
    fi
}

echo -e "\n${BLUE}=== 1. 基础服务 API (无需认证) ===${NC}"
test_api "健康检查" GET "/health"
test_api "客户端版本" GET "/_matrix/client/versions"
test_api "服务器版本" GET "/_matrix/client/r0/version"
test_api "客户端能力" GET "/_matrix/client/v3/capabilities"
test_api "Well-Known Server" GET "/.well-known/matrix/server"
test_api "Well-Known Client" GET "/.well-known/matrix/client"
test_api "Well-Known Support" GET "/.well-known/matrix/support"

echo -e "\n${BLUE}=== 2. 用户认证 API ===${NC}"
test_api "登录流程" GET "/_matrix/client/v3/login"
test_api "用户名可用性" GET "/_matrix/client/v3/register/available?username=testuser123"

echo -e "\n${BLUE}=== 3. 媒体服务 API (无需认证) ===${NC}"
test_api "媒体配置" GET "/_matrix/media/v3/config"
test_api "URL预览" GET "/_matrix/media/v3/preview_url?url=https://example.com"

echo -e "\n${BLUE}=== 4. 联邦 API (无需认证) ===${NC}"
test_api "联邦版本" GET "/_matrix/federation/v1/version"
test_api "服务器密钥" GET "/_matrix/key/v2/server"
test_api "联邦发现" GET "/.well-known/matrix/server"

echo -e "\n${CYAN}=========================================="
echo -e "  需要认证的 API 测试"
echo -e "==========================================${NC}"

echo -e "\n${BLUE}=== 5. 用户账户 API ===${NC}"
test_api "当前用户" GET "/_matrix/client/v3/account/whoami" "" "auth"
test_api "第三方ID列表" GET "/_matrix/client/v3/account/3pid" "" "auth"
test_api "设置账户数据" POST "/_matrix/client/v3/user/@admin11:cjystx.top/account_data/m.test" '{"test":"value"}' "" "auth"

echo -e "\n${BLUE}=== 6. 用户资料 API ===${NC}"
test_api "用户资料" GET "/_matrix/client/v3/profile/@admin11:cjystx.top" "" "auth"
test_api "显示名" GET "/_matrix/client/v3/profile/@admin11:cjystx.top/displayname" "" "auth"
test_api "头像URL" GET "/_matrix/client/v3/profile/@admin11:cjystx.top/avatar_url" "" "auth"

echo -e "\n${BLUE}=== 7. 房间管理 API ===${NC}"
test_api "已加入房间" GET "/_matrix/client/v3/joined_rooms" "" "auth"
test_api "公开房间" GET "/_matrix/client/v3/publicRooms"
test_api "创建房间" POST "/_matrix/client/v3/createRoom" '{"name":"Test Room API"}' "auth"

echo -e "\n${BLUE}=== 8. 设备管理 API ===${NC}"
test_api "设备列表" GET "/_matrix/client/v3/devices" "" "auth"

echo -e "\n${BLUE}=== 9. 推送通知 API ===${NC}"
test_api "推送器列表" GET "/_matrix/client/v3/pushers" "" "auth"
test_api "推送规则" GET "/_matrix/client/v3/pushrules" "" "auth"
test_api "通知列表" GET "/_matrix/client/v3/notifications" "" "auth"

echo -e "\n${BLUE}=== 10. E2EE 加密 API ===${NC}"
test_api "密钥上传" POST "/_matrix/client/v3/keys/upload" '{}' "auth"
test_api "密钥查询" POST "/_matrix/client/v3/keys/query" '{"device_keys":{}}' "auth"
test_api "密钥变更" GET "/_matrix/client/v3/keys/changes" "" "auth"
test_api "一次性密钥" POST "/_matrix/client/v3/keys/claim" '{"one_time_keys":{}}' "auth"

echo -e "\n${BLUE}=== 11. 好友系统 API ===${NC}"
test_api "好友列表" GET "/_matrix/client/v1/friends" "" "auth"
test_api "好友分组" GET "/_matrix/client/v1/friends/groups" "" "auth"

echo -e "\n${BLUE}=== 12. Space 空间 API ===${NC}"
test_api "公开空间" GET "/_matrix/client/v1/spaces/public" "" "auth"
test_api "用户空间" GET "/_matrix/client/v1/spaces/user" "" "auth"

echo -e "\n${BLUE}=== 13. 搜索服务 API ===${NC}"
test_api "消息搜索" POST "/_matrix/client/v3/search" '{"search_categories":{"room_events":{"search_term":"test"}}}' "auth"
test_api "用户搜索" POST "/_matrix/client/v3/user_directory/search" '{"search_term":"test"}' "auth"

echo -e "\n${BLUE}=== 14. 密钥备份 API ===${NC}"
test_api "备份版本" GET "/_matrix/client/v3/room_keys/version" "" "auth"
test_api "所有密钥" GET "/_matrix/client/v3/room_keys/keys" "" "auth"

echo -e "\n${BLUE}=== 15. VoIP 服务 API ===${NC}"
test_api "TURN服务器" GET "/_matrix/client/v3/voip/turnServer" "" "auth"

echo -e "\n${BLUE}=== 16. 语音消息 API ===${NC}"
test_api "语音配置" GET "/_matrix/client/r0/voice/config" "" "auth"

echo -e "\n${BLUE}=== 17. Sliding Sync API ===${NC}"
test_api "Sliding Sync" POST "/_matrix/client/unstable/org.matrix.msc3575/sync" '{"lists":[{"list_key":"joined","sort":["by_activity"],"ranges":[[0,99]]}]}' "auth"

echo -e "\n${CYAN}=========================================="
echo -e "  管理员 API 测试"
echo -e "==========================================${NC}"

echo -e "\n${BLUE}=== 18. 管理后台 - 服务器状态 ===${NC}"
test_api "服务器版本(admin)" GET "/_synapse/admin/v1/server_version" "" "admin"
test_api "服务器状态(admin)" GET "/_synapse/admin/v1/status" "" "admin"
test_api "服务器统计(admin)" GET "/_synapse/admin/v1/statistics" "" "admin"

echo -e "\n${BLUE}=== 19. 管理后台 - 用户管理 ===${NC}"
test_api "用户列表(admin)" GET "/_synapse/admin/v1/users?limit=10&offset=0" "" "admin"
test_api "用户数量(admin)" GET "/_synapse/admin/v1/users/count" "" "admin"
test_api "指定用户(admin)" GET "/_synapse/admin/v1/users/@admin:cjystx.top" "" "admin"

echo -e "\n${BLUE}=== 20. 管理后台 - 房间管理 ===${NC}"
test_api "房间列表(admin)" GET "/_synapse/admin/v1/rooms?limit=10&offset=0" "" "admin"
test_api "房间数量(admin)" GET "/_synapse/admin/v1/rooms/count" "" "admin"

echo -e "\n${BLUE}=== 21. 管理后台 - 媒体管理 ===${NC}"
test_api "媒体列表(admin)" GET "/_synapse/admin/v1/media?limit=10&offset=0" "" "admin"

echo -e "\n${BLUE}=== 22. 管理后台 - 注册令牌 ===${NC}"
test_api "令牌列表(admin)" GET "/_synapse/admin/v1/registration_tokens" "" "admin"

echo -e "\n${BLUE}=== 23. 管理后台 - 后台更新 ===${NC}"
test_api "更新列表(admin)" GET "/_synapse/admin/v1/background_updates" "" "admin"

echo -e "\n${BLUE}=== 24. 管理后台 - 事件举报 ===${NC}"
test_api "举报列表(admin)" GET "/_synapse/admin/v1/event_reports" "" "admin"

echo -e "\n${BLUE}=== 25. 管理后台 - Worker ===${NC}"
test_api "Worker列表(admin)" GET "/_synapse/admin/v1/workers" "" "admin"

echo -e "\n${BLUE}=== 26. 管理后台 - 联邦 ===${NC}"
test_api "联邦黑名单(admin)" GET "/_synapse/admin/v1/federation/blacklist" "" "admin"

echo -e "\n${BLUE}=== 27. 管理后台 - 速率限制 ===${NC}"
test_api "速率限制列表(admin)" GET "/_synapse/admin/v1/rate_limits" "" "admin"

echo -e "\n${BLUE}=== 28. 管理后台 - 遥测 ===${NC}"
test_api "遥测状态(admin)" GET "/_synapse/admin/v1/telemetry/status" "" "admin"

echo -e "\n${BLUE}=== 29. 管理后台 - CAS/SSO 配置 ===${NC}"
test_api "CAS配置(admin)" GET "/_synapse/admin/v1/cas/config" "" "admin"
test_api "SAML配置(admin)" GET "/_synapse/admin/v1/saml/config" "" "admin"
test_api "OIDC配置(admin)" GET "/_synapse/admin/v1/oidc/config" "" "admin"

echo -e "\n${CYAN}=========================================="
echo -e "  测试完成"
echo -e "==========================================${NC}"
echo ""
echo -e "${BLUE}总计: ${TOTAL} 个测试${NC}"
echo -e "${GREEN}通过: ${PASS} 个${NC}"
echo -e "${RED}失败: ${FAIL} 个${NC}"
echo -e "${YELLOW}跳过: ${SKIP} 个 (无Token/403/404)${NC}"
echo ""

if [ $FAIL -eq 0 ]; then
    echo -e "${GREEN}🎉 所有测试通过!${NC}"
    exit 0
else
    echo -e "${RED}❌ 有 $FAIL 个测试失败${NC}"
    exit 1
fi
