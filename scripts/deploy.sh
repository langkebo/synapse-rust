#!/bin/bash
# ============================================================================
# synapse-rust 一键部署脚本
# ============================================================================
# 功能:
#   - 自动生成安全的随机密码
#   - 启动数据库和 Redis
#   - 运行数据库迁移
#   - 启动 synapse-rust 服务
#
# 使用方法:
#   cd ~/Desktop/hu/synapse-rust
#   ./scripts/deploy.sh
# ============================================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 项目根目录
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

# 加载 .env 文件如果存在
if [ -f "$PROJECT_ROOT/.env" ]; then
    echo -e "${BLUE}加载现有 .env 配置...${NC}"
    source "$PROJECT_ROOT/.env"
fi

# 生成随机密码函数
generate_secret() {
    openssl rand -hex 32
}

# 打印带颜色的消息
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# ============================================================================
# 1. 生成随机密码
# ============================================================================
print_info "生成随机密码..."

# 如果环境变量未设置，则生成新的随机密码
OLM_PICKLE_KEY="${OLM_PICKLE_KEY:-$(generate_secret)}"
SECRET_KEY="${SECRET_KEY:-$(generate_secret)}"
MACAROON_SECRET="${MACAROON_SECRET:-$(generate_secret)}"
FORM_SECRET="${FORM_SECRET:-$(generate_secret)}"
REGISTRATION_SECRET="${REGISTRATION_SECRET:-$(generate_secret)}"

# Admin secret 需要更复杂的密码（至少32字符）
ADMIN_SECRET_LENGTH=48
ADMIN_SECRET="${ADMIN_SECRET:-$(openssl rand -base64 48 | tr -dc 'a-zA-Z0-9' | head -c $ADMIN_SECRET_LENGTH)}"

print_success "密码生成完成"

# ============================================================================
# 2. 导出环境变量
# ============================================================================
print_info "设置环境变量..."

export OLM_PICKLE_KEY
export SECRET_KEY
export MACAROON_SECRET
export FORM_SECRET
export REGISTRATION_SECRET
export ADMIN_SECRET
export SERVER_NAME="${SERVER_NAME:-localhost}"
export DATABASE_URL="${DATABASE_URL:-postgres://synapse:synapse@localhost:5432/synapse}"
export REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
export RUST_LOG="${RUST_LOG:-info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

print_success "环境变量设置完成"

# ============================================================================
# 3. 保存配置到 .env 文件
# ============================================================================
print_info "保存配置到 .env 文件..."

cat > "$PROJECT_ROOT/.env" << EOF
# ============================================================================
# synapse-rust 环境配置
# ============================================================================
# 生成时间: $(date '+%Y-%m-%d %H:%M:%S')
# 请勿将此文件提交到版本控制系统！
# ============================================================================

# OLM 加密密钥 (用于 Matrix E2EE)
OLM_PICKLE_KEY=$OLM_PICKLE_KEY

# 安全密钥 (主密钥)
SECRET_KEY=$SECRET_KEY

# Macaroon 密钥 (用于认证令牌)
MACAROON_SECRET=$MACAROON_SECRET

# 表单密钥 (用于表单验证)
FORM_SECRET=$FORM_SECRET

# 注册共享密钥 (用于管理注册)
REGISTRATION_SECRET=$REGISTRATION_SECRET

# 管理员密码
ADMIN_SECRET=$ADMIN_SECRET

# 服务器名称
SERVER_NAME=$SERVER_NAME

# 数据库连接
DATABASE_URL=$DATABASE_URL

# Redis 连接
REDIS_URL=$REDIS_URL

# 日志级别
RUST_LOG=$RUST_LOG

# Rust 回溯级别
RUST_BACKTRACE=$RUST_BACKTRACE
EOF

print_success "配置已保存到 .env 文件"

# ============================================================================
# 4. 启动数据库和 Redis (Docker)
# ============================================================================
print_info "启动数据库和 Redis..."

cd "$PROJECT_ROOT/docker"

# 检查 Docker 是否运行
if ! docker info > /dev/null 2>&1; then
    print_error "Docker 未运行，请先启动 Docker Desktop"
    exit 1
fi

# 启动数据库和 Redis
docker compose up -d db redis

print_success "数据库和 Redis 启动完成"

# 等待数据库就绪
print_info "等待数据库就绪..."
for i in {1..30}; do
    if docker compose exec -T db pg_isready -U synapse > /dev/null 2>&1; then
        print_success "数据库已就绪"
        break
    fi
    if [ $i -eq 30 ]; then
        print_error "数据库启动超时"
        exit 1
    fi
    sleep 1
done

# 等待 Redis 就绪
print_info "等待 Redis 就绪..."
for i in {1..15}; do
    if docker compose exec -T redis redis-cli ping > /dev/null 2>&1; then
        print_success "Redis 已就绪"
        break
    fi
    if [ $i -eq 15 ]; then
        print_error "Redis 启动超时"
        exit 1
    fi
    sleep 1
done

# ============================================================================
# 5. 运行数据库迁移
# ============================================================================
print_info "检查数据库迁移..."

cd "$PROJECT_ROOT"

# 检查是否需要运行迁移
MIGRATION_STATUS=$(docker compose exec -T db psql -U synapse -d synapse -t -c "SELECT COUNT(*) FROM schema_migrations" 2>/dev/null || echo "0")

if [ "$MIGRATION_STATUS" = "0" ] || [ -z "$MIGRATION_STATUS" ]; then
    print_info "运行数据库迁移..."
    cargo run --release --bin run_migrations
    print_success "数据库迁移完成"
else
    print_success "数据库迁移已是最新状态"
fi

# ============================================================================
# 6. 启动 synapse-rust
# ============================================================================
print_info "启动 synapse-rust 服务..."

# 使用 setcap 需要 root 权限，跳过
# 如果需要启用端口 < 1024，请使用:
# sudo setcap cap_net_bind_service=+ep target/release/synapse-rust

cargo run --release --bin synapse-rust &
SERVER_PID=$!

print_success "synapse-rust 服务已启动 (PID: $SERVER_PID)"

# ============================================================================
# 7. 等待服务就绪
# ============================================================================
print_info "等待服务就绪..."

for i in {1..30}; do
    if curl -s http://localhost:8008/_matrix/client/versions > /dev/null 2>&1; then
        print_success "synapse-rust 服务已就绪!"
        echo ""
        echo "========================================"
        echo -e "${GREEN}synapse-rust 服务启动成功!${NC}"
        echo "========================================"
        echo ""
        echo "服务地址: http://localhost:8008"
        echo "Fedsation: localhost:8448"
        echo ""
        echo "管理接口:"
        echo "  用户名: admin"
        echo "  密码:   $ADMIN_SECRET"
        echo ""
        echo "查看日志: docker compose -f $PROJECT_ROOT/docker/docker-compose.yml logs -f"
        echo "停止服务: kill $SERVER_PID"
        echo ""
        break
    fi
    if [ $i -eq 30 ]; then
        print_error "服务启动超时，请检查日志"
        exit 1
    fi
    sleep 1
done

# 保存 PID 到文件
echo $SERVER_PID > "$PROJECT_ROOT/.synapse-rust.pid"

print_success "部署完成!"
