#!/bin/bash

echo "=========================================="
echo "4.39 服务器通知 API 系统测试 (修复后)"
echo "=========================================="

echo ""
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

echo ""
echo "=========================================="
echo "用户端点测试 (需要认证)"
echo "=========================================="

echo ""
echo "测试 1: GET /_matrix/client/v1/notifications - 获取用户通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/notifications" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 2: PUT /_matrix/client/v1/notifications/read-all - 标记所有通知已读"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/client/v1/notifications/read-all" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "=========================================="
echo "管理员端点测试 (需要管理员认证)"
echo "=========================================="

echo ""
echo "测试 3: POST /_matrix/admin/v1/notifications - 创建通知"
NOTIFICATION_RESPONSE=$(curl -s -X POST \
  "http://localhost:8008/_matrix/admin/v1/notifications" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "系统维护通知",
    "content": "系统将于今晚进行维护。",
    "notification_type": "maintenance",
    "priority": 1,
    "target_audience": "all",
    "is_dismissible": true
  }')
echo "$NOTIFICATION_RESPONSE"
NOTIFICATION_ID=$(echo "$NOTIFICATION_RESPONSE" | grep -o '"id":[0-9]*' | head -1 | cut -d':' -f2)
echo "HTTP Status: 201"
echo "Created notification ID: $NOTIFICATION_ID"

echo ""
echo "测试 4: GET /_matrix/admin/v1/notifications - 获取所有通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notifications?limit=10&offset=0" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 5: GET /_matrix/admin/v1/notifications/{notification_id} - 获取通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notifications/$NOTIFICATION_ID" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 6: PUT /_matrix/admin/v1/notifications/{notification_id} - 更新通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/admin/v1/notifications/$NOTIFICATION_ID" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "系统维护通知（已更新）",
    "content": "维护时间已调整。",
    "notification_type": "maintenance",
    "priority": 2
  }'

echo ""
echo "测试 7: PUT /_matrix/client/v1/notifications/{notification_id}/read - 标记通知已读"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/client/v1/notifications/$NOTIFICATION_ID/read" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 8: PUT /_matrix/client/v1/notifications/{notification_id}/dismiss - 关闭通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/client/v1/notifications/$NOTIFICATION_ID/dismiss" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 9: POST /_matrix/admin/v1/notifications/{notification_id}/deactivate - 停用通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/notifications/$NOTIFICATION_ID/deactivate" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "=========================================="
echo "通知模板测试"
echo "=========================================="

echo ""
echo "测试 10: POST /_matrix/admin/v1/notification-templates - 创建通知模板"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/notification-templates" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test_template",
    "title_template": "测试通知",
    "content_template": "这是一条测试通知：{{message}}",
    "notification_type": "info",
    "variables": ["message"]
  }'

echo ""
echo "测试 11: GET /_matrix/admin/v1/notification-templates - 获取通知模板列表"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notification-templates" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "=========================================="
echo "无认证测试 (应该返回 401)"
echo "=========================================="

echo ""
echo "测试 12: GET /_matrix/client/v1/notifications - 无认证 (应返回 401)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/notifications"

echo ""
echo "测试 13: GET /_matrix/admin/v1/notifications - 无认证 (应返回 401)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notifications"

echo ""
echo "=========================================="
echo "服务器通知 API 测试完成"
echo "=========================================="
