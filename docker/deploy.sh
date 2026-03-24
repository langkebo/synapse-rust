#!/bin/bash
set -e

echo "=========================================="
echo " synapse-rust Production Deployment Script"
echo "=========================================="

cd "$(dirname "$0")"

generate_password() {
    openssl rand -base64 32 | tr -dc 'a-zA-Z0-9' | head -c 32
}

echo "[0/8] Generating secure passwords..."
DB_PASSWORD=$(generate_password)
REDIS_PASSWORD=$(generate_password)
SECRET_KEY=$(openssl rand -hex 32)
MACAROON_SECRET=$(openssl rand -base64 32 | tr -dc 'a-zA-Z0-9' | head -c 64)
FORM_SECRET=$(openssl rand -base64 32 | tr -dc 'a-zA-Z0-9' | head -c 64)
REGISTRATION_SECRET=$(openssl rand -base64 32 | tr -dc 'a-zA-Z0-9' | head -c 64)
ADMIN_SECRET=$(openssl rand -base64 24 | tr -dc 'a-zA-Z0-9' | head -c 24)
FEDERATION_SIGNING_KEY=$(openssl rand -base64 32 | tr -dc 'a-zA-Z0-9' | head -c 43)
OLM_KEY=$(openssl rand -hex 32)

cat > .env << EOF
# Synapse Rust Environment Configuration
# ============================================================================
# 数据库配置
DB_HOST=db
DB_PORT=5432
DB_USER=synapse
DB_PASSWORD=${DB_PASSWORD}
DB_NAME=synapse
DATABASE_URL=postgres://synapse:${DB_PASSWORD}@db:5432/synapse

# Redis配置
REDIS_URL=redis://:${REDIS_PASSWORD}@redis:6379
REDIS_PASSWORD=${REDIS_PASSWORD}

# 服务器配置
SERVER_NAME=cjystx.top

# 安全密钥 - 生成方法: openssl rand -hex 32
SECRET_KEY=${SECRET_KEY}
MACAROON_SECRET=${MACAROON_SECRET}
FORM_SECRET=${FORM_SECRET}
REGISTRATION_SECRET=${REGISTRATION_SECRET}
ADMIN_SECRET=${ADMIN_SECRET}

# 联邦签名密钥
FEDERATION_SIGNING_KEY=${FEDERATION_SIGNING_KEY}

# 上传限制 (100MB)
MAX_UPLOAD_SIZE=104857600

# SQLx离线模式
SQLX_OFFLINE=false

# 日志配置
RUST_LOG=info
RUST_BACKTRACE=1

# 连接池配置
DATABASE_POOL_SIZE=20
REDIS_POOL_SIZE=20

# Argon2密码哈希参数
ARGON2_M_COST=8192
ARGON2_T_COST=4
ARGON2_P_COST=2

# 时区
TZ=Asia/Shanghai

# CORS 配置
ALLOWED_ORIGINS=http://localhost:5173,http://localhost:3000,http://localhost:8008,http://127.0.0.1:5173,http://127.0.0.1:3000,http://127.0.0.1:8008

# OLM加密密钥 (必须设置)
OLM_PICKLE_KEY=${OLM_KEY}

# 迁移配置 - 设置为 true 启用运行时迁移
RUN_MIGRATIONS=true
EOF

echo "✅ Environment file created with secure passwords"
echo "   Database password: ${DB_PASSWORD:0:8}..."
echo "   Redis password: ${REDIS_PASSWORD:0:8}..."

echo "[1/8] Calculating migration checksum..."
MIGRATION_CHECKSUM=$(cd .. && find migrations -name "*.sql" -type f -exec md5sum {} \; | sort | md5sum | cut -d' ' -f1)
BUILD_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "   Migration checksum: ${MIGRATION_CHECKSUM:0:16}..."

