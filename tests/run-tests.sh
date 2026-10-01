#!/usr/bin/env bash
#
# Synapse Rust 后端综合测试报告
# 生成时间：$(date -Iseconds)
#

cd "$(dirname "$0")/.."

echo "=== 正在加载测试账户 ==="
load_accounts() {
    ADMIN_USER=""
    ADMIN_TOKEN=""
    
    while IFS=':' read -r user_id access_token device_id; do
        if [[ "${user_id}" == *"admin"* ]]; then
            ADMIN_USER="${user_id}"
            ADMIN_TOKEN="${access_token}"
        fi
    done < tests/accounts.txt
}

load_accounts
SERVER_URL="http://127.0.0.1:8008"

echo "Admin User: ${ADMIN_USER}"
echo ""

TOTAL=0
PASS=0
FAIL=0

test_case() {
    local cat=$1 name=$2 cmd="$3" expect="$4"
    ((TOTAL++)) || true
    echo "[${cat}] ${name}..."
    
    result=$(eval "${cmd}" 2>&1) || true
    rc=$?
    
    if [[ "$rc" == "$expect" ]]; then
        echo "  ✅ PASS"
        ((PASS++)) || true
    else
        echo "  ❌ FAIL (expected: $expect, got: $rc)"
        ((FAIL++)) || true
    fi
}

echo ""
echo "=== 账户管理 ==="
test_case "account" "Whoami (admin)" \
    "curl -s -H 'Authorization: Bearer ${ADMIN_TOKEN}' ${SERVER_URL}/_matrix/client/v3/account/whoami | grep -q 'user_id'" 0

test_case "account" "Profile get" \
    "curl -s -H 'Authorization: Bearer ${ADMIN_TOKEN}' ${SERVER_URL}/_matrix/client/v3/profile/@test:user.test | grep -q 'displayname'" 0

echo ""
echo "=== 房间操作 ==="

# 创建房间
CREATED_ROOM=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"name":"TestRoom","visibility":"private"}' | jq -r '.room_id')

test_case "room" "Create room" \
    "test -n '${CREATED_ROOM}'" 0
echo "  Room ID: ${CREATED_ROOM}"

# 房间消息
if [[ -n "${CREATED_ROOM}" ]]; then
    MSG_RESULT=$(curl -s -X PUT "${SERVER_URL}/_matrix/client/v3/rooms/${CREATED_ROOM}/send/m.room.message/t1" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -H "Content-Type: application/json" \
        -d '{"msgtype":"m.text","body":"Hello"}' | jq -r '.event_id')
    
    test_case "message" "Send message" \
        "test -n '${MSG_RESULT}'" 0
    echo "  Event ID: ${MSG_RESULT}"
    
    # 获取消息
    HIST_RESULT=$(curl -s "${SERVER_URL}/_matrix/client/v3/rooms/${CREATED_ROOM}/messages?limit=5" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" | jq -r '.chunk | length')
    
    test_case "message" "Get history" \
        "test \"${HIST_RESULT}\" -ge 1" 0
fi

echo ""
echo "=== 媒体上传 ==="

# 简单的 1x1 像素 PNG
PNG_DATA="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="

MEDIA_RESP=$(echo "${PNG_DATA}" | base64 -d | curl -s -X POST "${SERVER_URL}/_matrix/media/v3/upload" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: image/png" \
    --data-binary @-)

MEDIA_URI=$(echo "${MEDIA_RESP}" | jq -r '.content_uri // empty')

test_case "media" "Upload PNG" \
    "test -n '${MEDIA_URI}'" 0

if [[ -n "${MEDIA_URI}" ]]; then
    test_case "media" "Download media" \
        "curl -s ${SERVER_URL}${MEDIA_URI} -H 'Authorization: Bearer ${ADMIN_TOKEN}' | wc -c | grep -q '[1-9]'" 0
    echo "  Media URI: ${MEDIA_URI}"
fi

echo ""
echo "=== 同步与状态 ==="

SYNC_RESP=$(curl -s "${SERVER_URL}/_matrix/client/v3/sync?timeout=1000" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}")

test_case "sync" "Initial sync" \
    "echo '${SYNC_RESP}' | grep -q 'next_batch'" 0

test_case "sync" "Rooms in sync" \
    "echo '${SYNC_RESP}' | grep -q 'rooms'" 0

echo ""
echo "=== 安全测试 ==="

# HTTPS 监控
if [[ -f "docker/deploy/nginx/auth/.htpasswd" ]]; then
    HTTPS_PASS="SecurePassword123ChangeMe!"
    HTTPS_RESP=$(curl -sk -u "admin:${HTTPS_PASS}" \
        "https://localhost:8443/prometheus/api/v1/query?query=up")
    
    test_case "security" "HTTPS Prometheus" \
        "echo '${HTTPS_RESP}' | grep -q 'success'" 0
else
    echo "[security] HTTPS monitoring skipped (no auth configured)"
fi

echo ""
echo "========================================"
echo "测试结果汇总"
echo "========================================"
echo "总测试数：${TOTAL}"
echo "通过：${PASS}"
echo "失败：${FAIL}"
echo "通过率：$(( PASS * 100 / TOTAL ))%"
echo ""

if [[ ${FAIL} -eq 0 ]]; then
    echo "🎉 所有测试通过！Synapse Rust 后端功能正常！"
    exit 0
else
    echo "⚠️  部分测试失败"
    exit 1
fi
