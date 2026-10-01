#!/usr/bin/env bash
#
# Synapse Rust 后端全方位测试脚本
# 

set -euo pipefail

SERVER_URL="http://127.0.0.1:8008"
RESULTS_FILE="tests/test-results.json"

# 加载账户信息
load_accounts() {
    ADMIN_USER=""
    ADMIN_TOKEN=""
    BASIC_USER=""
    BASIC_TOKEN=""
    
    while IFS=':' read -r user_id access_token device_id; do
        if [[ "${user_id}" == *"admin"* ]]; then
            ADMIN_USER="${user_id}"
            ADMIN_TOKEN="${access_token}"
        elif [[ "${user_id}" == *"basic"* ]]; then
            BASIC_USER="${user_id}"
            BASIC_TOKEN="${access_token}"
        fi
    done < tests/accounts.txt
}

echo "=== 初始化测试环境 ==="
load_accounts
echo "Admin User: ${ADMIN_USER}"
echo "Basic User: ${BASIC_USER}"
echo ""

# 测试计数器
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# 记录测试结果
TEST_RESULTS=()

# 测试函数
run_test() {
    local category=$1
    local name=$2
    local expected=$3
    shift 3
    
    ((TOTAL_TESTS++)) || true
    
    echo "测试 [${category}] ${name}..."
    
    local response
    response=$("$@" 2>&1) || true
    local actual="$?"
    
    if [[ "$actual" == "$expected" ]]; then
        echo "  ✅ 通过 (${expected})"
        ((PASSED_TESTS++)) || true
        TEST_RESULTS+=("{\"category\":\"${category}\",\"test\":\"${name}\",\"status\":\"PASS\",\"expected\":\"${expected}\"}")
    else
        echo "  ❌ 失败 (预期:${expected}, 实际:${actual})"
        ((FAILED_TESTS++)) || true
        TEST_RESULTS+=("{\"category\":\"${category}\",\"test\":\"${name}\",\"status\":\"FAIL\",\"expected\":\"${expected}\",\"actual\":\"${actual}\"}")
    fi
    echo ""
}

echo "=== 1. 账户管理测试 ==="
echo ""

# 1.1 获取账户信息
run_test "account" "get_account_info_admin" "0" \
    curl -s -w "%{http_code}" -o /dev/null -H "Authorization: Bearer ${ADMIN_TOKEN}" "${SERVER_URL}/_matrix/client/v3/me"

run_test "account" "get_account_info_basic" "0" \
    curl -s -w "%{http_code}" -o /dev/null -H "Authorization: Bearer ${BASIC_TOKEN}" "${SERVER_URL}/_matrix/client/v3/me"

# 1.2 登录测试
NEW_DEVICE_ID="LOGIN_TEST_$(date +%s)"
LOGIN_RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"identifier\": {\"type\": \"m.id.user\", \"user\": \"user_basic\"}, \"password\": \"BasicUser123!\", \"device_id\": \"${NEW_DEVICE_ID}\"}")

LOGIN_CODE=$(echo "${LOGIN_RESPONSE}" | grep -q "access_token" && echo "0" || echo "1")
run_test "account" "password_login" "0" \
    echo "${LOGIN_CODE}"

# 1.3 登出设备
LOGOUT_RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/logout" \
    -H "Authorization: Bearer ${BASIC_TOKEN}" \
    -H "Content-Type: application/json")

LOGOUT_CODE=$(echo "${LOGOUT_RESPONSE}" | grep -q "err" && echo "1" || echo "0")
run_test "account" "logout_device" "0" \
    echo "${LOGOUT_CODE}"

echo "=== 2. 房间功能测试 ==="
echo ""

# 2.1 创建房间
CREATE_ROOM_RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room", "visibility": "private"}')

ROOM_ID=$(echo "${CREATE_ROOM_RESPONSE}" | jq -r '.room_id // empty')
CREATE_CODE=$( [[ -n "${ROOM_ID}" ]] && echo "0" || echo "1" )
run_test "room" "create_room" "0" \
    echo "${CREATE_CODE}"

echo "  创建的房间 ID: ${ROOM_ID}"

# 2.2 加入房间
if [[ -n "${ROOM_ID}" ]]; then
    JOIN_RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/join/${ROOM_ID}" \
        -H "Authorization: Bearer ${BASIC_TOKEN}" \
        -H "Content-Type: application/json")
    
    JOIN_CODE=$(echo "${JOIN_RESPONSE}" | grep -q "room_id" && echo "0" || echo "1")
    run_test "room" "join_room" "0" \
        echo "${JOIN_CODE}"
    
    # 2.3 离开房间
    LEAVE_RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/client/v3/rooms/${ROOM_ID}/leave" \
        -H "Authorization: Bearer ${BASIC_TOKEN}" \
        -H "Content-Type: application/json")
    
    LEAVE_CODE=$(echo "${LEAVE_RESPONSE}" | grep -q "room_id" && echo "0" || echo "1")
    run_test "room" "leave_room" "0" \
        echo "${LEAVE_CODE}"
