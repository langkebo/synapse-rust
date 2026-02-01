#!/bin/bash

BASE_URL="http://localhost:8008"
ADMIN_TOKEN=""
USER1_TOKEN=""
USER2_TOKEN=""

echo "=== 准备测试数据 ==="

# 1. 注册管理员
echo "1. 注册管理员 @admin:localhost"
resp=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "password123", "admin": true}')
ADMIN_TOKEN=$(echo $resp | grep -oP '(?<="access_token":")[^"]+')

# 2. 注册普通用户1
echo "2. 注册普通用户1 @user1:localhost"
resp=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{"username": "user1", "password": "password123"}')
USER1_TOKEN=$(echo $resp | grep -oP '(?<="access_token":")[^"]+')

# 3. 注册普通用户2
echo "3. 注册普通用户2 @user2:localhost"
resp=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{"username": "user2", "password": "password123"}')
USER2_TOKEN=$(echo $resp | grep -oP '(?<="access_token":")[^"]+')

# 4. 创建公共房间
echo "4. 创建公共房间"
resp=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "公共测试房间", "visibility": "public", "room_alias_name": "public_room"}')
PUBLIC_ROOM_ID=$(echo $resp | grep -oP '(?<="room_id":")[^"]+')

# 5. 创建私有房间并邀请 user1
echo "5. 创建私有房间并邀请 user1"
resp=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "私有测试房间", "visibility": "private", "invite": ["@user1:localhost"]}')
PRIVATE_ROOM_ID=$(echo $resp | grep -oP '(?<="room_id":")[^"]+')

# 6. user1 加入公共房间
echo "6. user1 加入公共房间"
curl -s -X POST "$BASE_URL/_matrix/client/r0/rooms/$PUBLIC_ROOM_ID/join" \
  -H "Authorization: Bearer $USER1_TOKEN"

# 7. 发送测试消息
echo "7. 发送测试消息"
curl -s -X POST "$BASE_URL/_matrix/client/r0/rooms/$PUBLIC_ROOM_ID/send/m.room.message" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"msgtype": "m.text", "body": "欢迎来到公共房间！"}'

# 8. 建立好友关系 (通过数据库直接注入或 API，目前 API 需要两步)
# user1 发送请求给 user2
echo "8. 建立好友关系"
curl -s -X POST "$BASE_URL/_synapse/enhanced/friend/request" \
  -H "Authorization: Bearer $USER1_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user_id": "@user2:localhost", "message": "交个朋友吧"}'

# 获取请求 ID 并接受
REQ_ID=$(curl -s -X GET "$BASE_URL/_synapse/enhanced/friend/requests" \
  -H "Authorization: Bearer $USER2_TOKEN" | grep -oP '(?<="request_id":)[0-9]+' | head -n 1)

if [ ! -z "$REQ_ID" ]; then
  curl -s -X POST "$BASE_URL/_synapse/enhanced/friend/request/$REQ_ID/accept" \
    -H "Authorization: Bearer $USER2_TOKEN"
fi

# 9. 创建私聊会话
echo "9. 创建私聊会话"
curl -s -X POST "$BASE_URL/_synapse/enhanced/private/sessions" \
  -H "Authorization: Bearer $USER1_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"other_user_id": "@user2:localhost"}'

echo "=== 测试数据准备完成 ==="
