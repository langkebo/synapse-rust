#!/bin/bash

# Synapse Matrix Server - 创建测试房间脚本
# 服务器地址
SERVER="http://localhost:8008"

# 颜色输出
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Access Tokens
ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46Y2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAYWRtaW46Y2p5c3R4LnRvcCIsImFkbWluIjpmYWxzZSwiZXhwIjoxNzcxMDQ2NjQ5LCJpYXQiOjE3NzEwNDMwNDksImRldmljZV9pZCI6IlRFU1RfREVWSUNFX2FkbWluIn0.HoSQO7Cv9j9IM8_gkA9P9HF2YNALTCTh9qlYqsf_sPQ"
USER1_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXJfbmV3XzE6Y2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAdGVzdHVzZXJfbmV3XzE6Y2p5c3R4LnRvcCIsImFkbWluIjpmYWxzZSwiZXhwIjoxNzcxMDQ2NjUwLCJpYXQiOjE3NzEwNDMwNTAsImRldmljZV9pZCI6IkZyZFhRVjFEa2pFdWtlVFRlbFlKcUEifQ.NU_ubFfTyrYwwX81aExybK2Z-0OyPddNOwwEyrs5RGw"
USER2_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXJfbmV3XzI6Y2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAdGVzdHVzZXJfbmV3XzI6Y2p5c3R4LnRvcCIsImFkbWluIjpmYWxzZSwiZXhwIjoxNzcxMDQ2NjUwLCJpYXQiOjE3NzEwNDMwNTAsImRldmljZV9pZCI6IndDYVY4VlFlMFE3Rk45SmdvTXRKRVEifQ.9zgXggEKLn_207cZLTUI_V36RKsjVh9V6CUNMom2kUQ"

echo "=========================================="
echo "Synapse Matrix Server - 创建测试房间"
echo "=========================================="
echo ""

# 函数：创建房间
create_room() {
    local name=$1
    local topic=$2
    local visibility=$3
    local preset=$4
    local token=$5
    
    echo -e "${YELLOW}正在创建房间: $name${NC}"
    
    response=$(curl -s -X POST "$SERVER/_matrix/client/r0/createRoom" \
        -H "Authorization: Bearer $token" \
        -H "Content-Type: application/json" \
        -d "{
            \"name\": \"$name\",
            \"topic\": \"$topic\",
            \"visibility\": \"$visibility\",
            \"preset\": \"$preset\",
            \"initial_state\": []
        }")
    
    # 检查是否成功
    if echo "$response" | jq -e '.room_id' > /dev/null 2>&1; then
        room_id=$(echo "$response" | jq -r '.room_id')
        echo -e "${GREEN}✓ 房间创建成功${NC}"
        echo "  房间名称: $name"
        echo "  房间 ID: $room_id"
        echo "  可见性: $visibility"
        echo "  预设: $preset"
        echo ""
        
        # 保存到文件
        echo "$name|$room_id|$visibility|$preset" >> /tmp/synapse_rooms.txt
        return 0
    else
        error=$(echo "$response" | jq -r '.error // .errcode')
        echo -e "${RED}✗ 房间创建失败: $error${NC}"
        echo ""
        return 1
    fi
}

# 清空之前的记录
> /tmp/synapse_rooms.txt

# 创建测试房间
echo "开始创建测试房间..."
echo ""

# 1. 公开房间
create_room "Test Public Room" "这是一个公开测试房间" "public" "public_chat" "$USER1_TOKEN"

# 2. 私有房间
create_room "Test Private Room" "这是一个私有测试房间" "private" "private_chat" "$USER1_TOKEN"

# 3. 直接消息房间
create_room "Test Direct Chat" "这是一个直接消息房间" "private" "trusted_private_chat" "$USER1_TOKEN"

# 4. 群组房间
create_room "Test Group" "这是一个群组测试房间" "private" "private_chat" "$USER1_TOKEN"

echo "=========================================="
echo "房间创建完成！"
echo "=========================================="
echo ""
echo "房间列表:"
echo "房间名称 | 房间ID | 可见性 | 预设"
echo "-------- | ------ | ------ | ----"
cat /tmp/synapse_rooms.txt
echo ""
echo "房间信息已保存到: /tmp/synapse_rooms.txt"
