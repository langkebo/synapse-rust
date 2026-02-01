#!/bin/bash

BASE_URL="http://localhost:8008"
TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW5fdGVzdGVyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkBhZG1pbl90ZXN0ZXI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMDA2NDQ2LCJpYXQiOjE3Njk5MjAwNDYsImRldmljZV9pZCI6Ii9BNXVSUVJqU3dBV2ZwbUY0L2dRZGc9PSJ9.cafEjcKR1WXTbBCC2I8ZbSULAHY6gkFks6WN0IaUGEQ"

test_ep() {
    local method=$1
    local path=$2
    local desc=$3
    local auth=$4
    local body=$5
    
    echo "Testing: $desc ($method $path)"
    
    local auth_header=""
    if [ "$auth" == "true" ]; then
        auth_header="-H \"Authorization: Bearer $TOKEN\""
    fi
    
    local body_arg=""
    if [ -n "$body" ]; then
        body_arg="-H \"Content-Type: application/json\" -d '$body'"
    fi
    
    local cmd="curl -s -w \"\\n%{http_code}\\n%{time_total}\\n\" -X $method \"$BASE_URL$path\" $auth_header $body_arg"
    # echo "Executing: $cmd"
    
    local out=$(eval "$cmd")
    local status_code=$(echo "$out" | tail -n 2 | head -n 1)
    local duration=$(echo "$out" | tail -n 1)
    local response=$(echo "$out" | head -n -2)
    
    echo "Status: $status_code, Time: ${duration}s"
    echo "Response: $response"
    echo "-----------------------------------"
}

echo "Starting API Tests..."
echo "==================================="

test_ep "GET" "/_matrix/client/versions" "Get client versions" "false"
test_ep "GET" "/_matrix/client/r0/account/whoami" "Who am I" "true"
test_ep "GET" "/_matrix/client/r0/register/available?username=testuser" "Check username availability" "false"
test_ep "POST" "/_matrix/client/r0/login" "Login" "false" '{"type":"m.login.password","user":"admin_tester","password":"password123"}'
test_ep "GET" "/_synapse/admin/v1/server_version" "Get server version" "true"
test_ep "GET" "/_synapse/admin/v1/users" "List users" "true"
test_ep "GET" "/_synapse/admin/v1/rooms" "List rooms" "true"
test_ep "GET" "/_synapse/admin/v1/status" "Server status" "true"
test_ep "GET" "/_matrix/client/r0/account/profile/@admin_tester:matrix.cjystx.top" "Get profile" "true"
test_ep "GET" "/_matrix/client/r0/sync" "Initial sync" "true"
test_ep "POST" "/_matrix/client/r0/createRoom" "Create room" "true" '{"name":"Test Room","topic":"Test Topic","visibility":"public"}'
test_ep "GET" "/_matrix/client/r0/publicRooms" "List public rooms" "true"
test_ep "GET" "/_matrix/client/r0/devices" "List devices" "true"
test_ep "GET" "/_synapse/enhanced/friends" "List friends" "true"
test_ep "GET" "/_synapse/enhanced/friends/search?query=admin" "Search friends" "true"
test_ep "GET" "/_matrix/client/r0/voice/user/@admin_tester:matrix.cjystx.top/stats" "Voice stats" "true"
test_ep "GET" "/_synapse/enhanced/private/sessions" "List private sessions" "true"
test_ep "GET" "/_synapse/enhanced/private/unread-count" "Unread count" "true"
