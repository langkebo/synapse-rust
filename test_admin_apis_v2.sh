#!/bin/bash
# 管理员API全面测试脚本
# 测试日期: 2026-02-04

BASE_URL="http://localhost:8008"
TOKEN=$(cat /home/hula/synapse_rust/admin_token.txt)

echo "========================================="
echo "管理员API全面测试"
echo "测试日期: $(date)"
echo "========================================="
echo ""

# 测试函数
test_endpoint() {
    local method=$1
    local endpoint=$2
    local description=$3
    local data=$4

    echo "--- 测试: $description ---"
    echo "端点: $method $endpoint"

    if [ "$method" == "GET" ]; then
        response=$(curl -s -X GET "${BASE_URL}${endpoint}" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json")
    elif [ "$method" == "POST" ]; then
        response=$(curl -s -X POST "${BASE_URL}${endpoint}" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d "$data")
    elif [ "$method" == "PUT" ]; then
        response=$(curl -s -X PUT "${BASE_URL}${endpoint}" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d "$data")
    elif [ "$method" == "DELETE" ]; then
        response=$(curl -s -X DELETE "${BASE_URL}${endpoint}" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json")
    fi

    status=$(echo "$response" | jq -r '.errcode // "OK"')
    http_code=$(echo "$response" | jq -r '."http_code" // "200"')

    if echo "$response" | jq -e '.success' > /dev/null 2>&1 || [ "$status" == "OK" ]; then
        echo "结果: ✅ 通过"
    elif [ "$http_code" == "200" ] || [ "$http_code" == "201" ]; then
        echo "结果: ✅ 通过 (HTTP $http_code)"
    else
        echo "结果: ❌ 失败"
        echo "错误码: $status"
    fi
    echo "响应: $(echo "$response" | jq '.' 2>/dev/null | head -20)"
    echo ""
}

# 1. 获取服务器版本
echo "========================================="
echo "1. 管理员API测试"
echo "========================================="
test_endpoint "GET" "/_synapse/admin/v1/server_version" "获取服务器版本"

# 2. 获取服务器统计
test_endpoint "GET" "/_synapse/admin/v1/server_stats" "获取服务器统计"

# 3. 获取用户列表
test_endpoint "GET" "/_synapse/admin/v1/users" "获取用户列表"

# 4. 获取特定用户信息
test_endpoint "GET" "/_synapse/admin/v1/users/@testuser1:cjystx.top" "获取特定用户信息"

# 5. 设置用户管理员状态
test_endpoint "PUT" "/_synapse/admin/v1/users/@testuser1:cjystx.top/admin" "设置用户管理员状态" '{"admin": false}'

# 6. 获取房间列表
test_endpoint "GET" "/_synapse/admin/v1/rooms" "获取房间列表"

# 7. 获取服务器配置
test_endpoint "GET" "/_synapse/admin/v1/config" "获取服务器配置"

# 8. 获取状态信息
test_endpoint "GET" "/_synapse/admin/v1/status" "获取状态信息"

# 9. 获取用户房间
test_endpoint "GET" "/_synapse/admin/v1/users/@testuser1:cjystx.top/rooms" "获取用户房间"

echo "========================================="
echo "测试完成"
echo "========================================="
