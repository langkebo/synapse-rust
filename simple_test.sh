#!/bin/bash
set -e

BASE_URL="http://localhost:8008"
UNIQUE_ID=$(head /dev/urandom | tr -dc a-z0-9 | head -c 8)

echo "=== 创建用户 ==="
REG=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"user_${UNIQUE_ID}\", \"password\": \"Password123!\"}")
TOKEN=$(echo "$REG" | sed "s/.*\"access_token\":\"\([^\"]*\)\".*/\1/")
echo "Token: ${TOKEN:0:30}..."

echo ""
echo "=== 创建房间 ==="
ROOM=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Test", "visibility": "private"}')
ROOM_ID=$(echo "$ROOM" | sed "s/.*\"room_id\":\"\([^\"]*\)\".*/\1/")
echo "Room: $ROOM_ID"

echo ""
echo "=== 获取房间详情（检查创建者） ==="
ROOM_DETAIL=$(curl -s -X GET "$BASE_URL/_matrix/client/r0/directory/room/$ROOM_ID" \
  -H "Authorization: Bearer $TOKEN")
echo "Room Detail: $ROOM_DETAIL"

echo ""
echo "=== 发送消息 (PUT) ==="
MSG=$(curl -s -X PUT "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message/txn_${UNIQUE_ID}" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"msgtype": "m.text", "body": "hello"}')
echo "Message Response: $MSG"
EVENT_ID=$(echo "$MSG" | sed "s/.*\"event_id\":\"\([^\"]*\)\".*/\1/")
echo "Event ID: $EVENT_ID"

echo ""
echo "=== 测试 3.1.4-33: PUT /directory/room/{room_alias} ==="
ALIAS_R=$(curl -s -w "\nHTTP:%{http_code}" -X PUT "$BASE_URL/_matrix/client/r0/directory/room/alias_${UNIQUE_ID}" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"room_id\": \"$ROOM_ID\"}")
echo "$ALIAS_R"

echo ""
echo "=== 测试 3.1.4-32: DELETE /directory/room/{room_id} ==="
DELETE_R=$(curl -s -w "\nHTTP:%{http_code}" -X DELETE "$BASE_URL/_matrix/client/r0/directory/room/$ROOM_ID" \
  -H "Authorization: Bearer $TOKEN")
echo "$DELETE_R"
