#!/bin/bash

BASE_URL="https://localhost"
# Tokens from previous step
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc2OTk5MTcyMSwiaWF0IjoxNzY5OTA1MzIxLCJkZXZpY2VfaWQiOiJWb0ZNcXNLMXROQVFMZTZBIn0.lqhB5LDgmEyAK61ltRR6gHHIndG7ZNIKiYqqu7ukb5U"
USER1_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOmZhbHNlLCJleHAiOjE3Njk5OTE3MjEsImlhdCI6MTc2OTkwNTMyMSwiZGV2aWNlX2lkIjoiSGkxVWJYMzhMVDdiNnhMZS94WFpjZz09In0.TSlS_MsLeFK64Jaq1SVqswrKa5J0bmadcbITqIPCpv0"

# Output file for test results
RESULT_FILE="api_test_results.txt"
echo "API Comprehensive Test Report - $(date)" > $RESULT_FILE
echo "----------------------------------------" >> $RESULT_FILE

test_api() {
    local method=$1
    local path=$2
    local data=$3
    local token=$4
    local desc=$5
    
    echo "Testing $desc ($method $path)..."
    start_time=$(date +%s%N)
    
    if [ -n "$token" ]; then
        auth_header="Authorization: Bearer $token"
    else
        auth_header="X-No-Auth: true"
    fi
    
    if [ "$method" == "GET" ]; then
        response=$(curl -sk -w "\n%{http_code}\n%{time_total}" -X GET -H "$auth_header" "$BASE_URL$path")
    else
        response=$(curl -sk -w "\n%{http_code}\n%{time_total}" -X $method -H "$auth_header" -H "Content-Type: application/json" -d "$data" "$BASE_URL$path")
    fi
    
    http_code=$(echo "$response" | tail -n 2 | head -n 1)
    total_time=$(echo "$response" | tail -n 1)
    body=$(echo "$response" | sed '$d' | sed '$d')
    
    echo "| $desc | $method | $path | $http_code | ${total_time}s |" >> $RESULT_FILE
}

echo "| 功能描述 | 方法 | 路径 | 状态码 | 响应时间 |" >> $RESULT_FILE
echo "| :--- | :--- | :--- | :--- | :--- |" >> $RESULT_FILE

# 1. Client API (Anonymous)
test_api "GET" "/" "" "" "Server Info"
test_api "GET" "/_matrix/client/versions" "" "" "Versions"

# 2. Client API (Auth)
test_api "GET" "/_matrix/client/r0/account/whoami" "" "$ADMIN_TOKEN" "WhoAmI (Admin)"
test_api "GET" "/_matrix/client/r0/sync" "" "$ADMIN_TOKEN" "Sync (Admin)"

# 3. Admin API
test_api "GET" "/_synapse/admin/v1/status" "" "$ADMIN_TOKEN" "Admin Status"
test_api "GET" "/_synapse/admin/v1/users" "" "$ADMIN_TOKEN" "Admin Users List"
test_api "GET" "/_synapse/admin/v1/rooms" "" "$ADMIN_TOKEN" "Admin Rooms List"
test_api "GET" "/_synapse/admin/v1/security/events" "" "$ADMIN_TOKEN" "Security Events"
test_api "GET" "/_synapse/admin/v1/security/ip/blocks" "" "$ADMIN_TOKEN" "IP Blocks"

# 4. Friend API (Auth)
test_api "GET" "/_synapse/enhanced/friends" "" "$USER1_TOKEN" "Friend List (User1)"
test_api "GET" "/_synapse/enhanced/friends/search?query=admin" "" "$USER1_TOKEN" "Search Friends"

# 5. Private Chat API (Auth)
test_api "GET" "/_synapse/enhanced/private/sessions" "" "$USER1_TOKEN" "Private Sessions"
test_api "GET" "/_synapse/enhanced/private/unread-count" "" "$USER1_TOKEN" "Unread Count"

# 6. Boundary/Error Tests
test_api "GET" "/_synapse/admin/v1/status" "" "$USER1_TOKEN" "Admin Status (Unauthorized)"
test_api "POST" "/_matrix/client/r0/login" '{"user":"nonexistent","password":"wrong"}' "" "Login (Failed)"

echo "Tests completed. Results in $RESULT_FILE"
