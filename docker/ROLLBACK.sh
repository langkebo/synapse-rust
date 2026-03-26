#!/bin/bash
set -e

# ============================================================================
# synapse-rust 一键回滚脚本 v1.0
# ============================================================================

PROJECT_ROOT="/Users/ljf/Desktop/hu/synapse-rust"
DOCKER_DIR="$PROJECT_ROOT/docker"
IMAGE_NAME="vmuser232922/synapse-rust:latest"
PREV_IMAGE_NAME="vmuser232922/synapse-rust:previous"

echo "=========================================="
echo "⚠️ 开始 synapse-rust 回滚流程"
echo "=========================================="

cd "$DOCKER_DIR"

# 检查是否有可回滚的镜像
if ! docker image inspect $PREV_IMAGE_NAME > /dev/null 2>&1; then
    echo "❌ 错误: 未找到 $PREV_IMAGE_NAME 镜像，无法回滚。"
    exit 1
fi

echo "[1/3] 正在停止当前运行的服务..."
docker compose -f docker-compose.prod.yml stop synapse-main synapse-worker

echo "[2/3] 正在恢复上一个版本的镜像..."
docker tag $PREV_IMAGE_NAME $IMAGE_NAME

echo "[3/3] 正在重启服务..."
docker compose -f docker-compose.prod.yml up -d synapse-main synapse-worker

echo "=========================================="
echo "🎉 回滚完成！"
docker compose -f docker-compose.prod.yml ps
echo "=========================================="
