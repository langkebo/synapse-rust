#!/bin/bash

BASE_URL="http://localhost:8008"
# Get token from login
echo "Logging in to get token..."
LOGIN_RES=$(curl -s -X POST -H "Content-Type: application/json" -d '{"username":"testuser1","password":"testpass123","type":"m.login.password"}' "$BASE_URL/_matrix/client/r0/login")
TOKEN=$(echo $LOGIN_RES | grep -oP '(?<="access_token":")[^"]+')

if [ -z "$TOKEN" ]; then
    echo "Failed to get token"
    exit 1
fi

echo "Token obtained: ${TOKEN:0:20}..."

test_api() {
    local method=$1
    local endpoint=$2
    local data=$3
    local description=$4

    echo "Testing $method $endpoint ($description)..."
    
    local curl_cmd="curl -s -X $method \"$BASE_URL$endpoint\" -H \"Authorization: Bearer $TOKEN\""
    if [ -n "$data" ]; then
        curl_cmd="$curl_cmd -H \"Content-Type: application/json\" -d '$data'"
    fi

    local response=$(eval $curl_cmd)
    echo "Response: $response"
    echo "-----------------------------------"
}

# Failing E2EE APIs
test_api "POST" "/_matrix/client/r0/keys/query" '{"device_keys":{"@testuser1:matrix.cjystx.top":[]}}' "Query keys"
test_api "POST" "/_matrix/client/r0/keys/claim" '{"one_time_keys":{"@testuser1:matrix.cjystx.top":{"OQ8PPUOQMKMvZcHy":"signed_curve25519"}}}' "Claim keys"

# Failing Voice API
test_api "POST" "/_matrix/client/r0/voice/upload" '{"file":"YmFzZTY0ZGF0YQ==","filename":"test.ogg","duration_ms":1000}' "Voice upload"
