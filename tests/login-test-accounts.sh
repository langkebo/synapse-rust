#!/usr/bin/env bash
#
# 通过登录获取测试账户 token（账户已存在时使用）
#

set -euo pipefail

SERVER_URL="http://127.0.0.1:8008"

# 账户 -> 密码
ACCOUNTS=(
    "admin_test:AdminTest123!"
    "user_basic:BasicUser123!"
    "user_voice:VoiceUser123!"
    "user_media:MediaUser123!"
    "bot_account:BotUser123!"
)

rm -f tests/accounts.txt
touch tests/accounts.txt

SUCCESS=0
FAILED=0

for account in "${ACCOUNTS[@]}"; do
    username="${account%%:*}"
    password="${account#*:}"

    echo "登录用户：${username}..."
    RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d "{
            \"type\": \"m.login.password\",
            \"identifier\": {\"type\": \"m.id.user\", \"user\": \"${username}\"},
            \"password\": \"${password}\",
            \"device_id\": \"TEST_$(date +%s)_${RANDOM}\"
        }")

    if echo "${RESPONSE}" | grep -q "access_token"; then
        USER_ID=$(echo "${RESPONSE}" | jq -r '.user_id')
        ACCESS_TOKEN=$(echo "${RESPONSE}" | jq -r '.access_token')
        DEVICE_ID=$(echo "${RESPONSE}" | jq -r '.device_id')
        echo "  ✅ 登录成功 ${USER_ID} (${DEVICE_ID})"
        echo "${USER_ID}:${ACCESS_TOKEN}:${DEVICE_ID}" >>tests/accounts.txt
        ((SUCCESS++)) || true
    else
        echo "  ❌ 登录失败：$(echo "${RESPONSE}" | jq -r '.error // "unknown"')"
        ((FAILED++)) || true
    fi
done

echo ""
echo "=== 登录完成：成功 ${SUCCESS}，失败 ${FAILED} ==="
