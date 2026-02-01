#!/bin/bash

BASE_URL="http://localhost:8008"

echo "=== 测试 API 端点 ==="
echo ""

echo "1. 测试注册用户"
REGISTER_RESULT=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{"username": "testuser1", "password": "testpass123"}')
echo "注册结果: $REGISTER_RESULT"
echo ""

echo "2. 测试登录"
LOGIN_RESULT=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/login" \
  -H "Content-Type: application/json" \
  -d '{"type": "m.login.password", "username": "testuser1", "password": "testpass123"}')
echo "登录结果: $LOGIN_RESULT"
TOKEN=$(echo $LOGIN_RESULT | grep -o '"access_token":"[^"]*' | cut -d'"' -f4)
echo "获取到的Token: $TOKEN"
echo ""

if [ -n "$TOKEN" ] && [ "$TOKEN" != "null" ]; then
    echo "3. 测试获取设备列表"
    DEVICES_RESULT=$(curl -s -X GET "$BASE_URL/_matrix/client/r0/devices" \
      -H "Authorization: Bearer $TOKEN")
    echo "设备列表: $DEVICES_RESULT"
    echo ""

    echo "4. 测试创建房间"
    ROOM_RESULT=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      -d '{"name": "测试房间", "topic": "这是一个测试房间"}')
    echo "创建房间结果: $ROOM_RESULT"
    ROOM_ID=$(echo $ROOM_RESULT | grep -o '"room_id":"[^"]*' | cut -d'"' -f4)
    echo "房间ID: $ROOM_ID"
    echo ""

    echo "5. 测试获取公共房间列表"
    PUBLIC_ROOMS=$(curl -s -X GET "$BASE_URL/_matrix/client/r0/publicRooms" \
      -H "Authorization: Bearer $TOKEN")
    echo "公共房间列表: $PUBLIC_ROOMS"
    echo ""

    echo "6. 测试获取好友列表"
    FRIENDS=$(curl -s -X GET "$BASE_URL/_synapse/enhanced/friends" \
      -H "Authorization: Bearer $TOKEN")
    echo "好友列表: $FRIENDS"
    echo ""

    echo "7. 测试获取私聊会话"
    SESSIONS=$(curl -s -X GET "$BASE_URL/_synapse/enhanced/private/sessions" \
      -H "Authorization: Bearer $TOKEN")
    echo "私聊会话: $SESSIONS"
    echo ""

    echo "8. 测试获取未读消息数"
    UNREAD=$(curl -s -X GET "$BASE_URL/_synapse/enhanced/private/unread-count" \
      -H "Authorization: Bearer $TOKEN")
    echo "未读消息数: $UNREAD"
    echo ""

    echo "9. 测试获取语音统计"
    VOICE_STATS=$(curl -s -X GET "$BASE_URL/_matrix/client/r0/voice/user/testuser1/stats" \
      -H "Authorization: Bearer $TOKEN")
    echo "语音统计: $VOICE_STATS"
    echo ""

    echo "10. 测试服务器状态"
    SERVER_STATUS=$(curl -s -X GET "$BASE_URL/_synapse/admin/v1/status")
    echo "服务器状态: $SERVER_STATUS"
    echo ""

    echo "11. 测试联邦版本"
    VERSION=$(curl -s -X GET "$BASE_URL/_matrix/federation/v1/version")
    echo "联邦版本: $VERSION"
    echo ""

else
    echo "无法获取Token，跳过需要认证的测试"
fi

echo "=== API 测试完成 ==="
