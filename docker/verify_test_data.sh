#!/bin/bash

echo "======================================"
echo "🔐 Synapse Rust 测试数据验证脚本"
echo "======================================"
echo ""

SERVER="http://localhost:8008"
PASSWORD="TestUser123!"

echo "📋 测试环境信息:"
echo "  服务器: $SERVER"
echo "  测试域名: cjystx.top"
echo ""

echo "======================================"
echo "✅ 步骤1: 验证测试用户登录状态"
echo "======================================"
echo ""

USERS_OK=0
USERS_FAIL=0

for i in {1..6}; do
  USER="testuser$i"
  RESULT=$(curl -s -X POST "$SERVER/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$USER\", \"password\": \"$PASSWORD\"}")

  if echo "$RESULT" | jq -e '.access_token' > /dev/null 2>&1; then
    USER_ID=$(echo "$RESULT" | jq -r '.user_id')
    echo "  ✅ $USER -> $USER_ID"
    ((USERS_OK++))
  else
    ERROR=$(echo "$RESULT" | jq -r '.errcode // "未知错误"')
    echo "  ❌ $USER -> $ERROR"
    ((USERS_FAIL++))
  fi
done

echo ""
echo "📊 用户统计: $USERS_OK 成功, $USERS_FAIL 失败"
echo ""

echo "======================================"
echo "✅ 步骤2: 获取用户 Token (供后续使用)"
echo "======================================"
echo ""

for i in {1..6}; do
  USER="testuser$i"
  TOKEN=$(curl -s -X POST "$SERVER/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$USER\", \"password\": \"$PASSWORD\"}" | jq -r '.access_token')

  echo "$USER_TOKEN=$TOKEN"
done > /tmp/test_tokens.txt

echo "Token 已保存到 /tmp/test_tokens.txt"
echo ""

echo "======================================"
echo "✅ 步骤3: 验证测试房间存在"
echo "======================================"
echo ""

ROOMS_OK=0
ROOMS_FAIL=0

ROOMS=(
  "!S1G22nzHWJW6yPmh9mMROB3y:cjystx.top:核心功能测试房间"
  "!EW-kKDLCGAwNsABC7ILNgW-Y:cjystx.top:好友测试房间"
  "!CZCjidUUpt1hSxCtiRwrdtIu:cjystx.top:联邦测试房间"
  "!NzYF8372_NPlNBmzJrjJX5gV:cjystx.top:设备测试房间"
  "!zssB-Il0YHxhox8j7JPlCHxf:cjystx.top:公共测试房间"
)

for ROOM_INFO in "${ROOMS[@]}"; do
  IFS=':' read -r ROOM_ID NAME <<< "$ROOM_INFO"

  TOKEN=$(cat /tmp/test_tokens.txt | grep "testuser1" | cut -d'=' -f2)

  RESULT=$(curl -s -X GET "$SERVER/_matrix/client/r0/rooms/$ROOM_ID/state" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json")

  if echo "$RESULT" | jq -e '.errcode' > /dev/null 2>&1; then
    echo "  ❌ $NAME ($ROOM_ID) - 未找到"
    ((ROOMS_FAIL++))
  else
    echo "  ✅ $NAME ($ROOM_ID)"
    ((ROOMS_OK++))
  fi
done

echo ""
echo "📊 房间统计: $ROOMS_OK 成功, $ROOMS_FAIL 失败"
echo ""

echo "======================================"
echo "📝 测试数据文件位置"
echo "======================================"
echo ""
echo "  配置文件: docker/test_data.json"
echo "  API文档: docs/synapse-rust/api-reference.md"
echo ""

echo "======================================"
echo "🚀 快速开始"
echo "======================================"
echo ""
echo "  # 1. 获取测试用户 Token"
echo "  source <(grep 'testuser1_TOKEN=' /tmp/test_tokens.txt)"
echo ""
echo "  # 2. 发送测试消息"
echo "  curl -X POST \"\$SERVER/_matrix/client/r0/rooms/!S1G22nzHWJW6yPmh9mMROB3y:cjystx.top/send/m.room.message\" \\"
echo "    -H \"Authorization: Bearer \$testuser1_TOKEN\" \\"
echo "    -H \"Content-Type: application/json\" \\"
echo "    -d '{\"msgtype\": \"m.text\", \"body\": \"Hello World!\"}'"
echo ""
echo "======================================"
echo "✨ 验证完成!"
echo "======================================"
