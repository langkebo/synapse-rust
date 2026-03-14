#!/bin/bash

BASE_URL="http://localhost:8008"

echo "=============================================="
echo "Phase 1-5 API 测试 (模块 1-5)"
echo "=============================================="

echo -e "\n[1] 注册新用户..."
REGISTER=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"ph1test4","password":"Test@123456","device_id":"PH1_TEST4"}')

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

echo -e "\n===== 模块1: 基础服务 API ====="
test_api "健康检查" "GET" "/health" "" "200"
test_api "客户端版本" "GET" "/_matrix/client/versions" "" "200"
test_api "服务器版本" "GET" "/_matrix/client/r0/version" "" "200"
test_api "客户端能力" "GET" "/_matrix/client/v3/capabilities" "" "200"
test_api "服务器发现" "GET" "/.well-known/matrix/server" "" "200"
test_api "客户端发现" "GET" "/.well-known/matrix/client" "" "200"

echo -e "\n===== 模块3: 账户管理 API ====="
test_api "获取当前用户" "GET" "/_matrix/client/v3/account/whoami" "" "200"
test_api "获取第三方绑定" "GET" "/_matrix/client/v3/account/3pid" "" "200"

echo -e "\n===== 模块4: 房间管理 API ====="
ROOM_RESP=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/createRoom" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test Room"}')
ROOM_ID=$(echo "$ROOM_RESP" | grep -o '"room_id":"[^"]*"' | sed 's/"room_id":"//;s/"//')

if [ -n "$ROOM_ID" ]; then
  echo "[成功] Room ID: $ROOM_ID"
  test_api "创建房间" "POST" "/_matrix/client/v3/createRoom" '{"name":"Test Room"}' "200"
  test_api "获取已加入房间" "GET" "/_matrix/client/v3/joined_rooms" "" "200"
  test_api "获取房间状态" "GET" "/_matrix/client/v3/rooms/$ROOM_ID/state" "" "200"
  test_api "设置房间名称" "PUT" "/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.name" '{"name":"Updated Room"}' "200"
  test_api "获取房间信息" "GET" "/_matrix/client/v3/rooms/$ROOM_ID" "" "200"
  test_api "离开房间" "POST" "/_matrix/client/v3/rooms/$ROOM_ID/leave" '{}' "200"
else
  echo "[FAIL] 创建房间失败"
  test_api "创建房间" "POST" "/_matrix/client/v3/createRoom" '{"name":"Test Room"}' "200"
fi

echo -e "\n===== 模块5: 消息发送 API ====="

ROOM2_RESP=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/createRoom" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Message Test Room"}')
ROOM2_ID=$(echo "$ROOM2_RESP" | grep -o '"room_id":"[^"]*"' | sed 's/"room_id":"//;s/"//')

if [ -n "$ROOM2_ID" ]; then
  echo "[成功] Room2 ID: $ROOM2_ID"
  test_api "发送消息" "PUT" "/_matrix/client/v3/rooms/$ROOM2_ID/send/m.room.message/test_txn_001" '{"msgtype":"m.text","body":"Test message"}' "200"
  test_api "获取消息列表" "GET" "/_matrix/client/v3/rooms/$ROOM2_ID/messages?limit=10" "" "200"
else
  echo "[FAIL] 创建消息测试房间失败"
fi

echo -e "\n=============================================="
echo "测试统计 (模块 1-5)"
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
