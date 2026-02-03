#!/bin/bash

# 创建管理员账户并测试管理员API的脚本

BASE_URL="http://localhost:8008"

# 管理员账户信息
ADMIN_USERNAME="testadmin"
ADMIN_PASSWORD="AdminPass123!"
ADMIN_USER_ID="@testadmin:matrix.cjystx.top"

echo "=== 创建管理员账户 ==="

# 步骤1: 创建管理员账户
echo "步骤1: 创建管理员账户..."
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/register" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$ADMIN_USERNAME\",\"password\":\"$ADMIN_PASSWORD\",\"auth\":{\"type\":\"m.login.dummy\"}}" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "400" ]; then
    echo "✅ 管理员账户创建/检查成功"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
else
    echo "❌ 管理员账户创建失败"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
    exit 1
fi

# 步骤2: 管理员登录
echo ""
echo "步骤2: 管理员登录..."
RESULT=$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"user\":\"$ADMIN_USERNAME\",\"password\":\"$ADMIN_PASSWORD\"}" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)

if [ "$HTTP_CODE" = "200" ]; then
    echo "✅ 管理员登录成功"
    ADMIN_TOKEN=$(echo "$RESPONSE_BODY" | jq -r '.access_token // empty')
    echo "管理员Token: $ADMIN_TOKEN"
else
    echo "❌ 管理员登录失败"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
    exit 1
fi

# 步骤3: 设置管理员权限
echo ""
echo "步骤3: 设置管理员权限..."
RESULT=$(docker exec synapse_postgres psql -U synapse -d synapse_test -c "UPDATE users SET is_admin = TRUE WHERE user_id = '$ADMIN_USER_ID';" 2>&1)

if [ $? -eq 0 ]; then
    echo "✅ 管理员权限设置成功"
else
    echo "❌ 管理员权限设置失败"
    echo "错误信息: $RESULT"
    exit 1
fi

# 步骤4: 测试管理员API - 获取服务器版本
echo ""
echo "步骤4: 测试管理员API - 获取服务器版本..."
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_synapse/admin/v1/server_version" \
    -H "Authorization: Bearer $ADMIN_TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)

if [ "$HTTP_CODE" = "200" ]; then
    echo "✅ 管理员API - 获取服务器版本成功"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
else
    echo "❌ 管理员API - 获取服务器版本失败"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
fi

# 步骤5: 测试管理员API - 获取用户列表
echo ""
echo "步骤5: 测试管理员API - 获取用户列表..."
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_synapse/admin/v1/users?limit=10" \
    -H "Authorization: Bearer $ADMIN_TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)

if [ "$HTTP_CODE" = "200" ]; then
    echo "✅ 管理员API - 获取用户列表成功"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
else
    echo "❌ 管理员API - 获取用户列表失败"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
fi

# 步骤6: 测试管理员API - 获取房间列表
echo ""
echo "步骤6: 测试管理员API - 获取房间列表..."
RESULT=$(curl -s -w "\n%{http_code}" -X GET "$BASE_URL/_synapse/admin/v1/rooms?limit=10" \
    -H "Authorization: Bearer $ADMIN_TOKEN" 2>&1)
HTTP_CODE=$(echo "$RESULT" | tail -n1)
RESPONSE_BODY=$(echo "$RESULT" | head -n -1)

if [ "$HTTP_CODE" = "200" ]; then
    echo "✅ 管理员API - 获取房间列表成功"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
else
    echo "❌ 管理员API - 获取房间列表失败"
    echo "HTTP状态码: $HTTP_CODE"
    echo "响应体: $RESPONSE_BODY"
fi

echo ""
echo "=== 管理员账户创建和测试完成 ==="
echo "管理员用户名: $ADMIN_USERNAME"
echo "管理员用户ID: $ADMIN_USER_ID"
echo "管理员Token: $ADMIN_TOKEN"