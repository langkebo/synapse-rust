#!/bin/bash

# Synapse Matrix Server - 测试账户注册脚本
# 服务器地址
SERVER="http://localhost:8008"

# 颜色输出
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=========================================="
echo "Synapse Matrix Server - 测试账户注册"
echo "=========================================="
echo ""

# 函数：注册用户
register_user() {
    local username=$1
    local password=$2
    local admin=$3
    
    echo -e "${YELLOW}正在注册用户: $username${NC}"
    
    if [ "$admin" = "true" ]; then
        response=$(curl -s -X POST "$SERVER/_matrix/client/r0/register" \
            -H "Content-Type: application/json" \
            -d "{
                \"username\": \"$username\",
                \"password\": \"$password\",
                \"auth\": {
                    \"type\": \"m.login.dummy\"
                },
                \"admin\": true
            }")
    else
        response=$(curl -s -X POST "$SERVER/_matrix/client/r0/register" \
            -H "Content-Type: application/json" \
            -d "{
                \"username\": \"$username\",
                \"password\": \"$password\",
                \"auth\": {
                    \"type\": \"m.login.dummy\"
                }
            }")
    fi
    
    # 检查是否成功
    if echo "$response" | jq -e '.access_token' > /dev/null 2>&1; then
        access_token=$(echo "$response" | jq -r '.access_token')
        user_id=$(echo "$response" | jq -r '.user_id')
        device_id=$(echo "$response" | jq -r '.device_id')
        refresh_token=$(echo "$response" | jq -r '.refresh_token')
        
        echo -e "${GREEN}✓ 注册成功${NC}"
        echo "  用户 ID: $user_id"
        echo "  Access Token: $access_token"
        echo "  Device ID: $device_id"
        echo "  Refresh Token: $refresh_token"
        echo ""
        
        # 保存到文件
        echo "$username|$user_id|$access_token|$device_id|$refresh_token" >> /tmp/synapse_accounts.txt
    else
        error=$(echo "$response" | jq -r '.error // .errcode')
        echo -e "${RED}✗ 注册失败: $error${NC}"
        echo ""
    fi
}

# 清空之前的记录
> /tmp/synapse_accounts.txt

# 注册测试账户
echo "开始注册测试账户..."
echo ""

# 1. 管理员账户
register_user "admin" "Admin@123" "true"

# 2. 测试用户账户
register_user "testuser1" "Test@123" "false"
register_user "testuser2" "Test@123" "false"
register_user "testuser3" "Test@123" "false"
register_user "testuser4" "Test@123" "false"
register_user "testuser5" "Test@123" "false"

echo "=========================================="
echo "注册完成！"
echo "=========================================="
echo ""
echo "账户信息已保存到: /tmp/synapse_accounts.txt"
echo ""
echo "账户列表:"
cat /tmp/synapse_accounts.txt | column -t -s "|"
