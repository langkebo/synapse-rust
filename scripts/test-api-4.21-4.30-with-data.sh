#!/bin/bash

BASE_URL="http://localhost:8008"

passed=0
failed=0
total=0

echo "=== 获取 Token ==="
ADMIN_TOKEN=$(curl -s -X POST ${BASE_URL}/_matrix/client/r0/login -H "Content-Type: application/json" -d '{"type": "m.login.password", "user": "admin", "password": "Admin@123"}' | jq -r '.access_token')
TEST_TOKEN=$(curl -s -X POST ${BASE_URL}/_matrix/client/r0/login -H "Content-Type: application/json" -d '{"type": "m.login.password", "user": "testuser1", "password": "Test@123"}' | jq -r '.access_token')

echo "ADMIN_TOKEN: $ADMIN_TOKEN"
echo "TEST_TOKEN: $TEST_TOKEN"
echo ""

echo "=== 创建测试 Space ==="
SPACE_RESULT=$(curl -s -X POST ${BASE_URL}/_matrix/client/v3/createRoom \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Space",
    "topic": "Test Space for API Testing",
    "preset": "public_chat",
    "creation_content": {"type": "m.space"}
  }')
SPACE_ID=$(echo "$SPACE_RESULT" | jq -r '.room_id')
echo "SPACE_ID: $SPACE_ID"

echo ""
echo "=== 创建测试 Room ==="
ROOM_RESULT=$(curl -s -X POST ${BASE_URL}/_matrix/client/v3/createRoom \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Room",
    "topic": "Test Room for API Testing",
    "preset": "public_chat"
  }')
ROOM_ID=$(echo "$ROOM_RESULT" | jq -r '.room_id')
echo "ROOM_ID: $ROOM_ID"

echo ""
echo "=== 同步测试数据到数据库 ==="
docker exec synapse-postgres psql -U synapse -d synapse_test -c "
-- 插入 Space 记录
INSERT INTO spaces (space_id, room_id, name, topic, creator, join_rule, visibility, is_public, room_type, creation_ts, updated_ts)
SELECT 
    '$SPACE_ID', 
    '$SPACE_ID', 
    'Test Space', 
    'Test Space for API Testing', 
    '@admin:cjystx.top',
    'public',
    'public',
    TRUE,
    'm.space',
    EXTRACT(EPOCH FROM NOW())::BIGINT,
    EXTRACT(EPOCH FROM NOW())::BIGINT
WHERE NOT EXISTS (SELECT 1 FROM spaces WHERE space_id = '$SPACE_ID');

-- 插入 RoomSummary 记录
INSERT INTO room_summaries (room_id, name, topic, is_public, member_count, join_rules, last_message_ts)
SELECT 
    '$ROOM_ID',
    'Test Room',
    'Test Room for API Testing',
    TRUE,
    1,
    'public',
    EXTRACT(EPOCH FROM NOW())::BIGINT
WHERE NOT EXISTS (SELECT 1 FROM room_summaries WHERE room_id = '$ROOM_ID');

-- 插入 Retention Policy 记录
INSERT INTO room_retention_policies (room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts)
SELECT 
    '$ROOM_ID',
    31536000000,
    0,
    FALSE,
    FALSE,
    EXTRACT(EPOCH FROM NOW())::BIGINT,
    EXTRACT(EPOCH FROM NOW())::BIGINT
WHERE NOT EXISTS (SELECT 1 FROM room_retention_policies WHERE room_id = '$ROOM_ID');

-- 插入 Retention Stats 记录
INSERT INTO retention_stats (room_id, total_events, events_in_retention, events_expired, last_cleanup_ts, next_cleanup_ts)
SELECT 
    '$ROOM_ID',
    0,
    0,
    0,
    EXTRACT(EPOCH FROM NOW())::BIGINT,
    EXTRACT(EPOCH FROM NOW())::BIGINT + 86400
WHERE NOT EXISTS (SELECT 1 FROM retention_stats WHERE room_id = '$ROOM_ID');
"

sleep 2

