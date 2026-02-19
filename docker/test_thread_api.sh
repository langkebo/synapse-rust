#!/bin/bash

echo "=========================================="
echo "4.37 线程 API 系统测试"
echo "=========================================="

# 获取管理员 Token
echo "1. 获取管理员 Token..."
ADMIN_TOKEN=$(curl -s -X POST "http://localhost:8008/_matrix/client/r0/login" \
  -H "Content-Type: application/json" \
  -d '{"type": "m.login.password", "user": "admin", "password": "Admin@123"}' \
  | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)

if [ -z "$ADMIN_TOKEN" ]; then
  echo "ERROR: Failed to get admin token"
  exit 1
fi
echo "Admin Token: ${ADMIN_TOKEN:0:50}..."

# 创建测试房间
echo ""
echo "2. 创建测试房间..."
ROOM_RESPONSE=$(curl -s -X POST "http://localhost:8008/_matrix/client/r0/createRoom" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Thread Test Room", "preset": "public_chat"}')

ROOM_ID=$(echo "$ROOM_RESPONSE" | grep -o '"room_id":"[^"]*"' | cut -d'"' -f4)
echo "Room ID: $ROOM_ID"

if [ -z "$ROOM_ID" ]; then
  echo "ERROR: Failed to create room"
  echo "Response: $ROOM_RESPONSE"
  exit 1
fi

# 测试 1: 创建线程
echo ""
echo "=========================================="
echo "测试 1: POST /rooms/{room_id}/threads - 创建线程"
echo "=========================================="
THREAD_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" -X POST \
  "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"root_event_id": "$thread_root_001", "content": {"msgtype": "m.text", "body": "Thread message"}}')

HTTP_CODE=$(echo "$THREAD_RESPONSE" | grep "HTTP_CODE:" | cut -d':' -f2)
RESPONSE=$(echo "$THREAD_RESPONSE" | sed '/HTTP_CODE:/d')
echo "HTTP Status: $HTTP_CODE"
echo "Response: $RESPONSE"

THREAD_ID=$(echo "$RESPONSE" | grep -o '"thread_id":"[^"]*"' | cut -d'"' -f4)
echo "Thread ID: $THREAD_ID"

# 测试 2: 获取线程列表
echo ""
echo "=========================================="
echo "测试 2: GET /rooms/{room_id}/threads - 获取线程列表"
echo "=========================================="
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads?limit=10" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# 测试 3: 搜索线程
echo ""
echo "=========================================="
echo "测试 3: GET /rooms/{room_id}/threads/search - 搜索线程"
echo "=========================================="
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/search?q=thread&limit=10" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# 测试 4: 获取未读线程
echo ""
echo "=========================================="
echo "测试 4: GET /rooms/{room_id}/threads/unread - 获取未读线程"
echo "=========================================="
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/unread" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

if [ -n "$THREAD_ID" ]; then
  # 测试 5: 获取单个线程
  echo ""
  echo "=========================================="
  echo "测试 5: GET /rooms/{room_id}/threads/{thread_id} - 获取线程"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 6: 添加回复
  echo ""
  echo "=========================================="
  echo "测试 6: POST /rooms/{room_id}/threads/{thread_id}/replies - 添加回复"
  echo "=========================================="
  REPLY_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/replies" \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"event_id": "$reply_001", "root_event_id": "$thread_root_001", "content": {"msgtype": "m.text", "body": "Reply message"}}')
  echo "$REPLY_RESPONSE"

  # 测试 7: 获取回复列表
  echo ""
  echo "=========================================="
  echo "测试 7: GET /rooms/{room_id}/threads/{thread_id}/replies - 获取回复列表"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/replies?limit=10" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 8: 订阅线程
  echo ""
  echo "=========================================="
  echo "测试 8: POST /rooms/{room_id}/threads/{thread_id}/subscribe - 订阅线程"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/subscribe" \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"notification_level": "all"}'

  # 测试 9: 取消订阅线程
  echo ""
  echo "=========================================="
  echo "测试 9: POST /rooms/{room_id}/threads/{thread_id}/unsubscribe - 取消订阅线程"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/unsubscribe" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 10: 静音线程
  echo ""
  echo "=========================================="
  echo "测试 10: POST /rooms/{room_id}/threads/{thread_id}/mute - 静音线程"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/mute" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 11: 标记线程已读
  echo ""
  echo "=========================================="
  echo "测试 11: POST /rooms/{room_id}/threads/{thread_id}/read - 标记线程已读"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/read" \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"event_id": "$reply_001", "origin_server_ts": 1771495000000}'

  # 测试 12: 获取线程统计
  echo ""
  echo "=========================================="
  echo "测试 12: GET /rooms/{room_id}/threads/{thread_id}/stats - 获取线程统计"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/stats" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 13: 冻结线程
  echo ""
  echo "=========================================="
  echo "测试 13: POST /rooms/{room_id}/threads/{thread_id}/freeze - 冻结线程"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/freeze" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 14: 解冻线程
  echo ""
  echo "=========================================="
  echo "测试 14: POST /rooms/{room_id}/threads/{thread_id}/unfreeze - 解冻线程"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}/unfreeze" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 15: 撤回回复
  echo ""
  echo "=========================================="
  echo "测试 15: POST /rooms/{room_id}/replies/{event_id}/redact - 撤回回复"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/replies/\$reply_001/redact" \
    -H "Authorization: Bearer $ADMIN_TOKEN"

  # 测试 16: 删除线程
  echo ""
  echo "=========================================="
  echo "测试 16: DELETE /rooms/{room_id}/threads/{thread_id} - 删除线程"
  echo "=========================================="
  curl -s -w "\nHTTP Status: %{http_code}\n" -X DELETE \
    "http://localhost:8008/_matrix/client/v1/rooms/${ROOM_ID}/threads/${THREAD_ID}" \
    -H "Authorization: Bearer $ADMIN_TOKEN"
else
  echo ""
  echo "WARNING: Thread ID not found, skipping thread-specific tests"
fi

echo ""
echo "=========================================="
echo "线程 API 测试完成"
echo "=========================================="
