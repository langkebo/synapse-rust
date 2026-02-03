#!/bin/bash

###############################################################################
# Synapse Rust 测试配置文件
#
# 功能: 定义测试环境的配置参数
# 版本: 1.0.0
# 创建日期: 2026-02-02
###############################################################################

# 服务器配置
export SERVER_URL="${SERVER_URL:-http://localhost:8008}"
export SERVER_NAME="${SERVER_NAME:-localhost}"

# 数据库配置
export DATABASE_URL="${DATABASE_URL:-postgres://synapse:synapse@localhost:5432/synapse_test}"

# JWT配置
export JWT_SECRET="${JWT_SECRET:-test_secret_key_for_development_only_change_in_production}"
export JWT_EXPIRY="${JWT_EXPIRY:-86400}"
export REFRESH_TOKEN_EXPIRY="${REFRESH_TOKEN_EXPIRY:-604800}"

# 认证配置
export TOKEN_EXPIRY="${TOKEN_EXPIRY:-86400}"
export PASSWORD_MIN_LENGTH="${PASSWORD_MIN_LENGTH:-8}"
export REQUIRE_STRONG_PASSWORD="${REQUIRE_STRONG_PASSWORD:-false}"

# 测试用户配置
export TEST_USER_PREFIX="testuser"
export TEST_PASSWORD="TestPassword123"
export TEST_ADMIN_PASSWORD="AdminPassword789"

# 测试超时配置
export REQUEST_TIMEOUT="${REQUEST_TIMEOUT:-30}"
export CONNECT_TIMEOUT="${CONNECT_TIMEOUT:-10}"

# 测试结果配置
export RESULT_DIR="${RESULT_DIR:-./tests/results}"
export LOG_LEVEL="${LOG_LEVEL:-INFO}"

# 颜色输出配置
export COLOR_OUTPUT="${COLOR_OUTPUT:-true}"

# 打印配置信息
print_config() {
    echo "========================================"
    echo "测试环境配置"
    echo "========================================"
    echo "服务器URL: $SERVER_URL"
    echo "服务器名称: $SERVER_NAME"
    echo "数据库URL: $DATABASE_URL"
    echo "JWT密钥: ${JWT_SECRET:0:10}..."
    echo "令牌过期时间: ${TOKEN_EXPIRY}秒"
    echo "刷新令牌过期时间: ${REFRESH_TOKEN_EXPIRY}秒"
    echo "密码最小长度: ${PASSWORD_MIN_LENGTH}"
    echo "强密码要求: $REQUIRE_STRONG_PASSWORD"
    echo "结果目录: $RESULT_DIR"
    echo "========================================"
}

# 如果直接运行此脚本，打印配置信息
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    print_config
fi
