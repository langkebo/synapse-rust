#!/bin/bash

BASE_URL="https://localhost"
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc2OTk5MTcyMSwiaWF0IjoxNzY5OTA1MzIxLCJkZXZpY2VfaWQiOiJWb0ZNcXNLMXROQVFMZTZBIn0.lqhB5LDgmEyAK61ltRR6gHHIndG7ZNIKiYqqu7ukb5U"

echo "=== Federation API Verification ==="

test_endpoint() {
    local method=$1
    local path=$2
    local data=$3
    local desc=$4
    
    echo -n "Testing $desc ($method $path)... "
    if [ "$method" == "GET" ]; then
        res=$(curl -sk -w "%{http_code}" -H "Authorization: Bearer $ADMIN_TOKEN" -o /tmp/api_res "$BASE_URL$path")
    else
        res=$(curl -sk -X $method -H "Content-Type: application/json" -H "Authorization: Bearer $ADMIN_TOKEN" -d "$data" -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
    fi
    
    if [ "$res" == "200" ]; then
        echo " [SUCCESS]"
    else
        echo " [FAILED] Status: $res"
        cat /tmp/api_res
        echo ""
    fi
}

# 1. Verify Federation Discovery
test_endpoint "GET" "/_matrix/federation/v1" "" "Federation Discovery"

# 2. Verify Send Transaction (PDU Persistence)
echo "Testing Send Transaction..."
test_endpoint "PUT" "/_matrix/federation/v1/send/txn_999" '{"origin":"remote.server","pdus":[{"event_id":"$remote_event_1","room_id":"!room1:localhost","sender":"@user:remote.server","type":"m.room.message","content":{"body":"Hello from federation"},"origin_server_ts":1700000000}]}' "Send Transaction"

# 3. Verify Make Join
test_endpoint "GET" "/_matrix/federation/v1/make_join/!room1:localhost/@user:remote.server" "" "Make Join"

# 4. Verify Send Join (Membership Persistence)
test_endpoint "PUT" "/_matrix/federation/v1/send_join/!room1:localhost/\$remote_join_1" '{"origin":"remote.server","event":{"sender":"@user:remote.server","origin_server_ts":1700000001,"content":{"membership":"join"}}}' "Send Join"

# 5. Verify Get Missing Events
test_endpoint "POST" "/_matrix/federation/v1/get_missing_events/!room1:localhost" '{"earliest_events":[],"latest_events":[],"limit":5}' "Get Missing Events"

echo "=== Federation Verification Completed ==="
