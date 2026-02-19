#!/bin/bash

echo "=========================================="
echo "4.38 媒体配额 API 系统测试"
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
echo "测试文档定义的端点"
echo "=========================================="

echo ""
echo "测试 1: GET /_matrix/client/v1/media/quota - 获取用户配额"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/media/quota" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 2: GET /_matrix/client/v1/media/quota/usage - 获取用户使用量"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/media/quota/usage" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 3: GET /_matrix/client/v1/media/quota/check - 检查配额限制"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/client/v1/media/quota/check?file_size=1024" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 4: GET /_synapse/admin/v1/media/quota/{user_id} - 获取用户配额 (Admin)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_synapse/admin/v1/media/quota/@admin:cjystx.top" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 5: PUT /_synapse/admin/v1/media/quota/{user_id} - 设置用户配额"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_synapse/admin/v1/media/quota/@admin:cjystx.top" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"max_storage_bytes": 1073741824}'

echo ""
echo "测试 6: GET /_synapse/admin/v1/media/quota/server - 获取服务器配额"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_synapse/admin/v1/media/quota/server" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "测试 7: PUT /_synapse/admin/v1/media/quota/server - 设置服务器配额"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_synapse/admin/v1/media/quota/server" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"max_storage_bytes": 107374182400}'

echo ""
echo "测试 8: GET /_synapse/admin/v1/media/quota/stats - 获取配额统计"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_synapse/admin/v1/media/quota/stats" \
  -H "Authorization: Bearer $ADMIN_TOKEN"

echo ""
echo "=========================================="
echo "测试源码实现的端点"
echo "=========================================="

echo ""
echo "测试 9: GET /_matrix/media/v1/quota/check - 检查配额 (源码路径)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/media/v1/quota/check?file_size=1024"

echo ""
echo "测试 10: GET /_matrix/media/v1/quota/stats - 获取使用统计 (源码路径)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/media/v1/quota/stats"

echo ""
echo "测试 11: GET /_matrix/media/v1/quota/alerts - 获取告警 (源码路径)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/media/v1/quota/alerts"

echo ""
echo "测试 12: GET /_matrix/admin/v1/media/quota/configs - 获取配额配置列表 (源码路径)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/media/quota/configs"

echo ""
echo "测试 13: GET /_matrix/admin/v1/media/quota/server - 获取服务器配额 (源码路径)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X GET \
  "http://localhost:8008/_matrix/admin/v1/media/quota/server"

echo ""
echo "测试 14: PUT /_matrix/admin/v1/media/quota/server - 设置服务器配额 (源码路径)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X PUT \
  "http://localhost:8008/_matrix/admin/v1/media/quota/server" \
  -H "Content-Type: application/json" \
  -d '{"max_storage_bytes": 107374182400, "max_file_size_bytes": 104857600}'

echo ""
echo "测试 15: POST /_matrix/admin/v1/media/quota/users - 设置用户配额 (源码路径)"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST \
  "http://localhost:8008/_matrix/admin/v1/media/quota/users" \
  -H "Content-Type: application/json" \
  -d '{"user_id": "@admin:cjystx.top", "custom_max_storage_bytes": 1073741824}'

echo ""
echo "=========================================="
echo "媒体配额 API 测试完成"
echo "=========================================="
