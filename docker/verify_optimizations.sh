#!/bin/bash

BASE_URL="https://localhost"
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc2OTk5MTcyMSwiaWF0IjoxNzY5OTA1MzIxLCJkZXZpY2VfaWQiOiJWb0ZNcXNLMXROQVFMZTZBIn0.lqhB5LDgmEyAK61ltRR6gHHIndG7ZNIKiYqqu7ukb5U"

echo "=== Optimized API Verification ==="

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

# 1. Verify /dm (formerly stub)
test_endpoint "GET" "/_matrix/client/r0/dm" "" "DM Rooms (Real Logic)"

# 2. Verify Admin Audit Logging
echo "Testing Admin Audit Logging..."
test_endpoint "POST" "/_synapse/admin/v1/security/ip/block" '{"ip_address":"1.2.3.4","reason":"audit_test"}' "Block IP"
echo "Checking security events for audit entry..."
curl -sk -H "Authorization: Bearer $ADMIN_TOKEN" "https://localhost/_synapse/admin/v1/security/events" | jq '.events[] | select(.event_type=="admin_action:block_ip")' | head -n 10
echo ""

# 3. Verify E2EE Key Changes
test_endpoint "GET" "/_matrix/client/v3/keys/changes?from=0" "" "Key Changes"

# 4. Verify Send to Device
test_endpoint "PUT" "/_matrix/client/v3/sendToDevice/m.test/txn_123" '{"messages":{"@admin:matrix.cjystx.top":{"DEVICE1":{"type":"m.test","content":{"foo":"bar"}}}}}' "Send To Device"

echo "=== Verification Completed ==="