test_endpoint() {
    local name="$1"
    local method="$2"
    local endpoint="$3"
    local token="$4"
    local data="$5"
    
    total=$((total + 1))
    
    if [ "$method" = "GET" ]; then
        code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $token" "${BASE_URL}${endpoint}")
    elif [ "$method" = "POST" ]; then
        code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H "Authorization: Bearer $token" -H "Content-Type: application/json" -d "$data" "${BASE_URL}${endpoint}")
    elif [ "$method" = "PUT" ]; then
        code=$(curl -s -o /dev/null -w "%{http_code}" -X PUT -H "Authorization: Bearer $token" -H "Content-Type: application/json" -d "$data" "${BASE_URL}${endpoint}")
    elif [ "$method" = "DELETE" ]; then
        code=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE -H "Authorization: Bearer $token" "${BASE_URL}${endpoint}")
    fi
    
    if [ "$code" = "200" ] || [ "$code" = "201" ] || [ "$code" = "202" ]; then
        echo "[PASS] $name: $code"
        passed=$((passed + 1))
    else
        echo "[FAIL] $name: $code (expected 200)"
        failed=$((failed + 1))
    fi
}

echo ""
echo "========================================"
echo "  API Test Execution - 4.21 to 4.30"
echo "========================================"
echo ""

echo "=== 4.21 Space 功能 API ==="
test_endpoint "GET rooms hierarchy" "GET" "/_matrix/client/v1/rooms/${SPACE_ID}/hierarchy" "$TEST_TOKEN"
test_endpoint "GET space info" "GET" "/_matrix/client/v1/spaces/${SPACE_ID}" "$TEST_TOKEN"
test_endpoint "GET space rooms" "GET" "/_matrix/client/v1/spaces/${SPACE_ID}/rooms" "$TEST_TOKEN"
test_endpoint "GET space summary" "GET" "/_matrix/client/v1/spaces/${SPACE_ID}/summary" "$TEST_TOKEN"
test_endpoint "GET space children" "GET" "/_matrix/client/v1/spaces/${SPACE_ID}/children" "$TEST_TOKEN"
test_endpoint "GET space state" "GET" "/_matrix/client/v1/spaces/${SPACE_ID}/state" "$TEST_TOKEN"
test_endpoint "GET all spaces (admin)" "GET" "/_synapse/admin/v1/spaces" "$ADMIN_TOKEN"
test_endpoint "GET space detail (admin)" "GET" "/_synapse/admin/v1/spaces/${SPACE_ID}" "$ADMIN_TOKEN"
test_endpoint "GET space users (admin)" "GET" "/_synapse/admin/v1/spaces/${SPACE_ID}/users" "$ADMIN_TOKEN"
test_endpoint "GET space rooms (admin)" "GET" "/_synapse/admin/v1/spaces/${SPACE_ID}/rooms" "$ADMIN_TOKEN"
test_endpoint "GET space stats (admin)" "GET" "/_synapse/admin/v1/spaces/${SPACE_ID}/stats" "$ADMIN_TOKEN"

echo ""
echo "=== 4.22 应用服务 API ==="
test_endpoint "GET all app services" "GET" "/_synapse/admin/v1/application_services" "$ADMIN_TOKEN"
test_endpoint "GET app service config" "GET" "/_synapse/admin/v1/application_services/config" "$ADMIN_TOKEN"
test_endpoint "GET all protocols" "GET" "/_synapse/admin/v1/application_services/protocols" "$ADMIN_TOKEN"

echo ""
echo "=== 4.23 Worker 架构 API ==="
test_endpoint "GET all workers" "GET" "/_synapse/admin/v1/workers" "$ADMIN_TOKEN"
test_endpoint "GET worker stats" "GET" "/_synapse/admin/v1/workers/stats" "$ADMIN_TOKEN"
test_endpoint "GET worker health" "GET" "/_synapse/admin/v1/workers/health" "$ADMIN_TOKEN"
test_endpoint "GET worker config" "GET" "/_synapse/admin/v1/workers/config" "$ADMIN_TOKEN"
test_endpoint "GET worker instances" "GET" "/_synapse/admin/v1/workers/instances" "$ADMIN_TOKEN"
test_endpoint "GET worker tasks" "GET" "/_synapse/admin/v1/workers/tasks" "$ADMIN_TOKEN"

echo ""
echo "=== 4.24 房间摘要 API ==="
test_endpoint "GET room summary" "GET" "/_matrix/client/v3/rooms/${ROOM_ID}/summary" "$TEST_TOKEN"
test_endpoint "GET user summaries" "GET" "/_synapse/room_summary/v1/summaries" "$TEST_TOKEN"

