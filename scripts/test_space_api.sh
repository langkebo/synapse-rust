#!/bin/bash

# Space API 测试脚本
# 测试所有 21 个端点

set -e

BASE_URL="http://localhost:28008"
TEST_USER="spacetest"
TEST_PASSWORD="Test@123"

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 计数器
TOTAL=0
PASSED=0
FAILED=0

echo "=========================================="
echo "Space API 测试 - $(date)"
echo "=========================================="

# 登录获取 token
echo -e "\n${YELLOW}[1/21]${NC} 登录获取 token..."
LOGIN_RESPONSE=$(curl -s -X POST "${BASE_URL}/_matrix/client/v3/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\":\"m.login.password\",\"user\":\"${TEST_USER}\",\"password\":\"${TEST_PASSWORD}\"}")

TOKEN=$(echo "$LOGIN_RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)

if [ -z "$TOKEN" ]; then
    echo -e "${RED}❌ 登录失败${NC}"
    exit 1
fi

echo -e "${GREEN}✅ 登录成功${NC}"
((TOTAL++))
((PASSED++))

# 1. 创建 Space
echo -e "\n${YELLOW}[2/21]${NC} 创建 Space..."

# 直接使用 Space ID 作为 room_id
SPACE_ID="!test_space:$(date +%s)"

CREATE_SPACE_RESPONSE=$(curl -s -X POST "${BASE_URL}/spaces" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d "{
        \"room_id\": \"${SPACE_ID}\",
        \"name\": \"Test Space\",
        \"topic\": \"A test space for API testing\",
        \"avatar_url\": null,
        \"join_rule\": \"invite\",
        \"visibility\": \"private\",
        \"is_public\": false,
        \"parent_space_id\": null
    }")

if echo "$CREATE_SPACE_RESPONSE" | grep -q "space_id"; then
    echo -e "${GREEN}✅ 创建 Space 成功${NC}"
    SPACE_ID=$(echo "$CREATE_SPACE_RESPONSE" | grep -o '"space_id":"[^"]*"' | cut -d'"' -f4)
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 创建 Space 失败: $CREATE_SPACE_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 2. 获取公开 Spaces
echo -e "\n${YELLOW}[3/21]${NC} 获取公开 Spaces..."
PUBLIC_SPACES_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/public" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$PUBLIC_SPACES_RESPONSE" | grep -q "space_id\|\" \[\]"; then
    echo -e "${GREEN}✅ 获取公开 Spaces 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取公开 Spaces 失败: $PUBLIC_SPACES_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 3. 搜索 Spaces
echo -e "\n${YELLOW}[4/21]${NC} 搜索 Spaces..."
SEARCH_SPACES_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/search?query=test" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$SEARCH_SPACES_RESPONSE" | grep -q "space_id\|\" \[\]"; then
    echo -e "${GREEN}✅ 搜索 Spaces 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 搜索 Spaces 失败: $SEARCH_SPACES_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 4. 获取 Space 统计
echo -e "\n${YELLOW}[5/21]${NC} 获取 Space 统计..."
STATS_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/statistics" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$STATS_RESPONSE" | grep -q "total_spaces\|count"; then
    echo -e "${GREEN}✅ 获取 Space 统计成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 统计失败: $STATS_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 5. 获取用户 Spaces
echo -e "\n${YELLOW}[6/21]${NC} 获取用户 Spaces..."
USER_SPACES_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/user" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$USER_SPACES_RESPONSE" | grep -q "space_id\|\" \[\]"; then
    echo -e "${GREEN}✅ 获取用户 Spaces 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取用户 Spaces 失败: $USER_SPACES_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 6. 获取单个 Space
echo -e "\n${YELLOW}[7/21]${NC} 获取单个 Space..."
GET_SPACE_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$GET_SPACE_RESPONSE" | grep -q "space_id"; then
    echo -e "${GREEN}✅ 获取单个 Space 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取单个 Space 失败: $GET_SPACE_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 7. 更新 Space
echo -e "\n${YELLOW}[8/21]${NC} 更新 Space..."
UPDATE_SPACE_RESPONSE=$(curl -s -X PUT "${BASE_URL}/spaces/${SPACE_ID}" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{
        "name": "Updated Space Name",
        "topic": "Updated topic",
        "join_rule": "public",
        "is_public": true
    }')

if echo "$UPDATE_SPACE_RESPONSE" | grep -q "name.*Updated"; then
    echo -e "${GREEN}✅ 更新 Space 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 更新 Space 失败: $UPDATE_SPACE_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 8. 获取 Space 子空间
echo -e "\n${YELLOW}[9/21]${NC} 获取 Space 子空间..."
CHILDREN_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/children" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$CHILDREN_RESPONSE" | grep -q "room_id\|\" \[\]"; then
    echo -e "${GREEN}✅ 获取 Space 子空间成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 子空间失败: $CHILDREN_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 9. 添加子空间
echo -e "\n${YELLOW}[10/21]${NC} 添加子空间..."
ADD_CHILD_RESPONSE=$(curl -s -X POST "${BASE_URL}/spaces/${SPACE_ID}/children" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{
        "room_id": "!child_room:localhost",
        "via_servers": ["localhost"],
        "suggested": true
    }')

if echo "$ADD_CHILD_RESPONSE" | grep -q "room_id"; then
    echo -e "${GREEN}✅ 添加子空间成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 添加子空间失败: $ADD_CHILD_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 10. 移除子空间
echo -e "\n${YELLOW}[11/21]${NC} 移除子空间..."
REMOVE_CHILD_RESPONSE=$(curl -s -X DELETE "${BASE_URL}/spaces/${SPACE_ID}/children/!child_room:localhost" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$REMOVE_CHILD_RESPONSE" | grep -q "room_id\|deleted"; then
    echo -e "${GREEN}✅ 移除子空间成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 移除子空间失败: $REMOVE_CHILD_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 11. 获取 Space 成员
echo -e "\n${YELLOW}[12/21]${NC} 获取 Space 成员..."
MEMBERS_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/members" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$MEMBERS_RESPONSE" | grep -q "user_id\|\" \[\]"; then
    echo -e "${GREEN}✅ 获取 Space 成员成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 成员失败: $MEMBERS_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 12. 获取 Space 房间
echo -e "\n${YELLOW}[13/21]${NC} 获取 Space 房间..."
ROOMS_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/rooms" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$ROOMS_RESPONSE" | grep -q "room_id\|\" \[\]"; then
    echo -e "${GREEN}✅ 获取 Space 房间成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 房间失败: $ROOMS_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 13. 获取 Space 状态
echo -e "\n${YELLOW}[14/21]${NC} 获取 Space 状态..."
STATE_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/state" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$STATE_RESPONSE" | grep -q "state_events\|\" \[\]"; then
    echo -e "${GREEN}✅ 获取 Space 状态成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 状态失败: $STATE_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 14. 邀请用户加入 Space
echo -e "\n${YELLOW}[15/21]${NC} 邀请用户加入 Space..."
INVITE_RESPONSE=$(curl -s -X POST "${BASE_URL}/spaces/${SPACE_ID}/invite" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{
        "user_id": "@testuser:localhost"
    }')

if echo "$INVITE_RESPONSE" | grep -q "room_id\|event_id"; then
    echo -e "${GREEN}✅ 邀请用户成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 邀请用户失败: $INVITE_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 15. 加入 Space
echo -e "\n${YELLOW}[16/21]${NC} 加入 Space..."
JOIN_RESPONSE=$(curl -s -X POST "${BASE_URL}/spaces/${SPACE_ID}/join" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$JOIN_RESPONSE" | grep -q "room_id"; then
    echo -e "${GREEN}✅ 加入 Space 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 加入 Space 失败: $JOIN_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 16. 离开 Space
echo -e "\n${YELLOW}[17/21]${NC} 离开 Space..."
LEAVE_RESPONSE=$(curl -s -X POST "${BASE_URL}/spaces/${SPACE_ID}/leave" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$LEAVE_RESPONSE" | grep -q "room_id"; then
    echo -e "${GREEN}✅ 离开 Space 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 离开 Space 失败: $LEAVE_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 17. 获取 Space 层级结构
echo -e "\n${YELLOW}[18/21]${NC} 获取 Space 层级结构..."
HIERARCHY_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/hierarchy" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$HIERARCHY_RESPONSE" | grep -q "room_id\|children"; then
    echo -e "${GREEN}✅ 获取 Space 层级结构成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 层级结构失败: $HIERARCHY_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 18. 获取 Space 层级结构 v1
echo -e "\n${YELLOW}[19/21]${NC} 获取 Space 层级结构 v1..."
HIERARCHY_V1_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/hierarchy/v1" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$HIERARCHY_V1_RESPONSE" | grep -q "room_id\|children"; then
    echo -e "${GREEN}✅ 获取 Space 层级结构 v1 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 层级结构 v1 失败: $HIERARCHY_V1_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 19. 获取 Space 摘要
echo -e "\n${YELLOW}[20/21]${NC} 获取 Space 摘要..."
SUMMARY_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/summary" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$SUMMARY_RESPONSE" | grep -q "room_id\|name"; then
    echo -e "${GREEN}✅ 获取 Space 摘要成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 摘要失败: $SUMMARY_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 20. 获取 Space 摘要（含子空间）
echo -e "\n${YELLOW}[21/21]${NC} 获取 Space 摘要（含子空间）..."
SUMMARY_WITH_CHILDREN_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/summary/with_children" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$SUMMARY_WITH_CHILDREN_RESPONSE" | grep -q "room_id\|name"; then
    echo -e "${GREEN}✅ 获取 Space 摘要（含子空间）成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 摘要（含子空间）失败: $SUMMARY_WITH_CHILDREN_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 21. 获取 Space 树路径
echo -e "\n${YELLOW}[22/21]${NC} 获取 Space 树路径..."
TREE_PATH_RESPONSE=$(curl -s -X GET "${BASE_URL}/spaces/${SPACE_ID}/tree_path" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$TREE_PATH_RESPONSE" | grep -q "tree_path\|path"; then
    echo -e "${GREEN}✅ 获取 Space 树路径成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 获取 Space 树路径失败: $TREE_PATH_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 清理：删除测试 Space
echo -e "\n${YELLOW}[清理]${NC} 删除测试 Space..."
DELETE_SPACE_RESPONSE=$(curl -s -X DELETE "${BASE_URL}/spaces/${SPACE_ID}" \
    -H "Authorization: Bearer ${TOKEN}")

if echo "$DELETE_SPACE_RESPONSE" | grep -q "room_id\|deleted"; then
    echo -e "${GREEN}✅ 删除 Space 成功${NC}"
    ((TOTAL++))
    ((PASSED++))
else
    echo -e "${RED}❌ 删除 Space 失败: $DELETE_SPACE_RESPONSE${NC}"
    ((TOTAL++))
    ((FAILED++))
fi

# 测试总结
echo ""
echo "=========================================="
echo "测试完成"
echo "=========================================="
echo -e "总计: ${TOTAL} 个测试"
echo -e "通过: ${GREEN}${PASSED}${NC} 个"
echo -e "失败: ${RED}${FAILED}${NC} 个"
echo -e "通过率: ${YELLOW}$(echo "scale=1; $PASSED * 100 / $TOTAL" | bc)${NC}%"

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ 所有测试通过！${NC}"
    exit 0
else
    echo -e "${RED}❌ 有 $FAILED 个测试失败${NC}"
    exit 1
fi
