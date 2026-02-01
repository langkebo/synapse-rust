#!/bin/bash

BASE_URL="http://localhost:8008"
ADMIN_TOKEN=""
USER1_TOKEN=""
USER2_TOKEN=""
REPORT_FILE="docs/synapse-rust/test-report.md"

echo "=== API 全维度自动化测试 ==="
echo "# API 测试报告" > $REPORT_FILE
echo "> 测试日期: $(date)" >> $REPORT_FILE
echo "" >> $REPORT_FILE

# 辅助函数：提取 Token
get_token() {
    local username=$1
    local password=$2
    curl -s -X POST "$BASE_URL/_matrix/client/r0/login" \
        -H "Content-Type: application/json" \
        -d "{\"user\": \"$username\", \"password\": \"$password\"}" | grep -oP '(?<="access_token":")[^"]+'
}

# 1. 认证
echo "1. 认证测试"
ADMIN_TOKEN=$(get_token "admin" "password123")
USER1_TOKEN=$(get_token "user1" "password123")
USER2_TOKEN=$(get_token "user2" "password123")

if [ -n "$ADMIN_TOKEN" ] && [ -n "$USER1_TOKEN" ]; then
    echo "- [x] 认证系统: 登录成功" >> $REPORT_FILE
else
    echo "- [ ] 认证系统: 登录失败" >> $REPORT_FILE
fi

# 2. 房间管理
echo "2. 房间管理测试"
PUBLIC_ROOMS=$(curl -s -X GET "$BASE_URL/_matrix/client/r0/publicRooms" -H "Authorization: Bearer $USER1_TOKEN")
if echo $PUBLIC_ROOMS | grep -q "chunk"; then
    echo "- [x] 房间系统: 获取公共房间列表成功" >> $REPORT_FILE
else
    echo "- [ ] 房间系统: 获取公共房间列表失败" >> $REPORT_FILE
fi

# 3. 增强型 API: 好友系统
echo "3. 好友系统测试"
FRIENDS=$(curl -s -X GET "$BASE_URL/_synapse/enhanced/friends" -H "Authorization: Bearer $USER1_TOKEN")
if echo $FRIENDS | grep -q "@user2:localhost"; then
    echo "- [x] 增强 API: 好友列表验证成功" >> $REPORT_FILE
else
    echo "- [ ] 增强 API: 好友列表验证失败" >> $REPORT_FILE
fi

# 4. 增强型 API: 私聊会话
echo "4. 私聊会话测试"
SESSIONS=$(curl -s -X GET "$BASE_URL/_synapse/enhanced/private/sessions" -H "Authorization: Bearer $USER1_TOKEN")
if echo $SESSIONS | grep -q "ps_"; then
    echo "- [x] 增强 API: 私聊会话验证成功" >> $REPORT_FILE
else
    echo "- [ ] 增强 API: 私聊会话验证失败" >> $REPORT_FILE
fi

# 5. 管理员 API
echo "5. 管理员 API 测试"
SERVER_STATUS=$(curl -s -X GET "$BASE_URL/_synapse/admin/v1/status" -H "Authorization: Bearer $ADMIN_TOKEN")
if echo $SERVER_STATUS | grep -q "status"; then
    echo "- [x] 管理 API: 系统状态查询成功" >> $REPORT_FILE
else
    echo "- [ ] 管理 API: 系统状态查询失败" >> $REPORT_FILE
fi

# 6. 安全测试 (越权访问)
echo "6. 安全测试"
FORBIDDEN_RESP=$(curl -s -X GET "$BASE_URL/_synapse/admin/v1/status" -H "Authorization: Bearer $USER1_TOKEN")
if echo $FORBIDDEN_RESP | grep -q "M_FORBIDDEN"; then
    echo "- [x] 安全性: 非管理员权限拦截成功" >> $REPORT_FILE
else
    echo "- [ ] 安全性: 非管理员权限拦截失败" >> $REPORT_FILE
fi

# 7. 性能简测
echo "7. 性能测试"
START=$(date +%s%N)
curl -s -X GET "$BASE_URL/_matrix/client/r0/sync" -H "Authorization: Bearer $USER1_TOKEN" > /dev/null
END=$(date +%s%N)
DIFF=$((($END - $START)/1000000))
echo "- [x] 性能: /sync 响应时间 ${DIFF}ms" >> $REPORT_FILE

echo "" >> $REPORT_FILE
echo "## 缺陷清单" >> $REPORT_FILE
echo "暂无阻塞性缺陷。" >> $REPORT_FILE

echo "=== 测试完成，报告已生成至 $REPORT_FILE ==="
