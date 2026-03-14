#!/bin/bash

BASE_URL="http://localhost:8008"

echo "=============================================="
echo "Phase 3 API 测试 (模块 11-15)"
echo "=============================================="

echo -e "\n[1] 注册新用户..."
REGISTER=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"ph3test1","password":"Test@123456","device_id":"PH3_TEST1"}')

TOKEN=$(echo "$REGISTER" | grep -o '"access_token":"[^"]*"' | sed 's/"access_token":"//;s/"//')

if [ -z "$TOKEN" ]; then
  echo "[错误] 无法获取Token"
  exit 1
fi

echo "[成功] Token: ${TOKEN:0:30}..."

TOTAL=0
PASSED=0
FAILED=0

test_api() {
  local name=$1
  local method=$2
  local endpoint=$3
  local data=$4
  local expected=${5:-200}

  TOTAL=$((TOTAL + 1))

  if [ "$method" = "GET" ]; then
    response=$(curl -s -w "\n%{http_code}" -H "Authorization: Bearer $TOKEN" "$BASE_URL$endpoint")
  elif [ "$method" = "POST" ]; then
    response=$(curl -s -w "\n%{http_code}" -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint")
  elif [ "$method" = "PUT" ]; then
    response=$(curl -s -w "\n%{http_code}" -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint")
  elif [ "$method" = "DELETE" ]; then
    response=$(curl -s -w "\n%{http_code}" -X DELETE -H "Authorization: Bearer $TOKEN" "$BASE_URL$endpoint")
  fi

  http_code=$(echo "$response" | tail -n1)

  if [ "$http_code" -eq "$expected" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
    echo "[PASS] $name (HTTP $http_code)"
    PASSED=$((PASSED + 1))
    return 0
  else
    echo "[FAIL] $name (HTTP $http_code, expected $expected)"
    FAILED=$((FAILED + 1))
    return 1
  fi
}

echo -e "\n===== 模块11: Space 空间 API ====="
test_api "获取公开空间" "GET" "/_matrix/client/v1/spaces/public" "" "200"
test_api "获取用户空间" "GET" "/_matrix/client/v1/spaces/user" "" "200"
test_api "获取空间层级" "GET" "/_matrix/client/v1/spaces/hierarchy?room_id=!test:cjystx.top" "" "200"

echo -e "\n===== 模块12: Thread 线程 API ====="
test_api "获取线程列表" "GET" "/_matrix/client/v1/threads" "" "200"
test_api "获取订阅列表" "GET" "/_matrix/client/v1/threads/subscribed" "" "200"
test_api "获取未读线程" "GET" "/_matrix/client/v1/threads/unread" "" "200"

echo -e "\n===== 模块13: 搜索服务 API ====="
test_api "搜索消息" "POST" "/_matrix/client/v3/search" '{"search_categories":{"room_events":{"search_term":"test"}}}' "200"
test_api "搜索用户" "POST" "/_matrix/client/v3/user_directory/search" '{"search_term":"test"}' "200"

echo -e "\n===== 模块14: 管理后台 API ====="
test_api "服务器状态" "GET" "/_synapse/admin/v1/status" "" "200"
test_api "服务器版本" "GET" "/_synapse/admin/v1/server_version" "" "200"
test_api "服务器配置" "GET" "/_synapse/admin/v1/config" "" "200"
test_api "服务器统计" "GET" "/_synapse/admin/v1/server_stats" "" "200"

echo -e "\n===== 模块15: 联邦 API ====="
test_api "联邦版本" "GET" "/_matrix/federation/v1/version" "" "200"
test_api "查询用户资料" "GET" "/_matrix/federation/v1/query/profile?user_id=@ph3test1:cjystx.top" "" "200"
test_api "公开房间" "GET" "/_matrix/federation/v1/publicRooms" "" "200"

echo -e "\n=============================================="
echo "测试统计 (模块 11-15)"
echo "=============================================="
echo "总计: $TOTAL"
echo "通过: $PASSED"
echo "失败: $FAILED"
if [ $TOTAL -gt 0 ]; then
  echo "通过率: $(( PASSED * 100 / TOTAL ))%"
fi
echo "=============================================="

echo -e "\n[清理] 登出..."
curl -s -X POST "$BASE_URL/_matrix/client/v3/logout" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"access_token\":\"$TOKEN\"}" > /dev/null
