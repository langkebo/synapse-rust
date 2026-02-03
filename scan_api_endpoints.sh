#!/bin/bash

# API端点统计脚本
# 扫描所有路由文件并提取API端点信息

echo "=== Synapse Rust API 端点统计 ===" > /tmp/api_endpoints.txt
echo "扫描时间: $(date)" >> /tmp/api_endpoints.txt
echo "" >> /tmp/api_endpoints.txt

# 扫描主路由文件
echo "## 主路由 (mod.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/mod.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描管理路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 管理路由 (admin.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/admin.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描联邦路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 联邦路由 (federation.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/federation.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描语音路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 语音路由 (voice.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/voice.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描好友路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 好友路由 (friend.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/friend.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描端到端加密路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 端到端加密路由 (e2ee_routes.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/e2ee_routes.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描媒体路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 媒体路由 (media.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/media.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描私聊路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 私聊路由 (private_chat.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/private_chat.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

# 扫描密钥备份路由文件
echo "" >> /tmp/api_endpoints.txt
echo "## 密钥备份路由 (key_backup.rs)" >> /tmp/api_endpoints.txt
grep -n "\.route(" /home/hula/synapse_rust/src/web/routes/key_backup.rs | \
    sed 's/.*\.route("\([^"]*\)", \([^)]*\)).*/\1 | \2/' >> /tmp/api_endpoints.txt

echo "" >> /tmp/api_endpoints.txt
echo "=== 统计完成 ===" >> /tmp/api_endpoints.txt

# 显示结果
cat /tmp/api_endpoints.txt