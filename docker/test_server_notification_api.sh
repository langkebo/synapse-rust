#!/bin/bash

echo "=========================================="
echo "4.39 服务器通知 API 系统测试"
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
echo "用户端点测试"
echo "=========================================="

echo ""
echo "测试 1: GET /_matrix/client/v1/notifications - 获取用户通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/notifications" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 2: PUT /_matrix/client/v1/notifications/{notification_id}/read - 标记通知已读"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/client/v1/notifications/1/read" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 3: PUT /_matrix/client/v1/notifications/{notification_id}/dismiss - 关闭通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/client/v1/notifications/1/dismiss" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 4: PUT /_matrix/client/v1/notifications/read-all - 标记所有通知已读"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/client/v1/notifications/read-all" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "=========================================="
echo "管理员端点测试"
echo "=========================================="

echo ""
echo "测试 5: GET /_matrix/admin/v1/notifications - 获取所有通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notifications?limit=10&offset=0"

echo ""
echo "测试 6: POST /_matrix/admin/v1/notifications - 创建通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/notifications" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "系统维护通知",
    "content": "系统将于今晚进行维护，请提前保存工作。",
    "notification_type": "maintenance",
    "priority": 1,
    "target_audience": "all",
    "is_dismissible": true,
    "action_url": "https://example.com/maintenance",
    "action_text": "查看详情"
  }'

echo ""
echo "测试 7: GET /_matrix/admin/v1/notifications/{notification_id} - 获取通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notifications/1"

echo ""
echo "测试 8: PUT /_matrix/admin/v1/notifications/{notification_id} - 更新通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/admin/v1/notifications/1" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "系统维护通知（已更新）",
    "content": "维护时间已调整为明天。",
    "notification_type": "maintenance",
    "priority": 2,
    "is_dismissible": true
  }'

echo ""
echo "测试 9: DELETE /_matrix/admin/v1/notifications/{notification_id} - 删除通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X DELETE \
  "http://localhost:8008/_matrix/admin/v1/notifications/999"

echo ""
echo "测试 10: POST /_matrix/admin/v1/notifications/{notification_id}/deactivate - 停用通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/notifications/1/deactivate" \
  -H "Content-Type: application/json"

echo ""
echo "测试 11: POST /_matrix/admin/v1/notifications/{notification_id}/schedule - 调度通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/notifications/1/schedule" \
  -H "Content-Type: application/json" \
  -d '{"scheduled_for": "2026-02-20T10:00:00Z"}'

echo ""
echo "测试 12: POST /_matrix/admin/v1/notifications/{notification_id}/broadcast - 广播通知"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/notifications/1/broadcast" \
  -H "Content-Type: application/json" \
  -d '{"delivery_method": "push"}'

echo ""
echo "=========================================="
echo "通知模板测试"
echo "=========================================="

echo ""
echo "测试 13: GET /_matrix/admin/v1/notification-templates - 获取通知模板列表"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notification-templates"

echo ""
echo "测试 14: POST /_matrix/admin/v1/notification-templates - 创建通知模板"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/notification-templates" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "maintenance_template",
    "title_template": "系统维护通知",
    "content_template": "系统将于 {{date}} 进行维护。",
    "notification_type": "maintenance",
    "variables": ["date"]
  }'

echo ""
echo "测试 15: GET /_matrix/admin/v1/notification-templates/{name} - 获取模板"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/notification-templates/maintenance_template"

echo ""
echo "测试 16: DELETE /_matrix/admin/v1/notification-templates/{name} - 删除模板"
curl -s -w "\nHTTP Status: %{http_code}\n" -X DELETE \
  "http://localhost:8008/_matrix/admin/v1/notification-templates/nonexistent_template"

echo ""
echo "=========================================="
echo "服务器通知 API 测试完成"
echo "=========================================="
