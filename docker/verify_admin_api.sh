#!/bin/bash

BASE_URL="https://localhost"
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc2OTk5MTcyMSwiaWF0IjoxNzY5OTA1MzIxLCJkZXZpY2VfaWQiOiJWb0ZNcXNLMXROQVFMZTZBIn0.lqhB5LDgmEyAK61ltRR6gHHIndG7ZNIKiYqqu7ukb5U"

echo "=== Admin API Verification ==="

test_admin_endpoint() {
    local method=$1
    local path=$2
    local desc=$3
    echo -n "Testing $desc ($method $path)... "
    res=$(curl -sk -X $method -H "Authorization: Bearer $ADMIN_TOKEN" -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
    if [ "$res" == "200" ]; then
        echo " [SUCCESS]"
    else
        echo " [FAILED] Status: $res"
        cat /tmp/api_res
        echo ""
    fi
}

# 1. Admin Endpoints
test_admin_endpoint "GET" "/_synapse/admin/v1/status" "Server Status"
test_admin_endpoint "GET" "/_synapse/admin/v1/users" "Users List"
test_admin_endpoint "GET" "/_synapse/admin/v1/server_version" "Server Version"
test_admin_endpoint "GET" "/_synapse/admin/v1/rooms" "Rooms List"

# 2. Client Endpoints with Admin Token
test_admin_endpoint "GET" "/_matrix/client/r0/account/whoami" "WhoAmI (Admin)"
test_admin_endpoint "GET" "/_matrix/client/r0/sync" "Sync (Admin)"
test_admin_endpoint "GET" "/_matrix/client/r0/devices" "Devices (Admin)"

echo "=== Verification Completed ==="
