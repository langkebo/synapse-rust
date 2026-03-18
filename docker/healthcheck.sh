#!/bin/bash
# ============================================================================
# Healthcheck Script for Synapse Rust
# 用途: Docker healthcheck 端点检查
# ============================================================================

set -e

# 检查进程是否运行
if ! pgrep -x "synapse-rust" > /dev/null; then
    echo "Process not running"
    exit 1
fi

# 检查端口是否监听
if ! nc -z localhost 8008 2>/dev/null; then
    echo "Port 8008 not listening"
    exit 1
fi

# 检查 HTTP 端点
if curl -sf http://localhost:8008/_matrix/federation/v1/version > /dev/null 2>&1; then
    exit 0
fi

# 如果 /_matrix/federation 不可用，尝试其他端点
if curl -sf http://localhost:8008/_matrix/client/versions > /dev/null 2>&1; then
    exit 0
fi

echo "Healthcheck failed"
exit 1
