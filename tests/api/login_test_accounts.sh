#!/bin/bash

# Synapse Matrix Server - 测试账户登录脚本
# 服务器地址
SERVER="http://localhost:8008"

# 颜色输出
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=========================================="
echo "Synapse Matrix Server - 测试账户登录"
echo "=========================================="
echo ""

# 函数：登录用户
login_user() {
    local username=$1
    local password=$2
    
    echo -e "${YELLOW}正在登录用户: $username${NC}"
    
    response=$(curl -s -X POST "$SERVER/_matrix/client/r0/login" \
        -H "Content-Type: application/json" \
        -d "{
            \"type\": \"m.login.password\",
            \"user\": \"$username\",
            \"password\": \"$password\",
            \"device_id\": \"TEST_DEVICE_$username\"
        }")
    
    # 检查是否成功
    if echo "$response" | jq -e '.access_token' > /dev/null 2>&1; then
        access_token=$(echo "$response" | jq -r '.access_token')
        user_id=$(echo "$response" | jq -r '.user_id')
        device_id=$(echo "$response" | jq -r '.device_id')
        refresh_token=$(echo "$response" | jq -r '.refresh_token // "N/A"')
        
        echo -e "${GREEN}✓ 登录成功${NC}"
        echo "  用户 ID: $user_id"
        echo "  Access Token: $access_token"
        echo "  Device ID: $device_id"
        echo "  Refresh Token: $refresh_token"
        echo ""
        
        # 保存到文件
        echo "$username|$user_id|$access_token|$device_id|$refresh_token" >> /tmp/synapse_tokens.txt
    else
        error=$(echo "$response" | jq -r '.error // .errcode')
        echo -e "${RED}✗ 登录失败: $error${NC}"
        echo ""
    fi
}

# 清空之前的记录
> /tmp/synapse_tokens.txt

# 登录测试账户
echo "开始登录测试账户..."
echo ""

# 1. 管理员账户
login_user "admin" "Admin@123"

# 2. 测试用户账户
login_user "testuser1" "Test@123"
login_user "testuser2" "Test@123"
login_user "testuser3" "Test@123"
login_user "testuser4" "Test@123"
login_user "testuser5" "Test@123"

echo "=========================================="
echo "登录完成！"
echo "=========================================="
echo ""
echo "Token 信息已保存到: /tmp/synapse_tokens.txt"
echo ""
echo "账户列表:"
echo "用户名 | 用户ID | Access Token | Device ID | Refresh Token"
echo "------ | ------ | ------------ | --------- | -------------"
cat /tmp/synapse_tokens.txt
