#!/bin/bash
set -e

echo "=========================================="
echo "  测试4个3.1客户端API端点"
echo "=========================================="
echo ""

BASE_URL="http://localhost:8008"
TS=$(date +%s)
UNIQUE_ID=$(head /dev/urandom | tr -dc a-z0-9 | head -c 10)

echo "=== 1. 创建用户 ==="
REG=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"test_${UNIQUE_ID}\", \"password\": \"Password123!\"}")
TOKEN=$(echo "$REG" | sed "s/.*\"access_token\":\"\([^\"]*\)\".*/\1/")
if [ -z "$TOKEN" ] || [ "$TOKEN" = "$REG" ]; then
  echo "ERROR: 注册失败"
  exit 1
fi
echo "OK: 用户创建成功"

echo ""
echo "=== 2. 创建房间 ==="
ROOM=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"name\": \"Test${TS}\", \"visibility\": \"private\"}")
ROOM_ID=$(echo "$ROOM" | sed "s/.*\"room_id\":\"\([^\"]*\)\".*/\1/")
if [ -z "$ROOM_ID" ] || [ "$ROOM_ID" = "$ROOM" ]; then
  echo "ERROR: 创建房间失败"
  exit 1
fi
echo "OK: Room ID: $ROOM_ID"

echo ""
echo "=== 3. 发送消息 ==="
MSG=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message/txn_${TS}_msg" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"msgtype": "m.text", "body": "test"}')
EVENT_ID=$(echo "$MSG" | sed "s/.*\"event_id\":\"\([^\"]*\)\".*/\1/")
if [ -z "$EVENT_ID" ]; then
  echo "WARNING: 消息发送失败，event_id为空"
  EVENT_ID="test_event"
else
  echo "OK: Event ID: $EVENT_ID"
fi

echo ""
echo "=========================================="
echo " 测试 3.1.4-33: PUT /directory/room/{room_alias}"
echo "=========================================="
ALIAS_RESP=$(curl -s -w "\nHTTP:%{http_code}" -X PUT "$BASE_URL/_matrix/client/r0/directory/room/testalias_${UNIQUE_ID}" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"room_id\": \"$ROOM_ID\"}")
echo "$ALIAS_RESP"

echo ""
echo "=========================================="
echo " 测试 3.1.4-32: DELETE /directory/room/{room_id}"
echo "=========================================="
DELETE_RESP=$(curl -s -w "\nHTTP:%{http_code}" -X DELETE "$BASE_URL/_matrix/client/r0/directory/room/$ROOM_ID" \
  -H "Authorization: Bearer $TOKEN")
echo "$DELETE_RESP"

echo ""
echo "=========================================="
echo " 测试 3.1.7-5: POST /receipt/m.read/{event_id}"
echo "=========================================="
RECEIPT_RESP=$(curl -s -w "\nHTTP:%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/receipt/m.read/$EVENT_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{}")
echo "$RECEIPT_RESP"

echo ""
echo "=========================================="
echo " 测试 3.1.7-6: POST /read_markers"
echo "=========================================="
READ_RESP=$(curl -s -w "\nHTTP:%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/read_markers" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"event_id\": \"$EVENT_ID\"}")
echo "$READ_RESP"

echo ""
echo "=========================================="
echo "             测试结果汇总"
echo "=========================================="
echo ""
echo "API端点                           | 状态码 | 结果"
echo "-----------------------------------|--------|------"
printf "3.1.4-33 PUT /directory/room    | %s | %s\n" \
  "$(echo "$ALIAS_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2)" \
  "$([[ $(echo "$ALIAS_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2) =~ ^2 ]] && echo 'OK' || echo 'FAIL')"
printf "3.1.4-32 DELETE /directory/room  | %s | %s\n" \
  "$(echo "$DELETE_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2)" \
  "$([[ $(echo "$DELETE_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2) =~ ^2 ]] && echo 'OK' || echo 'FAIL')"
printf "3.1.7-5 POST /receipt           | %s | %s\n" \
  "$(echo "$RECEIPT_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2)" \
  "$([[ $(echo "$RECEIPT_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2) =~ ^2 ]] && echo 'OK' || echo 'FAIL')"
printf "3.1.7-6 POST /read_markers      | %s | %s\n" \
  "$(echo "$READ_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2)" \
  "$([[ $(echo "$READ_RESP" | grep -o 'HTTP:[0-9]*' | cut -d: -f2) =~ ^2 ]] && echo 'OK' || echo 'FAIL')"
