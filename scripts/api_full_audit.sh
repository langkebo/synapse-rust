#!/bin/bash

# 配置
BASE_URL="http://localhost:8008"
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYXBpX3Rlc3Rlcl9hZG1pbjptYXRyaXguY2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAYXBpX3Rlc3Rlcl9hZG1pbjptYXRyaXguY2p5c3R4LnRvcCIsImFkbWluIjp0cnVlLCJleHAiOjE3NzAwMDkyMzQsImlhdCI6MTc2OTkyMjgzNCwiZGV2aWNlX2lkIjoiSmxYVEZhOGRza3dVZVpMT1ZjSG1wdz09In0.YL4Zd2DT5EcPZsYzJxyrICMnvQmXNF_L88wpaJnrUdM"

# 结果保存文件
OUTPUT_FILE="/app/api_test_results.log"
echo "API Test Execution Log - $(date)" > $OUTPUT_FILE

test_api() {
    local method=$1
    local path=$2
    local body=$3
    local desc=$4
    local auth=$5 # "admin" or "none"

    echo "Testing $desc ($method $path)..."
    
    local cmd="curl -s -w \"\n%{http_code}\n%{time_total}\n\" -X $method \"$BASE_URL$path\""
    if [ "$auth" == "admin" ]; then
        cmd="$cmd -H \"Authorization: Bearer $ADMIN_TOKEN\""
    fi
    if [ -n "$body" ]; then
        cmd="$cmd -H \"Content-Type: application/json\" -d '$body'"
    fi

    local response=$(eval "$cmd")
    local status=$(echo "$response" | tail -n 2 | head -n 1)
    local duration=$(echo "$response" | tail -n 1)
    local body_content=$(echo "$response" | head -n -2)

    echo "[$desc] $method $path -> Status: $status, Duration: ${duration}s" >> $OUTPUT_FILE
    echo "Response: $body_content" >> $OUTPUT_FILE
    echo "-----------------------------------" >> $OUTPUT_FILE
}

# 1. Client API (mod.rs)
test_api "GET" "/_matrix/client/versions" "" "Client Versions" "none"
test_api "GET" "/_matrix/client/r0/account/whoami" "" "Who Am I" "admin"
test_api "GET" "/_matrix/client/r0/publicRooms" "" "Get Public Rooms" "admin"
test_api "POST" "/_matrix/client/r0/createRoom" '{"name":"Test Room","visibility":"public"}' "Create Room" "admin"
test_api "GET" "/_matrix/client/r0/sync" "" "Sync" "admin"

# 2. Admin API (admin.rs)
test_api "GET" "/_synapse/admin/v1/server_version" "" "Admin Server Version" "admin"
test_api "GET" "/_synapse/admin/v1/users" "" "Admin List Users" "admin"
test_api "GET" "/_synapse/admin/v1/rooms" "" "Admin List Rooms" "admin"
test_api "GET" "/_synapse/admin/v1/status" "" "Admin Status" "admin"

# 3. Friend API (friend.rs)
test_api "GET" "/_synapse/enhanced/friends" "" "Get Friends" "admin"
test_api "GET" "/_synapse/enhanced/friend/requests" "" "Get Friend Requests" "admin"

# 4. Voice API (voice.rs)
test_api "GET" "/_matrix/client/r0/voice/user/@api_tester_admin:matrix.cjystx.top/stats" "" "Voice Stats" "admin"

# 5. Private Chat (private_chat.rs)
test_api "GET" "/_matrix/client/r0/dm" "" "Get DM Rooms" "admin"
test_api "GET" "/_synapse/enhanced/private/unread-count" "" "Private Unread Count" "admin"

# 6. Media (media.rs)
test_api "GET" "/_matrix/media/v1/config" "" "Media Config" "none"

# 7. Federation (federation.rs)
test_api "GET" "/_matrix/federation/v1/version" "" "Federation Version" "none"
test_api "GET" "/_matrix/federation/v1" "" "Federation Discovery" "none"

echo "Testing completed. Results saved to $OUTPUT_FILE"
