#!/usr/bin/env bash
#
# 创建测试账户脚本
# 用法：./tests/create-test-accounts.sh
#

set -euo pipefail

SERVER_URL="http://127.0.0.1:8008"

# 测试账户配置
TEST_ACCOUNTS_ADMIN="AdminTest123!"
TEST_ACCOUNTS_BASIC="BasicUser123!"
TEST_ACCOUNTS_VOICE="VoiceUser123!"
TEST_ACCOUNTS_MEDIA="MediaUser123!"
TEST_ACCOUNTS_BOT="BotUser123!"

echo "=== 开始创建测试账户 ==="
echo "服务器地址：${SERVER_URL}"
echo ""

# 获取 registration token (如果启用了 token 认证)
TOKEN=""
if curl -s "${SERVER_URL}/_matrix/client/v3/register" | grep -q "registration_token"; then
    echo "检测到需要 registration token，请手动配置 TOKEN 变量"
    exit 1
fi

# 注册账户函数
register_account() {
    local username=$1
    local password=$2
    
    echo "正在注册用户：${username}..."
    
    # 步骤 1: 获取会话
    SESSION=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/register" \
        -H "Content-Type: application/json" \
        -d '{"type": "m.login.dummy", "initial_device_display_name": "Test Device"}' \
        | jq -r '.sid // empty')
    
    if [[ -z "${SESSION}" ]]; then
        # 直接注册（可能不需要流程）
        RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/register" \
            -H "Content-Type: application/json" \
            -d "{
                \"username\": \"${username}\",
                \"password\": \"${password}\",
                \"auth\": {\"type\": \"m.login.dummy\"},
                \"device_id\": \"TEST_DEVICE_${RANDOM}\"
            }")
        
        if echo "${RESPONSE}" | grep -q "access_token"; then
            ACCESS_TOKEN=$(echo "${RESPONSE}" | jq -r '.access_token')
            DEVICE_ID=$(echo "${RESPONSE}" | jq -r '.device_id')
            USER_ID=$(echo "${RESPONSE}" | jq -r '.user_id')
            
            echo "  ✅ 注册成功"
            echo "    User ID: ${USER_ID}"
            echo "    Device ID: ${DEVICE_ID}"
            echo ""
            
            # 保存到文件
            echo "${USER_ID}:${ACCESS_TOKEN}:${DEVICE_ID}" >> tests/accounts.txt
            
            return 0
        else
            ERROR=$(echo "${RESPONSE}" | jq -r '.error // "Unknown error"')
            echo "  ❌ 注册失败：${ERROR}"
            return 1
        fi
    fi
}

# 清理旧的账户文件
rm -f tests/accounts.txt
touch tests/accounts.txt

# 注册所有账户
declare -a ACCOUNTS=(
    "admin_test:${TEST_ACCOUNTS_ADMIN}"
    "user_basic:${TEST_ACCOUNTS_BASIC}"
    "user_voice:${TEST_ACCOUNTS_VOICE}"
    "user_media:${TEST_ACCOUNTS_MEDIA}"
    "bot_account:${TEST_ACCOUNTS_BOT}"
)

FAILED=0
SUCCESS=0

for account in "${ACCOUNTS[@]}"; do
    username="${account%%:*}"
    password="${account#*:}"
    
    if register_account "${username}" "${password}"; then
        ((SUCCESS++)) || true
    else
        ((FAILED++)) || true
    fi
done

echo ""
echo "=== 注册完成 ==="
echo "成功：${SUCCESS}"
echo "失败：${FAILED}"
echo ""

if [[ ${SUCCESS} -gt 0 ]]; then
    echo "账户信息已保存到 tests/accounts.txt"
    echo "格式：user_id:access_token:device_id"
    echo ""
    echo "查看账户列表:"
    cat tests/accounts.txt
fi

if [[ ${FAILED} -gt 0 ]]; then
    echo ""
    echo "⚠️  部分账户注册失败，可能需要先配置 registration token"
    echo "或者检查服务器是否允许开放注册"
fi