echo "[2/8] Building Docker image (linux/amd64)..."
cd ..
docker buildx build \
    --platform linux/amd64 \
    --build-arg BUILD_DATE=${BUILD_DATE} \
    --build-arg MIGRATION_CHECKSUM=${MIGRATION_CHECKSUM} \
    -f docker/Dockerfile \
    -t synapse-rust-main:latest \
    --load \
    .

echo "[3/8] Tagging image..."
docker tag synapse-rust-main:latest vmuser232922/synapse-rust:latest

echo "[4/8] Starting database and Redis..."
cd docker
docker compose -f docker-compose.prod.yml up -d db redis

echo "Waiting for db and redis to be healthy..."
for i in {1..30}; do
    if docker compose -f docker-compose.prod.yml exec -T db pg_isready -U synapse > /dev/null 2>&1; then
        echo "✅ Database is ready"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "❌ Database failed to start"
        docker compose -f docker-compose.prod.yml logs db
        exit 1
    fi
    echo "   Waiting for database... ($i/30)"
    sleep 2
done

for i in {1..15}; do
    if docker compose -f docker-compose.prod.yml exec -T redis redis-cli -a "${REDIS_PASSWORD}" ping > /dev/null 2>&1; then
        echo "✅ Redis is ready"
        break
    fi
    if [ $i -eq 15 ]; then
        echo "❌ Redis failed to start"
        docker compose -f docker-compose.prod.yml logs redis
        exit 1
    fi
    echo "   Waiting for redis... ($i/15)"
    sleep 2
done

echo "[5/8] Checking database schema..."
SCHEMA_EXISTS=$(docker compose -f docker-compose.prod.yml exec -T db psql -U synapse -d synapse -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public' AND table_name='users';" 2>/dev/null | tr -d ' ')
if [ "$SCHEMA_EXISTS" -gt 0 ]; then
    echo "✅ Database schema already exists"
else
    echo "⚠️  Database schema not found, running initial migration..."
    docker cp ../migrations/00000000_unified_schema_v6.sql synapse_db_prod:/tmp/migration.sql
    docker compose -f docker-compose.prod.yml exec -T db psql -U synapse -d synapse -f /tmp/migration.sql
fi

echo "[6/8] Cleaning up old failed migrations..."
docker compose -f docker-compose.prod.yml exec -T db psql -U synapse -d synapse -c "DELETE FROM schema_migrations WHERE version = '20260322000001' OR version = 'UNIFIED';" 2>/dev/null || true

echo "[7/8] Starting synapse main service..."
docker compose -f docker-compose.prod.yml up -d synapse-main

echo "[8/8] Verifying deployment..."
sleep 10
ERRORS=$(docker compose -f docker-compose.prod.yml logs --tail=50 synapse-main 2>&1 | grep -i -E "panic|error returned from database" || true)
if [ -n "$ERRORS" ]; then
    echo "⚠️  Found potential issues:"
    echo "$ERRORS"
else
    echo "✅ Deployment completed successfully!"
fi

echo ""
echo "=========================================="
echo " Deployment Summary"
echo "=========================================="
echo ""
echo "📝 Service Endpoints:"
echo "   - Matrix Client API: http://localhost:8008"
echo "   - Database: localhost:5432"
echo "   - Redis: localhost:6379"
echo ""
echo "📝 Configuration:"
echo "   - All passwords stored in: $(pwd)/.env"
echo "   - Config file: $(pwd)/config/homeserver.yaml"
echo ""
echo "🔐 Security:"
echo "   - OLM_PICKLE_KEY=${OLM_KEY}"
echo ""
echo "🔧 Migration Status:"
docker compose -f docker-compose.prod.yml exec -T db psql -U synapse -d synapse -c "SELECT version, success FROM schema_migrations ORDER BY applied_ts;" 2>/dev/null || true
echo ""
echo "To view logs: docker compose -f docker-compose.prod.yml logs -f synapse-main"
echo "To stop: docker compose -f docker-compose.prod.yml down"
echo ""
