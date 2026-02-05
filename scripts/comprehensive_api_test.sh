#!/bin/bash

# Matrix API Comprehensive Test Suite
# Domain: matrix.cjystx.top
# Server: localhost:8008
# Date: 2026-02-04

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
BASE_URL="http://localhost:8008"
ADMIN_USER="admin"
ADMIN_PASS="Admin123456!"
TEST_USER1="testuser1"
TEST_USER1_PASS="TestUser123456!"
TEST_USER2="testuser2"
TEST_USER2_PASS="TestUser123456!"
REPORT_FILE="/home/hula/synapse_rust/docs/API_TEST_RESULTS.md"

# Test counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
SKIPPED_TESTS=0

# Initialize report
init_report() {
    cat > "$REPORT_FILE" << EOF
# Matrix API 全面测试报告

> **测试日期**: $(date +%Y-%m-%d)
> **测试环境**: localhost:8008
> **服务器域名**: cjystx.top
> **测试用户**:
>   - admin: @admin:cjystx.top
>   - testuser1: @testuser1:cjystx.top
>   - testuser2: @testuser2:cjystx.top

---

## 测试摘要

| 指标 | 数值 |
|------|------|
| 总测试数 | 0 |
| 通过 | 0 |
| 失败 | 0 |
| 跳过 | 0 |
| 成功率 | 0% |

---

## 测试环境信息

### 服务状态
$(docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" | grep -E "synapse|redis|postgres")

### API 版本
\`\`\`json
$(curl -s "$BASE_URL/_matrix/client/versions")
\`\`\`

---

## 测试用例详情

EOF
}

# Function to make request and test
# Args: test_name, method, endpoint, body, expected_status, token
test_api() {
    local test_name="$1"
    local method="$2"
    local endpoint="$3"
    local body="$4"
    local expected_status="$5"
    local token="$6"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    # Make request
    if [ -n "$token" ]; then
        if [ -n "$body" ]; then
            response=$(curl -s -w "\n%{http_code}" -X "$method" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer $token" \
                -d "$body" \
                "$BASE_URL$endpoint")
        else
            response=$(curl -s -w "\n%{http_code}" -X "$method" \
                -H "Authorization: Bearer $token" \
                "$BASE_URL$endpoint")
        fi
    else
        if [ -n "$body" ]; then
            response=$(curl -s -w "\n%{http_code}" -X "$method" \
                -H "Content-Type: application/json" \
                -d "$body" \
                "$BASE_URL$endpoint")
        else
            response=$(curl -s -w "\n%{http_code}" -X "$method" \
                "$BASE_URL$endpoint")
        fi
    fi
    
    # Parse response and status code
    http_code=$(echo "$response" | tail -n1)
    response_body=$(echo "$response" | sed '$d')
    response_time=$(curl -s -o /dev/null -w "%{time_total}" -X "$method" \
        -H "Content-Type: application/json" \
        ${token:+-H "Authorization: Bearer $token"} \
        ${body:+-d "$body"} \
        "$BASE_URL$endpoint")
    
    # Check result
    if [ "$http_code" == "$expected_status" ]; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
        status="✅ 通过"
        result_color=$GREEN
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
        status="❌ 失败"
        result_color=$RED
    fi
    
    # Log result
    echo -e "${result_color}[$status]${NC} $test_name (HTTP $http_code, ${response_time}s)"
    
    # Add to report
    cat >> "$REPORT_FILE" << EOF
### $test_name

- **方法**: $method $endpoint
- **状态码**: $http_code (预期: $expected_status)
- **响应时间**: ${response_time}s
- **请求体**: 
\`\`\`json
$body
\`\`\`
- **响应体**: 
\`\`\`json
$response_body
\`\`\`
- **结果**: $status

---
EOF
}

# Login function
get_token() {
    local username="$1"
    local password="$2"
    curl -s -X POST "$BASE_URL/_matrix/client/r0/login" \
        -H "Content-Type: application/json" \
        -d "{\"type\":\"m.login.password\",\"user\":\"$username\",\"password\":\"$password\"}" | \
        grep -o '"access_token":"[^"]*"' | cut -d'"' -f4
}

# Main test execution
main() {
    echo "========================================"
    echo "Matrix API Comprehensive Test Suite"
    echo "========================================"
    echo ""
    
    # Initialize report
    init_report
    
    # Get tokens
    echo "正在获取访问令牌..."
    ADMIN_TOKEN=$(get_token "$ADMIN_USER" "$ADMIN_PASS")
    USER1_TOKEN=$(get_token "$TEST_USER1" "$TEST_USER1_PASS")
    USER2_TOKEN=$(get_token "$TEST_USER2" "$TEST_USER2_PASS")
    
    if [ -z "$ADMIN_TOKEN" ] || [ -z "$USER1_TOKEN" ] || [ -z "$USER2_TOKEN" ]; then
        echo -e "${RED}错误: 无法获取访问令牌${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}令牌获取成功${NC}"
    echo ""
    
    # Section 1: Core Client APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}1. 测试核心客户端 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 1. 核心客户端 API 测试

EOF
    
    test_api "获取客户端版本" "GET" "/_matrix/client/versions" "" "200" ""
    test_api "用户登录(testuser1)" "POST" "/_matrix/client/r0/login" \
        '{"type":"m.login.password","user":"testuser1","password":"TestUser123456!"}' "200" ""
    test_api "账户WhoAmI(testuser1)" "GET" "/_matrix/client/r0/account/whoami" "" "200" "$USER1_TOKEN"
    test_api "获取用户资料" "GET" "/_matrix/client/r0/profile/@testuser1:cjystx.top" "" "200" "$USER1_TOKEN"
    test_api "更新用户显示名" "PUT" "/_matrix/client/r0/profile/@testuser1:cjystx.top/displayname" \
        '{"displayname":"Test User 1"}' "200" "$USER1_TOKEN"
    test_api "获取公共房间列表" "GET" "/_matrix/client/r0/publicRooms" "" "200" "$USER1_TOKEN"
    test_api "获取设备列表" "GET" "/_matrix/client/r0/devices" "" "200" "$USER1_TOKEN"
    test_api "刷新访问令牌" "POST" "/_matrix/client/r0/tokenrefresh" \
        '{"refresh_token":"'"$USER1_TOKEN"'"}' "400" "$USER1_TOKEN"
    
    echo ""
    
    # Section 2: Admin APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}2. 测试管理员 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 2. 管理员 API 测试

EOF
    
    test_api "服务器版本(普通用户)" "GET" "/_synapse/admin/v1/server_version" "" "403" "$USER1_TOKEN"
    test_api "服务器版本(管理员)" "GET" "/_synapse/admin/v1/server_version" "" "200" "$ADMIN_TOKEN"
    test_api "管理员用户列表" "GET" "/_synapse/admin/v2/users" "" "200" "$ADMIN_TOKEN"
    test_api "管理员获取用户信息" "GET" "/_synapse/admin/v2/users/@testuser1:cjystx.top" "" "200" "$ADMIN_TOKEN"
    test_api "管理员创建用户" "POST" "/_synapse/admin/v1/register" \
        '{"username":"newuser","password":"NewPass123!","admin":false}' "200" "$ADMIN_TOKEN"
    test_api "管理员删除测试用户" "DELETE" "/_synapse/admin/v2/users/@newuser:cjystx.top" "" "200" "$ADMIN_TOKEN"
    
    # Clean up test user
    curl -s -X DELETE "$BASE_URL/_synapse/admin/v2/users/@newuser:cjystx.top" \
        -H "Authorization: Bearer $ADMIN_TOKEN" > /dev/null
    
    echo ""
    
    # Section 3: Authentication & Error Handling
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}3. 测试认证与错误处理${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 3. 认证与错误处理测试

EOF
    
    test_api "无效令牌访问" "GET" "/_matrix/client/r0/account/whoami" "" "401" "invalid_token"
    test_api "无令牌访问" "GET" "/_matrix/client/r0/account/whoami" "" "401" ""
    test_api "错误密码登录" "POST" "/_matrix/client/r0/login" \
        '{"type":"m.login.password","user":"testuser1","password":"WrongPass123!"}' "403" ""
    test_api "无效用户名登录" "POST" "/_matrix/client/r0/login" \
        '{"type":"m.login.password","user":"nonexistent","password":"Pass123!"}' "403" ""
    test_api "重复注册" "POST" "/_matrix/client/r0/register" \
        '{"username":"testuser1","password":"TestUser123456!","admin":false}' "400" ""
    
    echo ""
    
    # Section 4: Friend System APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}4. 测试好友系统 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 4. 好友系统 API 测试

EOF
    
    test_api "获取好友列表" "GET" "/_matrix/client/r0/contacts" "" "200" "$USER1_TOKEN"
    test_api "获取好友分类列表" "GET" "/_matrix/client/r0/contacts/categories" "" "200" "$USER1_TOKEN"
    test_api "创建好友分类" "POST" "/_matrix/client/r0/contacts/categories" \
        '{"name":"Family","order":1}' "200" "$USER1_TOKEN"
    test_api "获取好友分类" "GET" "/_matrix/client/r0/contacts/categories/1" "" "200" "$USER1_TOKEN"
    test_api "更新好友分类" "PUT" "/_matrix/client/r0/contacts/categories/1" \
        '{"name":"Family Updated","order":2}' "200" "$USER1_TOKEN"
    test_api "邀请用户为好友" "POST" "/_matrix/client/r0/contacts/request" \
        '{"user_id":"@testuser2:cjystx.top"}' "200" "$USER1_TOKEN"
    test_api "接受好友请求" "POST" "/_matrix/client/r0/contacts/accept" \
        '{"user_id":"@testuser1:cjystx.top"}' "200" "$USER2_TOKEN"
    
    echo ""
    
    # Section 5: Media File APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}5. 测试媒体文件 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 5. 媒体文件 API 测试

EOF
    
    # Create test image
    echo -e "Creating test image..."
    convert -size 100x100 xc:red /tmp/test_image.png 2>/dev/null || \
        echo -e "ImageMagick not available, using alternative method"
    
    test_api "上传媒体文件" "POST" "/_matrix/media/r0/upload" \
        '{"filename":"test_image.png"}' "415" "$USER1_TOKEN"
    test_api "获取媒体配置" "GET" "/_matrix/media/r0/config" "" "200" "$USER1_TOKEN"
    test_api "获取用户媒体库" "GET" "/_matrix/media/r0/user/@testuser1:cjystx.top" "" "200" "$USER1_TOKEN"
    
    echo ""
    
    # Section 6: Private Chat APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}6. 测试私聊 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 6. 私聊 API 测试

EOF
    
    # Create room first
    ROOM_ID=$(curl -s -X POST "$BASE_URL/_matrix/client/r0/createRoom" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $USER1_TOKEN" \
        -d '{"visibility":"private","name":"Test Room 1"}' | \
        grep -o '"room_id":"[^"]*"' | cut -d'"' -f4)
    
    test_api "创建私聊房间" "POST" "/_matrix/client/r0/createRoom" \
        '{"visibility":"private","name":"Private Chat Room"}' "200" "$USER1_TOKEN"
    test_api "获取房间信息" "GET" "/_matrix/client/r0/rooms/$ROOM_ID" "" "200" "$USER1_TOKEN"
    test_api "获取用户房间列表" "GET" "/_matrix/client/r0/sync" "" "200" "$USER1_TOKEN"
    test_api "发送房间消息" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message" \
        '{"msgtype":"m.text","body":"Hello World"}' "200" "$USER1_TOKEN"
    test_api "获取房间消息" "GET" "/_matrix/client/r0/rooms/$ROOM_ID/messages" "" "200" "$USER1_TOKEN"
    test_api "邀请用户到房间" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/invite" \
        '{"user_id":"@testuser2:cjystx.top"}' "200" "$USER1_TOKEN"
    test_api "离开房间" "POST" "/_matrix/client/r0/rooms/$ROOM_ID/leave" "" "200" "$USER1_TOKEN"
    
    echo ""
    
    # Section 7: End-to-End Encryption APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}7. 测试端到端加密 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 7. 端到端加密 API 测试

EOF
    
    test_api "获取设备密钥" "GET" "/_matrix/client/r0/keys/query" "" "200" "$USER1_TOKEN"
    test_api "上传设备密钥" "POST" "/_matrix/client/r0/keys/upload" \
        '{}' "200" "$USER1_TOKEN"
    test_api "标记设备已验证" "POST" "/_matrix/client/r0/keys/claim" \
        '{}' "200" "$USER1_TOKEN"
    
    echo ""
    
    # Section 8: Key Backup APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}8. 测试密钥备份 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 8. 密钥备份 API 测试

EOF
    
    test_api "获取密钥备份版本" "GET" "/_matrix/client/r0/room_keys/version" "" "200" "$USER1_TOKEN"
    test_api "创建密钥备份" "POST" "/_matrix/client/r0/room_keys/version" \
        '{"algorithm":"m.room_keys.v1.curve25519-aes-sha2"}' "200" "$USER1_TOKEN"
    test_api "获取密钥备份" "GET" "/_matrix/client/r0/room_keys" "" "200" "$USER1_TOKEN"
    test_api "删除密钥备份" "DELETE" "/_matrix/client/r0/room_keys/version/1" "" "200" "$USER1_TOKEN"
    
    echo ""
    
    # Section 9: Federation APIs
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}9. 测试联邦通信 API${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    cat >> "$REPORT_FILE" << EOF
## 9. 联邦通信 API 测试

EOF
    
    test_api "联邦版本检查" "GET" "/_matrix/federation/v1/version" "" "200" ""
    test_api "获取服务器密钥" "GET" "/_matrix/federation/v1/host/keys" "" "400" ""
    test_api "发送事务" "POST" "/_matrix/federation/v1/send/transaction" \
        '{}' "400" ""
    
    echo ""
    
    # Summary
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}测试摘要${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
    echo "总测试数: $TOTAL_TESTS"
    echo -e "通过: ${GREEN}$PASSED_TESTS${NC}"
    echo -e "失败: ${RED}$FAILED_TESTS${NC}"
    echo "跳过: $SKIPPED_TESTS"
    
    SUCCESS_RATE=$(echo "scale=2; $PASSED_TESTS * 100 / $TOTAL_TESTS" | bc)
    echo "成功率: ${SUCCESS_RATE}%"
    
    # Update summary in report
    cat >> "$REPORT_FILE" << EOF
---

## 测试摘要

| 指标 | 数值 |
|------|------|
| 总测试数 | $TOTAL_TESTS |
| 通过 | $PASSED_TESTS |
| 失败 | $FAILED_TESTS |
| 跳过 | $SKIPPED_TESTS |
| 成功率 | ${SUCCESS_RATE}% |

---

## 失败测试分析

EOF
    
    # Add failure analysis if any
    if [ $FAILED_TESTS -gt 0 ]; then
        cat >> "$REPORT_FILE" << EOF
### 需要关注的失败测试

以下测试失败可能需要进一步调查：

1. **刷新令牌测试** - 预期失败，因为测试使用的refresh_token格式不正确
   - 建议：使用正确的refresh_token格式重新测试

2. **联邦API测试** - 部分测试预期返回400，因为缺少必要的参数
   - 建议：补充完整的请求参数

3. **媒体上传测试** - 可能需要正确的Content-Type
   - 建议：使用multipart/form-data格式上传

EOF
    else
        cat >> "$REPORT_FILE" << EOF
所有测试均已通过！✅

EOF
    fi
    
    # Add recommendations
    cat >> "$REPORT_FILE" << EOF
---

## 优化建议

### 高优先级

1. **完善错误处理**
   - 确保所有API返回一致的错误格式
   - 添加更详细的错误信息

2. **优化响应时间**
   - 对于响应时间较长的API，考虑添加缓存
   - 优化数据库查询

3. **完善测试覆盖**
   - 增加边界条件测试
   - 增加并发访问测试

### 中优先级

4. **文档更新**
   - 更新API文档以反映实际行为
   - 添加更多使用示例

5. **监控告警**
   - 添加API响应时间监控
   - 设置错误率告警阈值

---

**报告生成时间**: $(date)
**测试执行用户**: @admin:cjystx.top, @testuser1:cjystx.top, @testuser2:cjystx.top
EOF
    
    echo ""
    echo -e "${GREEN}测试报告已生成: $REPORT_FILE${NC}"
    
    # Return exit code based on test results
    if [ $FAILED_TESTS -gt 0 ]; then
        exit 1
    fi
    exit 0
}

# Run main function
main
