#!/bin/bash

BASE_URL="http://localhost:8008"

echo "=============================================="
echo "Phase 7 API 测试 (模块 31-35)"
echo "=============================================="

echo -e "\n[1] 注册新用户..."
REGISTER=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"ph7test1","password":"Test@123456","device_id":"PH7_TEST1"}')

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

echo -e "\n===== 模块31: Rendezvous API ====="
test_api "创建会话" "POST" "/_matrix/client/v1/rendezvous" '{"intent":"login.start"}' "200"

echo -e "\n===== 模块32: Worker API ====="
test_api "Worker健康" "GET" "/_synapse/worker/v1/health" "" "200"
test_api "Worker统计" "GET" "/_synapse/worker/v1/stats" "" "200"

echo -e "\n===== 模块33: 联邦黑名单 API ====="
test_api "获取黑名单" "GET" "/_synapse/admin/v1/federation/blacklist" "" "200"

echo -e "\n===== 模块34: 联邦缓存 API ====="
test_api "获取缓存" "GET" "/_synapse/admin/v1/federation/cache" "" "200"

echo -e "\n===== 模块35: 刷新令牌 API ====="
test_api "刷新Token" "POST" "/_matrix/client/v3/refresh" '{"refresh_token":"test"}' "401"

echo -e "\n=============================================="
echo "测试统计 (模块 31-35)"
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
