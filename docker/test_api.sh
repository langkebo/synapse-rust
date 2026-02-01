#!/bin/bash

BASE_URL="https://localhost"
CURL_CMD="curl -sk"

echo "=== Container Health Check ==="
docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
echo ""

echo "=== Identity Verification (@user:cjystx.top) ==="
# Test registration with a specific user format if supported, or just verify server name
SERVER_NAME=$(curl -sk https://localhost/ | jq -r .server_name)
echo "Server Name from API: $SERVER_NAME"
echo ""

echo "=== API Endpoint Testing ==="

test_endpoint() {
    local method=$1
    local path=$2
    local data=$3
    local desc=$4
    
    echo -n "Testing $desc ($method $path)... "
    if [ "$method" == "GET" ]; then
        res=$(curl -sk -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
    else
        res=$(curl -sk -X $method -H "Content-Type: application/json" -d "$data" -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
    fi
    
    if [ "$res" == "200" ]; then
        echo " [SUCCESS]"
    else
        echo " [FAILED] Status: $res"
        cat /tmp/api_res
        echo ""
    fi
}

# 1. Anonymous Endpoints
test_endpoint "GET" "/" "" "Server Info"
test_endpoint "GET" "/_matrix/client/versions" "" "Protocol Versions"
test_endpoint "GET" "/_matrix/client/r0/register/available?username=tester$(date +%s)" "" "Username Check"
test_endpoint "GET" "/_matrix/media/v1/config" "" "Media Config"

# 2. User Registration & Login
REG_DATA='{"username":"testuser_'$(date +%s)'","password":"password123","auth":{"type":"m.login.dummy"}}'
echo -n "Testing User Registration... "
reg_res=$(curl -sk -X POST -H "Content-Type: application/json" -d "$REG_DATA" -w "%{http_code}" -o /tmp/reg_json "$BASE_URL/_matrix/client/r0/register")
if [ "$reg_res" == "200" ]; then
    echo " [SUCCESS]"
    TOKEN=$(cat /tmp/reg_json | jq -r .access_token)
    USER_ID=$(cat /tmp/reg_json | jq -r .user_id)
    echo "User ID: $USER_ID"
else
    echo " [FAILED] Status: $reg_res"
    cat /tmp/reg_json
    echo ""
fi

# 3. Authenticated Endpoints (if token obtained)
if [ ! -z "$TOKEN" ] && [ "$TOKEN" != "null" ]; then
    echo "Testing Authenticated API with token..."
    test_auth_endpoint() {
        local method=$1
        local path=$2
        local desc=$3
        echo -n "Testing $desc ($method $path)... "
        res=$(curl -sk -X $method -H "Authorization: Bearer $TOKEN" -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
        if [ "$res" == "200" ]; then
            echo " [SUCCESS]"
        else
            echo " [FAILED] Status: $res"
            cat /tmp/api_res
            echo ""
        fi
    }
    
    test_auth_endpoint "GET" "/_matrix/client/r0/account/whoami" "WhoAmI"
    test_auth_endpoint "GET" "/_matrix/client/r0/sync" "Sync"
    test_auth_endpoint "GET" "/_matrix/client/r0/devices" "Devices List"
    test_auth_endpoint "GET" "/_synapse/admin/v1/status" "Admin Status"
else
    echo "Skipping authenticated tests due to login failure."
fi

echo "=== Test Completed ==="
