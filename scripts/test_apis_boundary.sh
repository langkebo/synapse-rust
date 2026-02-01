#!/bin/bash

BASE_URL="http://localhost:8008"
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW5fdGVzdGVyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkBhZG1pbl90ZXN0ZXI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMDA2NDgzLCJpYXQiOjE3Njk5MjAwODMsImRldmljZV9pZCI6IkZXZ3Vicnlpdk9CVWRXanUifQ.PWoDyhJDQLWvFkOSuX3yH6MaDxOEtyiIF3NVNIsRUXU"

test_ep() {
    local method=$1
    local path=$2
    local desc=$3
    local token=$4
    local body=$5
    
    echo "Testing: $desc ($method $path)"
    
    local auth_header=""
    if [ -n "$token" ]; then
        auth_header="-H \"Authorization: Bearer $token\""
    fi
    
    local body_arg=""
    if [ -n "$body" ]; then
        body_arg="-H \"Content-Type: application/json\" -d '$body'"
    fi
    
    local cmd="curl -s -w \"\\n%{http_code}\\n%{time_total}\\n\" -X $method \"$BASE_URL$path\" $auth_header $body_arg"
    
    local out=$(eval "$cmd")
    local status_code=$(echo "$out" | tail -n 2 | head -n 1)
    local duration=$(echo "$out" | tail -n 1)
    local response=$(echo "$out" | head -n -2)
    
    echo "Status: $status_code, Time: ${duration}s"
    echo "Response: $response"
    echo "-----------------------------------"
}

# 1. Register a normal user
echo "Creating a normal user..."
USER_REG=$(curl -s -X POST http://localhost:8008/_matrix/client/r0/register -H "Content-Type: application/json" -d '{"username": "normal_user", "password": "password123", "admin": false}')
USER_TOKEN=$(echo $USER_REG | grep -o '"access_token":"[^"]*' | cut -d'"' -f4)
echo "Normal user token obtained."

echo "==================================="
echo "Boundary & Error Tests"
echo "==================================="

test_ep "GET" "/_matrix/client/r0/account/whoami" "Invalid Token Test" "invalid_token_here"
test_ep "GET" "/_synapse/admin/v1/users" "Non-Admin access to Admin API" "$USER_TOKEN"
test_ep "POST" "/_matrix/client/r0/register" "Duplicate User Registration" "" '{"username": "normal_user", "password": "password123"}'
test_ep "POST" "/_matrix/client/r0/createRoom" "Missing room name (Optional but check behavior)" "$ADMIN_TOKEN" '{}'
test_ep "POST" "/_matrix/client/r0/createRoom" "Invalid visibility value" "$ADMIN_TOKEN" '{"visibility": "super_secret"}'
test_ep "GET" "/_synapse/admin/v1/users/non_existent_user" "Non-existent user details" "$ADMIN_TOKEN"