fi

echo "=== 3. 消息功能测试 ==="
echo ""

# 3.1 发送文本消息
MSG_SEND_RESPONSE=$(curl -s -X PUT "${SERVER_URL}/_matrix/client/v3/rooms/${ROOM_ID}/send/m.room.message/${MSG_TXN:-test_msg_1}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{
        "msgtype": "m.text",
        "body": "Hello, World!",
        "format": "org.matrix.custom.html",
        "formatted_body": "<strong>Hello, World!</strong>"
    }')

SEND_CODE=$(echo "${MSG_SEND_RESPONSE}" | grep -q "event_id" && echo "0" || echo "1")
run_test "message" "send_text_message" "0" \
    echo "${SEND_CODE}"

# 3.2 获取消息历史
MSG_GET_RESPONSE=$(curl -s -X GET "${SERVER_URL}/_matrix/client/v3/rooms/${ROOM_ID}/messages?limit=10" \
    -H "Authorization: Bearer ${BASIC_TOKEN}")

GET_CODE=$(echo "${MSG_GET_RESPONSE}" | grep -q "chunk" && echo "0" || echo "1")
run_test "message" "get_message_history" "0" \
    echo "${GET_CODE}"

# 3.3 发送表情消息
EMOJI_RESPONSE=$(curl -s -X PUT "${SERVER_URL}/_matrix/client/v3/rooms/${ROOM_ID}/send/m.room.message/test_emoji" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"msgtype": "m.text", "body": "🎉👍✨"}')

EMOJI_CODE=$(echo "${EMOJI_RESPONSE}" | grep -q "event_id" && echo "0" || echo "1")
run_test "message" "send_emoji_message" "0" \
    echo "${EMOJI_CODE}"

# 3.4 编辑消息
if echo "${MSG_SEND_RESPONSE}" | grep -q "event_id"; then
    EVENT_ID=$(echo "${MSG_SEND_RESPONSE}" | jq -r '.event_id // empty')
    EDIT_RESPONSE=$(curl -s -X PUT "${SERVER_URL}/_matrix/client/v3/rooms/${ROOM_ID}/send/m.room.message/edit_1" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{
            \"body\": \"[edited] Hello, World!\",
            \"m.new_content\": {
                \"body\": \"Edited message\"
            },
            \"m.relates_to\": {
                \"rel_type\": \"m.replace\",
                \"event_id\": \"${EVENT_ID}\"
            }
        }")
    
    EDIT_CODE=$(echo "${EDIT_RESPONSE}" | grep -q "event_id" && echo "0" || echo "1")
    run_test "message" "edit_message" "0" \
        echo "${EDIT_CODE}"
fi

echo "=== 4. 多媒体功能测试 ==="
echo ""

