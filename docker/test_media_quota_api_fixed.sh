#!/bin/bash

echo "=========================================="
echo "4.38 媒体配额 API 系统测试 (修复后)"
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
echo "测试 1: GET /_matrix/media/v1/quota/check - 检查配额 (带认证)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/media/v1/quota/check?file_size=1024" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 2: GET /_matrix/media/v1/quota/stats - 获取使用统计 (带认证)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/media/v1/quota/stats" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 3: GET /_matrix/media/v1/quota/alerts - 获取告警 (带认证)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/media/v1/quota/alerts" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 4: POST /_matrix/media/v1/quota/upload - 记录上传 (带认证)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/media/v1/quota/upload" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"media_id": "test_media_001", "file_size": 1024, "mime_type": "image/png"}'

echo ""
echo "=========================================="
echo "管理员端点测试 (需要管理员认证)"
echo "=========================================="

echo ""
echo "测试 5: GET /_matrix/admin/v1/media/quota/configs - 获取配额配置列表"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/media/quota/configs" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 6: POST /_matrix/admin/v1/media/quota/configs - 创建配额配置"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/media/quota/configs" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "premium", "description": "Premium user quota", "max_storage_bytes": 107374182400, "max_file_size_bytes": 524288000, "max_files_count": 5000}'

echo ""
echo "测试 7: GET /_matrix/admin/v1/media/quota/server - 获取服务器配额"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/media/quota/server" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 8: PUT /_matrix/admin/v1/media/quota/server - 设置服务器配额"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/admin/v1/media/quota/server" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"max_storage_bytes": 214748364800}'

echo ""
echo "测试 9: POST /_matrix/admin/v1/media/quota/users - 设置用户配额"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/media/quota/users" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user_id": "@admin:cjystx.top", "custom_max_storage_bytes": 21474836480}'

echo ""
echo "=========================================="
echo "无认证测试 (应该返回 401)"
echo "=========================================="

echo ""
echo "测试 10: GET /_matrix/media/v1/quota/check - 无认证 (应返回 401)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/media/v1/quota/check?file_size=1024"

echo ""
echo "测试 11: GET /_matrix/admin/v1/media/quota/configs - 无认证 (应返回 401)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/media/quota/configs"

echo ""
echo "=========================================="
echo "媒体配额 API 测试完成"
echo "=========================================="
