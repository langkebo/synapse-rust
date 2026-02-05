#!/bin/bash
set -e

BASE_URL="http://localhost:8008"
SHARED_SECRET="test_shared_secret"

calculate_hmac() {
    local nonce=$1
    local username=$2
    local password=$3
    local admin=$4
    
    local key=$(echo -n "$SHARED_SECRET" | xxd -p | tr -d '\n')
    local message="${nonce}\x00${username}\x00${password}\x00${admin}"
    local message_hex=$(echo -n "$message" | xxd -p | tr -d '\n')
    
    if command -v openssl &> /dev/null; then
        echo -n "$message" | openssl dgst -sha256 -hmac "$SHARED_SECRET" | awk '{print $2}'
    else
        echo "0000000000000000000000000000000000000000000000000000000000000000"
    fi
}

register_user() {
    local username=$1
    local password=$2
    local is_admin=$3
    
    echo "=== 注册用户: $username (admin: $is_admin) ==="
    
    # 获取nonce
    nonce=$(curl -s "${BASE_URL}/_synapse/admin/v1/register/nonce" | jq -r '.nonce')
    
    if [ "$nonce" == "null" ] || [ -z "$nonce" ]; then
        echo "✗ 获取nonce失败"
        return 1
    fi
    
    # 计算HMAC
    mac=$(calculate_hmac "$nonce" "$username" "$password" "$is_admin")
    
    # 注册
    response=$(curl -s -X POST "${BASE_URL}/_synapse/admin/v1/register" \
        -H "Content-Type: application/json" \
        -d "{
            \"nonce\": \"$nonce\",
            \"username\": \"$username\",
            \"password\": \"$password\",
            \"admin\": $is_admin,
            \"mac\": \"$mac\"
        }")
    
    if echo "$response" | jq -e '.access_token' > /dev/null 2>&1; then
        echo "✓ $username 注册成功"
        echo "$response" | jq -r '.access_token' > "${username}_token.txt"
        echo "$response" | jq -r '.user_id' > "${username}_userid.txt"
        return 0
    elif echo "$response" | jq -e '.errcode' > /dev/null 2>&1; then
        errcode=$(echo "$response" | jq -r '.errcode')
        if [ "$errcode" == "M_USER_IN_USE" ]; then
            echo "⚠ $username 已存在"
            return 0
        else
            echo "✗ 注册失败: $errcode"
            echo "$response"
            return 1
        fi
    else
        echo "✗ 注册失败"
        echo "$response"
        return 1
    fi
}

echo "========================================="
echo "   Matrix 测试环境准备脚本"
echo "========================================="
echo ""

# 注册管理员
register_user "admin" "AdminPass123!" true

# 注册测试用户
for i in {1..6}; do
    register_user "testuser$i" "TestPass123!" false
done

echo ""
echo "========================================="
echo "   用户注册完成！"
echo "========================================="
echo ""
cat *_token.txt 2>/dev/null || echo "无token文件"
echo ""

echo "测试账号信息:"
echo "-------------"
for i in {1..6}; do
    if [ -f "testuser${i}_userid.txt" ]; then
        echo "testuser$i: $(cat testuser${i}_userid.txt)"
    fi
done