echo ""
echo "=== 4.25 消息保留策略 API ==="
test_endpoint "GET server retention policy" "GET" "/_synapse/retention/v1/server/policy" "$ADMIN_TOKEN"
test_endpoint "GET rooms with policies" "GET" "/_synapse/retention/v1/rooms" "$ADMIN_TOKEN"
test_endpoint "GET room retention policy" "GET" "/_synapse/retention/v1/rooms/${ROOM_ID}/policy" "$ADMIN_TOKEN"
test_endpoint "GET effective policy" "GET" "/_synapse/retention/v1/rooms/${ROOM_ID}/effective_policy" "$ADMIN_TOKEN"
test_endpoint "GET retention stats" "GET" "/_synapse/retention/v1/rooms/${ROOM_ID}/stats" "$ADMIN_TOKEN"
test_endpoint "GET cleanup logs" "GET" "/_synapse/retention/v1/rooms/${ROOM_ID}/logs" "$ADMIN_TOKEN"
test_endpoint "GET deleted events" "GET" "/_synapse/retention/v1/rooms/${ROOM_ID}/deleted" "$ADMIN_TOKEN"
test_endpoint "GET pending cleanup" "GET" "/_synapse/retention/v1/rooms/${ROOM_ID}/pending" "$ADMIN_TOKEN"

echo ""
echo "=== 4.26 刷新令牌 API ==="
test_endpoint "GET user tokens" "GET" "/_synapse/admin/v1/users/@testuser1:cjystx.top/tokens" "$ADMIN_TOKEN"
test_endpoint "GET active tokens" "GET" "/_synapse/admin/v1/users/@testuser1:cjystx.top/tokens/active" "$ADMIN_TOKEN"
test_endpoint "GET token stats" "GET" "/_synapse/admin/v1/users/@testuser1:cjystx.top/tokens/stats" "$ADMIN_TOKEN"
test_endpoint "GET token usage" "GET" "/_synapse/admin/v1/users/@testuser1:cjystx.top/tokens/usage" "$ADMIN_TOKEN"

echo ""
echo "=== 4.27 注册令牌 API ==="
test_endpoint "GET registration tokens" "GET" "/_synapse/admin/v1/registration_tokens" "$ADMIN_TOKEN"
test_endpoint "GET active registration tokens" "GET" "/_synapse/admin/v1/registration_tokens/active" "$ADMIN_TOKEN"

echo ""
echo "=== 4.28 事件举报 API ==="
test_endpoint "GET all event reports" "GET" "/_synapse/admin/v1/event_reports" "$ADMIN_TOKEN"
test_endpoint "GET report count" "GET" "/_synapse/admin/v1/event_reports/count" "$ADMIN_TOKEN"
test_endpoint "GET report stats" "GET" "/_synapse/admin/v1/event_reports/stats" "$ADMIN_TOKEN"

echo ""
echo "=== 4.29 后台更新 API ==="
test_endpoint "GET all background updates" "GET" "/_synapse/admin/v1/background_updates" "$ADMIN_TOKEN"
test_endpoint "GET update count" "GET" "/_synapse/admin/v1/background_updates/count" "$ADMIN_TOKEN"
test_endpoint "GET pending updates" "GET" "/_synapse/admin/v1/background_updates/pending" "$ADMIN_TOKEN"
test_endpoint "GET running updates" "GET" "/_synapse/admin/v1/background_updates/running" "$ADMIN_TOKEN"
test_endpoint "GET next update" "GET" "/_synapse/admin/v1/background_updates/next" "$ADMIN_TOKEN"
test_endpoint "GET update stats" "GET" "/_synapse/admin/v1/background_updates/stats" "$ADMIN_TOKEN"

echo ""
echo "=== 4.30 可插拔模块 API ==="
test_endpoint "GET all modules" "GET" "/_synapse/admin/v1/modules" "$ADMIN_TOKEN"
test_endpoint "GET password auth providers" "GET" "/_synapse/admin/v1/password_auth_providers" "$ADMIN_TOKEN"
test_endpoint "GET presence routes" "GET" "/_synapse/admin/v1/presence_routes" "$ADMIN_TOKEN"
test_endpoint "GET media callbacks" "GET" "/_synapse/admin/v1/media_callbacks" "$ADMIN_TOKEN"
test_endpoint "GET rate limit callbacks" "GET" "/_synapse/admin/v1/rate_limit_callbacks" "$ADMIN_TOKEN"
test_endpoint "GET account data callbacks" "GET" "/_synapse/admin/v1/account_data_callbacks" "$ADMIN_TOKEN"

echo ""
echo "========================================"
echo "  Test Summary"
echo "========================================"
echo "Total: $total"
echo "Passed: $passed"
echo "Failed: $failed"
echo "Pass Rate: $(echo "scale=1; $passed * 100 / $total" | bc)%"
echo "========================================"
