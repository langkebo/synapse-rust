#!/bin/bash
# 一键启动开发环境

set -e

echo "=== Synapse-Rust 开发环境启动 ==="
echo ""

# 检查是否存在 .env 文件
if [ ! -f .env ]; then
    echo "⚠️  未找到 .env 文件，正在生成..."
    ./scripts/generate_env.sh > .env
    echo "✅ .env 文件已生成"
    echo ""
    echo "⚠️  请检查并修改 .env 文件中的配置"
    echo "   特别是数据库密码和域名配置"
    echo ""
    read -p "按 Enter 继续，或 Ctrl+C 取消..."
fi

# 加载环境变量
echo "📝 加载环境变量..."
set -a
source .env
set +a

# 验证配置
echo "🔍 验证配置..."
./scripts/validate_config.sh

# 启动 Docker 服务
echo ""
echo "🚀 启动 Docker 服务..."
cd docker && docker compose up -d

echo ""
echo "✅ 开发环境启动完成！"
echo ""
echo "服务地址:"
echo "  - Synapse: http://localhost:8008"
echo "  - PostgreSQL: localhost:5432"
echo "  - Redis: localhost:6379"
echo ""
echo "查看日志: docker compose logs -f"
echo "停止服务: docker compose down"
