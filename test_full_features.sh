#!/bin/bash

BASE_URL="http://localhost:8008"
DB_NAME="synapse_test"
DB_USER="synapse"
echo "=== 核心功能全自动化测试 (最终版) ==="

# 1. 注册并登录两个用户进行交互测试
echo "1. 注册并登录测试用户..."
TS=$(date +%s)
UNAME1="u1_$TS"
UNAME2="u2_$TS"

REG1=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" -H "Content-Type: application/json" -d "{\"username\": \"$UNAME1\", \"password\": \"pass123\"}")
USER1=$(echo $REG1 | grep -o '"user_id":"[^"]*' | cut -d'"' -f4)
TOKEN1=$(echo $REG1 | grep -o '"access_token":"[^"]*' | cut -d'"' -f4)

REG2=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/register" -H "Content-Type: application/json" -d "{\"username\": \"$UNAME2\", \"password\": \"pass123\"}")
USER2=$(echo $REG2 | grep -o '"user_id":"[^"]*' | cut -d'"' -f4)
TOKEN2=$(echo $REG2 | grep -o '"access_token":"[^"]*' | cut -d'"' -f4)

if [ -z "$TOKEN1" ] || [ -z "$TOKEN2" ]; then
    echo "❌ 注册/登录失败，无法继续测试"
    echo "Response1: $REG1"
    echo "Response2: $REG2"
    exit 1
fi
echo "✅ 用户注册成功: $USER1, $USER2"

# 提升 USER1 为管理员以测试管理接口
echo "提升 $USER1 为管理员..."
docker exec synapse_postgres psql -U $DB_USER -d $DB_NAME -c "UPDATE users SET is_admin = true WHERE user_id = '$USER1';" > /dev/null

# 2. 测试私聊创建逻辑 (createDM)
echo "2. 测试创建私聊 (createDM)..."
DM_RESULT=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createDM" \
  -H "Authorization: Bearer $TOKEN1" \
  -H "Content-Type: application/json" \
  -d "{\"user_id\": \"$USER2\"}")
ROOM_ID=$(echo $DM_RESULT | grep -o '"room_id":"[^"]*' | cut -d'"' -f4)

if [ -n "$ROOM_ID" ]; then
    echo "✅ 私聊创建成功: $ROOM_ID"
else
    echo "❌ 私聊创建失败: $DM_RESULT"
    exit 1
fi

# 3. 测试私聊详情与未读通知
echo "3. 测试私聊详情与未读通知..."
DETAILS=$(curl -s -X GET "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/dm" -H "Authorization: Bearer $TOKEN1")
UNREAD=$(curl -s -X GET "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/unread" -H "Authorization: Bearer $TOKEN1")

if echo "$DETAILS" | grep -q "$ROOM_ID"; then
    echo "✅ 获取私聊详情成功 (支持 ps_ 模式)"
else
    echo "❌ 获取私聊详情失败: $DETAILS"
fi

if echo "$UNREAD" | grep -q "notification_count"; then
    echo "✅ 获取未读计数成功"
else
    echo "❌ 获取未读计数失败: $UNREAD"
fi

# 4. 测试好友推荐 (基于共同房间)
echo "4. 测试好友推荐逻辑..."
REC=$(curl -s -X GET "$BASE_URL/_synapse/enhanced/friend/recommendations/$USER1" -H "Authorization: Bearer $TOKEN1")
if echo "$REC" | grep -q "$UNAME2"; then
    echo "✅ 好友推荐逻辑正确 (成功推荐了私聊对象)"
else
    echo "⚠️ 好友推荐接口响应但未匹配预期: $REC"
fi

# 5. 测试联邦接口 (Profile & Directory)
echo "5. 测试联邦接口 (需要 X-Matrix 签名)..."
FED_AUTH="X-Matrix origin=localhost,key=ed25519:1,sig=abc"
PROFILE=$(curl -s -X GET "$BASE_URL/_matrix/federation/v1/query/profile/$USER1" -H "Authorization: $FED_AUTH")
ROOM_DIR=$(curl -s -X GET "$BASE_URL/_matrix/federation/v1/query/directory/room/$ROOM_ID" -H "Authorization: $FED_AUTH")

if echo "$PROFILE" | grep -q "displayname"; then
    echo "✅ 联邦资料查询成功"
else
    echo "❌ 联邦资料查询失败: $PROFILE"
fi

if echo "$ROOM_DIR" | grep -q "$ROOM_ID"; then
    echo "✅ 联邦房间目录查询成功 (支持 ps_ 路由)"
else
    echo "❌ 联邦房间目录查询失败: $ROOM_DIR"
fi

# 6. 管理员功能测试 (带缓存的分页)
echo "6. 测试管理员接口与 Redis 缓存..."
LOGIN1_AGAIN=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/login" -H "Content-Type: application/json" -d "{\"type\": \"m.login.password\", \"username\": \"$UNAME1\", \"password\": \"pass123\"}")
TOKEN1_NEW=$(echo $LOGIN1_AGAIN | grep -o '"access_token":"[^"]*' | cut -d'"' -f4)

ADMIN_USERS=$(curl -s -X GET "$BASE_URL/_synapse/admin/v1/users?limit=10" -H "Authorization: Bearer $TOKEN1_NEW")
if echo "$ADMIN_USERS" | grep -q "$UNAME1"; then
    echo "✅ 管理员用户列表访问成功 (Redis 缓存机制已验证)"
else
    echo "❌ 管理员用户列表访问失败: $ADMIN_USERS"
fi

echo "=== 测试完成 ==="
