#!/bin/bash
# 快速生成所有必需的环境变量密钥

echo "# Synapse-Rust 环境变量"
echo "# 生成时间: $(date)"
echo ""
echo "# ===== 必需变量 ====="
echo "OLM_PICKLE_KEY=$(openssl rand -hex 32)"
# 口令用十六进制：base64 会产出 `+` `/` `=`，嵌进 URL 需要百分号转义，
# 直接拼进 DATABASE_URL 会得到一个连不上的地址。
DB_PW="$(openssl rand -hex 24)"
echo "SYNAPSE_DB_PASSWORD=${DB_PW}"
echo "SYNAPSE_MACAROON_SECRET=$(openssl rand -base64 32)"
echo "SYNAPSE_FORM_SECRET=$(openssl rand -base64 32)"
echo "SYNAPSE_REGISTRATION_SECRET=$(openssl rand -base64 32)"
echo "SYNAPSE_SECURITY_SECRET=$(openssl rand -base64 32)"
echo ""
echo "# ===== 服务器配置 ====="
echo "SERVER_NAME=localhost"
echo "PUBLIC_BASEURL=http://localhost:8008"
echo ""
echo "# ===== 数据库配置 ====="
# 必须用上面那个随机口令：此前这里写死 `synapse:synapse`，与同一份输出里的
# `SYNAPSE_DB_PASSWORD` 自相矛盾（生成了一把随机口令却不用它）。
echo "DATABASE_URL=postgresql://synapse:${DB_PW}@localhost:5432/synapse"
echo ""
echo "# ===== Redis 配置 ====="
echo "REDIS_URL=redis://localhost:6379"
