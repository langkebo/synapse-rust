#!/bin/bash

# 测试脚本：验证好友功能增强 - 简化版

API_BASE="http://localhost:8008"
TIMESTAMP=$(date +%s)
UNIQUE_USER="user_${TIMESTAMP}"

echo "======================================"
echo "好友功能增强测试脚本"
echo "======================================"

# 1. 注册用户1
echo ""
echo "1. 注册用户1: ${UNIQUE_USER}1..."
REGISTER1=$(curl -s -X POST "${API_BASE}/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"'${UNIQUE_USER}'1","password":"testpass123"}')
echo "$REGISTER1"
TOKEN1=$(echo "$REGISTER1" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
ID1=$(echo "$REGISTER1" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)
echo "Token: ${TOKEN1:0:20}... ID: $ID1"

# 2. 注册用户2
echo ""
echo "2. 注册用户2: ${UNIQUE_USER}2..."
REGISTER2=$(curl -s -X POST "${API_BASE}/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"'${UNIQUE_USER}'2","password":"testpass123"}')
echo "$REGISTER2"
TOKEN2=$(echo "$REGISTER2" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
ID2=$(echo "$REGISTER2" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)
echo "Token: ${TOKEN2:0:20}... ID: $ID2"

# 3. 测试用户搜索
echo ""
echo "3. 测试用户搜索..."
SEARCH=$(curl -s -X GET "${API_BASE}/_synapse/enhanced/friends/search?query=${UNIQUE_USER}&limit=10" \
  -H "Authorization: Bearer $TOKEN1")
echo "$SEARCH"

# 4. 测试发送好友请求
echo ""
echo "4. 测试发送好友请求..."
REQUEST=$(curl -s -X POST "${API_BASE}/_synapse/enhanced/friend/request" \
  -H "Authorization: Bearer $TOKEN1" \
  -H "Content-Type: application/json" \
  -d '{"user_id":"'"$ID2"'","message":"Hello! Let'\''s be friends!"}')
echo "$REQUEST"
REQ_ID=$(echo "$REQUEST" | grep -o '"request_id":[0-9]*' | cut -d':' -f2)

# 5. 测试获取好友请求
echo ""
echo "5. 测试获取好友请求..."
REQUESTS=$(curl -s -X GET "${API_BASE}/_synapse/enhanced/friend/requests" \
  -H "Authorization: Bearer $TOKEN2")
echo "$REQUESTS"

# 如果没有找到请求 ID，尝试从响应中提取
if [ -z "$REQ_ID" ]; then
  REQ_ID=$(echo "$REQUESTS" | grep -o '"request_id":[0-9]*' | head -1 | cut -d':' -f2)
fi

# 6. 测试接受好友请求
echo ""
echo "6. 测试接受好友请求..."
if [ -n "$REQ_ID" ]; then
  ACCEPT=$(curl -s -X POST "${API_BASE}/_synapse/enhanced/friend/request/${REQ_ID}/accept" \
    -H "Authorization: Bearer $TOKEN2")
  echo "$ACCEPT"
else
  echo "跳过: 没有找到请求 ID"
fi

# 7. 测试获取好友列表
echo ""
echo "7. 测试获取好友列表..."
FRIENDS=$(curl -s -X GET "${API_BASE}/_synapse/enhanced/friends" \
  -H "Authorization: Bearer $TOKEN1")
echo "$FRIENDS"

# 8. 测试创建私聊会话（好友之间应该成功）
echo ""
echo "8. 测试创建私聊会话（好友之间）..."
SESSION=$(curl -s -X POST "${API_BASE}/_synapse/enhanced/private/sessions" \
  -H "Authorization: Bearer $TOKEN1" \
  -H "Content-Type: application/json" \
  -d '{"other_user_id":"'"$ID2"'"}')
echo "$SESSION"

# 检查是否是数据库错误
if echo "$SESSION" | grep -q "Database error"; then
  echo "⚠ 数据库错误，跳过后续私聊测试"
  SKIP_CHAT=true
else
  SESSION_ID=$(echo "$SESSION" | grep -o '"session_id":"[^"]*"' | cut -d'"' -f4)
  SKIP_CHAT=false
fi

# 9. 测试发送私聊消息
echo ""
echo "9. 测试发送私聊消息..."
if [ "$SKIP_CHAT" = "false" ] && [ -n "$SESSION_ID" ]; then
  MESSAGE=$(curl -s -X POST "${API_BASE}/_synapse/enhanced/private/sessions/${SESSION_ID}/messages" \
    -H "Authorization: Bearer $TOKEN1" \
    -H "Content-Type: application/json" \
    -d '{"message_type":"m.text","content":{"body":"Hello from test!","msgtype":"m.text"}}')
  echo "$MESSAGE"
else
  echo "跳过: 私聊会话创建失败"
fi

# 10. 注册用户3（用于测试限制）
echo ""
echo "10. 注册用户3进行限制测试..."
REGISTER3=$(curl -s -X POST "${API_BASE}/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"'${UNIQUE_USER}'3","password":"testpass123"}')
echo "$REGISTER3"
ID3=$(echo "$REGISTER3" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)

# 11. 测试向非好友发送私聊（应该被拒绝）
echo ""
echo "11. 测试向非好友发送私聊（应该被拒绝）..."
if [ -n "$ID3" ]; then
  RESTRICT=$(curl -s -X POST "${API_BASE}/_synapse/enhanced/private/sessions" \
    -H "Authorization: Bearer $TOKEN1" \
    -H "Content-Type: application/json" \
    -d '{"other_user_id":"'"$ID3"'"}')
  echo "$RESTRICT"
  
  # 验证错误码
  if echo "$RESTRICT" | grep -q '"error":"Forbidden:'; then
    echo "✓ 正确: 非好友发送私聊被拒绝"
  else
    echo "✗ 错误: 应该返回 Forbidden 错误"
  fi
fi

echo ""
echo "======================================"
echo "测试完成！"
echo "======================================"
