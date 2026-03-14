#!/bin/bash

BASE_URL="http://localhost:8008"

echo "=============================================="
echo "Phase 2 API 测试 (模块 6-10)"
echo "=============================================="

echo -e "\n[1] 注册新用户..."
REGISTER=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"ph2test1","password":"Test@123456","device_id":"PH2_TEST1"}')

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

echo -e "\n===== 模块6: 设备管理 API ====="
test_api "获取设备列表" "GET" "/_matrix/client/v3/devices" "" "200"

DEVICE_RESP=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"ph2test2","password":"Test@123456","device_id":"PH2_TEST2"}')
DEVICE_TOKEN=$(echo "$DEVICE_RESP" | grep -o '"access_token":"[^"]*"' | sed 's/"access_token":"//;s/"//')
DEVICE_ID=$(echo "$DEVICE_RESP" | grep -o '"device_id":"[^"]*"' | sed 's/"device_id":"//;s/"//')

if [ -n "$DEVICE_ID" ]; then
  echo "[成功] Device ID: $DEVICE_ID"
  test_api "获取设备详情" "GET" "/_matrix/client/v3/devices/$DEVICE_ID" "" "200"
  test_api "更新设备" "PUT" "/_matrix/client/v3/devices/$DEVICE_ID" '{"display_name":"Test Device"}' "200"
else
  echo "[FAIL] 获取设备失败"
fi

echo -e "\n===== 模块7: 推送通知 API ====="
test_api "获取推送器列表" "GET" "/_matrix/client/v3/pushers" "" "200"
test_api "获取推送规则" "GET" "/_matrix/client/v3/pushrules" "" "200"
test_api "获取全局规则" "GET" "/_matrix/client/v3/pushrules/global" "" "200"
test_api "获取通知列表" "GET" "/_matrix/client/v3/notifications" "" "200"

echo -e "\n===== 模块8: E2EE 加密 API ====="
test_api "上传设备密钥" "POST" "/_matrix/client/v3/keys/upload" '{"device_keys":{}}' "200"
test_api "查询设备密钥" "POST" "/_matrix/client/v3/keys/query" '{"device_keys":{"@ph2test1:cjystx.top":[]}}' "200"

echo -e "\n===== 模块9: 媒体服务 API ====="
test_api "媒体配置" "GET" "/_matrix/media/v3/config" "" "200"
test_api "上传媒体" "POST" "/_matrix/media/v3/upload?filename=test.txt" "dGVzdCBjb250ZW50" "200"

echo -e "\n===== 模块10: 好友系统 API ====="
test_api "获取好友列表" "GET" "/_matrix/client/v1/friends" "" "200"
test_api "获取好友分组" "GET" "/_matrix/client/v1/friends/groups" "" "200"
test_api "获取好友请求" "GET" "/_matrix/client/v1/friends/requests/incoming" "" "200"
test_api "检查好友关系" "GET" "/_matrix/client/v1/friends/check/@ph2test1:cjystx.top" "" "200"

echo -e "\n=============================================="
echo "测试统计 (模块 6-10)"
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
