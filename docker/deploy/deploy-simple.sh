#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=========================================="
echo " Synapse-Rust 一键部署脚本"
echo "=========================================="

# 检查 .env 文件
if [ ! -f .env ]; then
    echo "❌ 未找到 .env 文件"
    echo "请复制 .env.example 为 .env 并配置必需的环境变量"
    exit 1
fi

# 加载环境变量
set -a
source .env
set +a

# 验证必需的环境变量
REQUIRED_VARS=(
    "SERVER_NAME"
    "PUBLIC_BASEURL"
    "POSTGRES_PASSWORD"
    "REDIS_PASSWORD"
    "ADMIN_SHARED_SECRET"
    "JWT_SECRET"
    "REGISTRATION_SHARED_SECRET"
    "SECRET_KEY"
    "MACAROON_SECRET"
    "FORM_SECRET"
)

echo "🔍 验证环境变量..."
MISSING_VARS=()
for var in "${REQUIRED_VARS[@]}"; do
    if [ -z "${!var:-}" ]; then
        MISSING_VARS+=("$var")
    fi
done

if [ ${#MISSING_VARS[@]} -gt 0 ]; then
    echo "❌ 缺少必需的环境变量:"
    for var in "${MISSING_VARS[@]}"; do
        echo "   - $var"
    done
    exit 1
fi

echo "✅ 环境变量验证通过"

# 创建必要的目录
mkdir -p data logs media

# 停止旧容器
echo ""
echo "[1/5] 停止旧容器..."
docker compose down --remove-orphans || true

# 拉取/构建镜像
echo ""
echo "[2/5] 准备镜像..."
if [ "${SYNAPSE_IMAGE:-}" = "vmuser232922/mysynapse:latest" ]; then
    echo "拉取生产镜像..."
    docker pull vmuser232922/mysynapse:latest
else
    echo "构建本地镜像..."
    docker compose build synapse
fi

# 启动数据库和 Redis
echo ""
echo "[3/5] 启动数据库和 Redis..."
docker compose up -d postgres redis

# 等待服务就绪
echo "等待数据库就绪..."
for i in {1..30}; do
    if docker compose exec -T postgres pg_isready -U ${POSTGRES_USER:-postgres} >/dev/null 2>&1; then
        echo "✅ 数据库就绪"
        break
    fi
    echo "等待中... ($i/30)"
    sleep 2
done

echo "等待 Redis 就绪..."
for i in {1..20}; do
    if docker compose exec -T redis redis-cli -a "${REDIS_PASSWORD}" ping >/dev/null 2>&1; then
        echo "✅ Redis 就绪"
        break
    fi
    echo "等待中... ($i/20)"
    sleep 1
done

# 执行数据库迁移
echo ""
echo "[4/5] 执行数据库迁移..."
docker compose up migrator
if [ $? -ne 0 ]; then
    echo "❌ 数据库迁移失败"
    exit 1
fi
echo "✅ 数据库迁移完成"

# 启动应用
echo ""
echo "[5/5] 启动应用服务..."
docker compose up -d synapse nginx

# 等待应用就绪
echo "等待应用就绪..."
for i in {1..40}; do
    if curl -sf http://localhost:${SYNAPSE_PORT:-8008}/_matrix/client/versions >/dev/null 2>&1; then
        echo "✅ 应用就绪"
        break
    fi
    echo "等待中... ($i/40)"
    sleep 3
done

echo ""
echo "=========================================="
echo "✅ 部署完成！"
echo "=========================================="
echo ""
echo "服务地址:"
echo "  - Client API: http://localhost:${SYNAPSE_PORT:-8008}"
echo "  - Federation: https://localhost:${FEDERATION_PORT:-8448}"
echo "  - Prometheus: http://localhost:${PROMETHEUS_PORT:-9090}/metrics"
echo ""
echo "查看日志: docker compose logs -f synapse"
echo "查看状态: docker compose ps"
echo "停止服务: docker compose down"
echo ""
docker compose ps
