#!/bin/bash

# 从临时文件读取最新的账户信息
ACCOUNTS_FILE="/tmp/synapse_final_accounts.txt"

if [ ! -f "$ACCOUNTS_FILE" ]; then
    echo "错误: 找不到账户信息文件 $ACCOUNTS_FILE"
    echo "请先运行 manage_test_accounts.sh 脚本"
    exit 1
fi

echo "=========================================="
echo "更新测试账户信息到文档"
echo "=========================================="
echo ""

# 读取账户信息
declare -A accounts
while IFS='|' read -r username user_id access_token device_id refresh_token password; do
    accounts["$username,user_id"]="$user_id"
    accounts["$username,access_token"]="$access_token"
    accounts["$username,device_id"]="$device_id"
    accounts["$username,refresh_token"]="$refresh_token"
    accounts["$username,password"]="$password"
done < "$ACCOUNTS_FILE"

# 显示账户信息
echo "当前账户信息:"
echo ""
for username in "${!accounts[@]}"; do
    if [[ $username == *",user_id" ]]; then
        name="${username%,user_id}"
        echo "用户: $name"
        echo "  用户ID: ${accounts[$name,user_id]}"
        echo "  密码: ${accounts[$name,password]}"
        echo "  Access Token: ${accounts[$name,access_token]}"
        echo ""
    fi
done

echo "=========================================="
echo "账户信息已准备就绪"
echo "=========================================="
