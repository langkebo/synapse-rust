#!/bin/bash

# Synapse Matrix Server - 完整测试账户管理脚本
# 服务器地址
SERVER="http://localhost:8008"

# 颜色输出
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "=========================================="
echo "Synapse Matrix Server - 测试账户管理"
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
        echo "$username|$user_id|$access_token|$device_id|$refresh_token|$password" >> /tmp/synapse_final_accounts.txt
        return 0
    else
        error=$(echo "$response" | jq -r '.error // .errcode')
        if [ "$error" = "Username already taken" ]; then
            echo -e "${BLUE}用户已存在，尝试登录...${NC}"
            login_user "$username" "$password"
            return $?
        else
            echo -e "${RED}✗ 注册失败: $error${NC}"
            echo ""
            return 1
        fi
    fi
}

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
        echo "$username|$user_id|$access_token|$device_id|$refresh_token|$password" >> /tmp/synapse_final_accounts.txt
        return 0
    else
        error=$(echo "$response" | jq -r '.error // .errcode')
        echo -e "${RED}✗ 登录失败: $error${NC}"
        echo ""
        return 1
    fi
}

# 清空之前的记录
> /tmp/synapse_final_accounts.txt

# 注册/登录测试账户
echo "开始管理测试账户..."
echo ""

# 1. 管理员账户
register_user "admin" "Admin@123" "true"

# 2. 测试用户账户
register_user "testuser1" "Test@123" "false"
register_user "testuser2" "Test@123" "false"
register_user "testuser3" "Test@123" "false"
register_user "testuser4" "Test@123" "false"
register_user "testuser5" "Test@123" "false"

# 如果某些账户无法登录，创建替代账户
echo ""
echo "检查是否需要创建替代账户..."
echo ""

# 尝试登录所有账户，如果失败则创建新账户
for i in 1 2 3; do
    if ! grep -q "^testuser$i|" /tmp/synapse_final_accounts.txt; then
        echo -e "${YELLOW}testuser$i 无法登录，创建替代账户 testuser_new_$i${NC}"
        register_user "testuser_new_$i" "Test@123" "false"
    fi
done

echo "=========================================="
echo "账户管理完成！"
echo "=========================================="
echo ""
echo "最终账户列表:"
echo "用户名 | 用户ID | Access Token | Device ID | Refresh Token | 密码"
echo "------ | ------ | ------------ | --------- | ------------- | ----"
cat /tmp/synapse_final_accounts.txt
echo ""
echo "账户信息已保存到: /tmp/synapse_final_accounts.txt"
