#!/bin/bash

BASE_URL="https://localhost"
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc2OTk5MTcyMSwiaWF0IjoxNzY5OTA1MzIxLCJkZXZpY2VfaWQiOiJWb0ZNcXNLMXROQVFMZTZBIn0.lqhB5LDgmEyAK61ltRR6gHHIndG7ZNIKiYqqu7ukb5U"
REPORT_FILE="/tmp/audit_report.txt"

echo "API Audit & Verification Report - $(date)" > $REPORT_FILE
echo "==========================================" >> $REPORT_FILE

test_api() {
    local module=$1
    local method=$2
    local path=$3
    local data=$4
    local desc=$5
    local auth=$6
    
    echo "Testing [$module] $desc ($method $path)..."
    
    local start_time=$(date +%s%N)
    if [ "$auth" == "true" ]; then
        if [ "$method" == "GET" ]; then
            res_code=$(curl -sk -w "%{http_code}" -H "Authorization: Bearer $ADMIN_TOKEN" -o /tmp/api_res "$BASE_URL$path")
        else
            res_code=$(curl -sk -X $method -H "Content-Type: application/json" -H "Authorization: Bearer $ADMIN_TOKEN" -d "$data" -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
        fi
    else
        if [ "$method" == "GET" ]; then
            res_code=$(curl -sk -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
        else
            res_code=$(curl -sk -X $method -H "Content-Type: application/json" -d "$data" -w "%{http_code}" -o /tmp/api_res "$BASE_URL$path")
        fi
    fi
    local end_time=$(date +%s%N)
    local duration=$(( (end_time - start_time) / 1000000 ))
    
    local status="✅ 正常"
    if [ "$res_code" -ge 400 ]; then
        status="❌ 异常 ($res_code)"
    fi
    
    echo "| $module | $desc | $method $path | $status | ${duration}ms |" >> $REPORT_FILE
}

echo "| 模块 | 功能描述 | 接口路径 | 状态 | 响应时间 |" >> $REPORT_FILE
echo "| :--- | :--- | :--- | :--- | :--- |" >> $REPORT_FILE

# Core Client
test_api "Client" "服务器信息" "GET" "/" "" "false"
test_api "Client" "协议版本" "GET" "/_matrix/client/versions" "" "false"
test_api "Client" "用户名检查" "GET" "/_matrix/client/r0/register/available?username=tester" "" "false"
test_api "Client" "WhoAmI" "GET" "/_matrix/client/r0/account/whoami" "" "true"
test_api "Client" "同步接口" "GET" "/_matrix/client/r0/sync?timeout=0" "" "true"
test_api "Client" "设备列表" "GET" "/_matrix/client/r0/devices" "" "true"
test_api "Client" "公共房间" "GET" "/_matrix/client/r0/publicRooms" "" "true"

# Admin
test_api "Admin" "系统状态" "GET" "/_synapse/admin/v1/status" "" "true"
test_api "Admin" "用户列表" "GET" "/_synapse/admin/v1/users" "" "true"
test_api "Admin" "房间列表" "GET" "/_synapse/admin/v1/rooms" "" "true"
test_api "Admin" "安全审计" "GET" "/_synapse/admin/v1/security/events" "" "true"

# Friend
test_api "Friend" "好友列表" "GET" "/_synapse/enhanced/friends/list" "" "true"
test_api "Friend" "好友推荐" "GET" "/_synapse/enhanced/friends/recommend" "" "true"

# Private
test_api "Private" "DM列表" "GET" "/_matrix/client/r0/dm" "" "true"
test_api "Private" "增强会话" "GET" "/_synapse/enhanced/private/sessions" "" "true"
test_api "Private" "未读计数" "GET" "/_synapse/enhanced/private/unread-count" "" "true"

# Media
test_api "Media" "配置信息" "GET" "/_matrix/media/v1/config" "" "true"
test_api "Media" "语音统计" "GET" "/_synapse/enhanced/media/voice/stats" "" "true"

# E2EE
test_api "E2EE" "密钥变更" "GET" "/_matrix/client/v3/keys/changes?from=0" "" "true"

# Federation
test_api "Federation" "联邦版本" "GET" "/_matrix/federation/v1/version" "" "false"
test_api "Federation" "服务发现" "GET" "/_matrix/federation/v1" "" "false"

echo "Audit completed. Report saved to $REPORT_FILE"
cat $REPORT_FILE