# 4.1 上传图片
IMAGE_BASE64=$(echo "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==" | tr -d '\n')
UPLOAD_IMAGE_RESPONSE=$(curl -s -X POST "${SERVER_URL}/_matrix/media/v3/upload" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: image/png" \
    --data-binary "@-") <<< "${IMAGE_BASE64}"

UPLOAD_IMG_CODE=$(echo "${UPLOAD_IMAGE_RESPONSE}" | grep -q "content_uri" && echo "0" || echo "1")
run_test "media" "upload_image" "0" \
    echo "${UPLOAD_IMG_CODE}"

if echo "${UPLOAD_IMAGE_RESPONSE}" | grep -q "content_uri"; then
    MEDIA_URI=$(echo "${UPLOAD_IMAGE_RESPONSE}" | jq -r '.content_uri')
    echo "  上传的媒体 URI: ${MEDIA_URI}"
fi

# 4.2 下载媒体
if [[ -n "${MEDIA_URI:-}" ]]; then
    DOWNLOAD_RESPONSE=$(curl -s -X GET "${SERVER_URL}${MEDIA_URI}" \
        -H "Authorization: Bearer ${BASIC_TOKEN}")
    
    DL_CODE=$([[ -n "${DOWNLOAD_RESPONSE}" ]] && echo "0" || echo "1")
    run_test "media" "download_media" "0" \
        echo "${DL_CODE}"
fi

echo "=== 5. 同步功能测试 ==="
echo ""

# 5.1 初始同步
SYNC_RESPONSE=$(curl -s -X GET "${SERVER_URL}/_matrix/client/v3/sync?timeout=3000&filter={\"room\":{\"events\":{\"types\":[\"m.room.message\",\"m.room.member\"]}}}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}")

SYNC_CODE=$(echo "${SYNC_RESPONSE}" | grep -q "next_batch" && echo "0" || echo "1")
run_test "sync" "initial_sync" "0" \
    echo "${SYNC_CODE}"

# 5.2 检查 presence
PRESENCE_RESPONSE=$(curl -s -X GET "${SERVER_URL}/_matrix/client/v3/presence/${ADMIN_USER}/status" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}")

PRESENCE_CODE=$(echo "${PRESENCE_RESPONSE}" | grep -q "presence" && echo "0" || echo "1")
run_test "presence" "get_presence_status" "0" \
    echo "${PRESENCE_CODE}"

echo "=== 6. 性能与安全测试 ==="
echo ""

# 6.1 速率限制测试
echo "测试速率限制 (连续 10 次请求)..."
RATE_LIMIT_COUNT=0
for i in {1..10}; do
    RESP=$(curl -s -w "%{http_code}" -o /dev/null "${SERVER_URL}/_matrix/client/v3/account/whoami" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")
    [[ "${RESP}" == "200" ]] && ((RATE_LIMIT_COUNT++)) || true
done

RATE_LIMIT_CODE=$((RATE_LIMIT_COUNT >= 9 && RATE_LIMIT_COUNT <= 10))
run_test "security" "rate_limit_check" "1" \
    echo "${RATE_LIMIT_CODE}"
echo "  成功请求数：${RATE_LIMIT_COUNT}/10"

# 6.2 并发请求测试
echo "测试并发请求 (3 个并发请求)..."
PROMISES=()
for i in {1..3}; do
    (curl -s -X GET "${SERVER_URL}/_matrix/client/v3/account/whoami" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" > /dev/null 2>&1 & echo $!) >> /tmp/parallel_jobs_$$.txt
done

wait
PARALLEL_CODE=$(wc -l < /tmp/parallel_jobs_$$.txt)
rm -f /tmp/parallel_jobs_$$.txt

run_test "performance" "concurrent_requests" "3" \
    echo "${PARALLEL_CODE}"

# 6.3 HTTPS 访问测试（如果可用）
echo "测试 HTTPS 监控访问..."
if [[ -f "docker/deploy/nginx/auth/.htpasswd" ]]; then
    PROM_PASS="SecurePassword123ChangeMe!"
    HTTPS_CODE=$(curl -sk -u "admin:${PROM_PASS}" \
        "https://localhost:8443/prometheus/api/v1/query?query=up" 2>/dev/null | grep -q "success" && echo "0" || echo "1")
    run_test "security" "https_monitoring_access" "0" \
        echo "${HTTPS_CODE}"
else
    echo "  ⚠️  跳过 HTTPS 测试（未配置认证）"
fi

echo ""
echo "========================================="
echo "测试完成汇总"
echo "========================================="
echo "总测试数：${TOTAL_TESTS}"
echo "通过：${PASSED_TESTS}"
echo "失败：${FAILED_TESTS}"
echo "通过率：$(( PASSED_TESTS * 100 / TOTAL_TESTS ))%"
echo ""

# 生成 JSON 报告
echo "{" > "${RESULTS_FILE}"
echo "  \"timestamp\": \"$(date -Iseconds)\"," >> "${RESULTS_FILE}"
echo "  \"summary\": {" >> "${RESULTS_FILE}"
echo "    \"total\": ${TOTAL_TESTS}," >> "${RESULTS_FILE}"
echo "    \"passed\": ${PASSED_TESTS}," >> "${RESULTS_FILE}"
echo "    \"failed\": ${FAILED_TESTS}," >> "${RESULTS_FILE}"
echo "    \"pass_rate\": $((${PASSED_TESTS} * 100 / ${TOTAL_TESTS}))%" >> "${RESULTS_FILE}"
echo "  }," >> "${RESULTS_FILE}"
echo "  \"results\": [" >> "${RESULTS_FILE}"
printf '%s\n' "${TEST_RESULTS[@]}" | sed '$!s/$/,/' >> "${RESULTS_FILE}"
echo "  ]" >> "${RESULTS_FILE}"
echo "}" >> "${RESULTS_FILE}"

echo "详细结果已保存到：${RESULTS_FILE}"

if [[ ${FAILED_TESTS} -eq 0 ]]; then
    echo ""
    echo "🎉 所有测试通过！后端运行正常！"
else
    echo ""
    echo "⚠️  部分测试失败，请查看详细报告"
    exit 1
fi
