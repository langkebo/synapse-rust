# ============================================================================
# Synapse Rust - 新服务器部署指南
# ============================================================================
# 适用于从 Docker Hub 拉取 vmuser232922/synapse-rust:v3.1 部署
# ============================================================================

# ============================================================================
# 方式一：使用 docker-compose.prod.yml (推荐)
# ============================================================================

# 1. 在新服务器上创建项目目录
ssh user@your-server
mkdir -p ~/synapse-rust && cd ~/synapse-rust

# 2. 下载 docker-compose.prod.yml
curl -O https://raw.githubusercontent.com/your-repo/synapse-rust/main/docker/docker-compose.prod.yml

# 或者手动创建文件 (内容如下)
cat > docker-compose.prod.yml << 'EOF'
services:
  synapse-rust:
    image: vmuser232922/synapse-rust:v3.1
    container_name: synapse-rust
    restart: unless-stopped
    networks:
      - synapse_network
    ports:
      - "8008:8008"
      - "8448:8448"
    environment:
      - RUST_LOG=info
      - DATABASE_URL=postgres://synapse:synapse@synapse-postgres:5432/synapse
      - REDIS_URL=redis://synapse-redis:6379
    volumes:
      - synapse_data:/app/data
    depends_on:
      synapse-postgres:
        condition: service_healthy
      synapse-redis:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8008/_matrix/client/versions"]
      interval: 30s
      timeout: 10s
      retries: 5
      start_period: 30s

  synapse-postgres:
    image: postgres:16-alpine
    container_name: synapse-postgres
    restart: unless-stopped
    environment:
      - POSTGRES_USER=synapse
      - POSTGRES_PASSWORD=synapse
      - POSTGRES_DB=synapse
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U synapse -d synapse"]
      interval: 10s
      timeout: 5s
      retries: 5

  synapse-redis:
    image: redis:7-alpine
    container_name: synapse-redis
    restart: unless-stopped
    command: >
      redis-server
      --appendonly yes
      --maxmemory 256mb
      --maxmemory-policy allkeys-lru
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
  redis_data:
  synapse_data:

networks:
  synapse_network:
    driver: bridge
EOF

# 3. 启动服务
docker compose -f docker-compose.prod.yml up -d

# 4. 查看日志确认迁移执行
docker logs synapse-rust 2>&1 | grep -i migrat

# 5. 验证数据库表
docker exec synapse-postgres psql -U synapse -d synapse -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';"

# ============================================================================
# 方式二：手动部署
# ============================================================================

# 1. 拉取镜像
docker pull vmuser232922/synapse-rust:v3.1

# 2. 创建网络
docker network create synapse_network

# 3. 启动 PostgreSQL
docker run -d \
  --name synapse-postgres \
  --restart unless-stopped \
  --network synapse_network \
  -e POSTGRES_USER=synapse \
  -e POSTGRES_PASSWORD=synapse \
  -e POSTGRES_DB=synapse \
  -v synapse_postgres_data:/var/lib/postgresql/data \
  postgres:16-alpine

# 4. 启动 Redis
docker run -d \
  --name synapse-redis \
  --restart unless-stopped \
  --network synapse_network \
  -v synapse_redis_data:/data \
  redis:7-alpine \
  redis-server --appendonly yes --maxmemory 256mb --maxmemory-policy allkeys-lru

# 5. 等待数据库就绪
sleep 10

# 6. 启动 Synapse Rust (会自动运行迁移)
docker run -d \
  --name synapse-rust \
  --restart unless-stopped \
  --network synapse_network \
  -p 8008:8008 \
  -p 8448:8448 \
  -v synapse_data:/app/data \
  -e DATABASE_URL=postgres://synapse:synapse@synapse-postgres:5432/synapse \
  -e REDIS_URL=redis://synapse-redis:6379 \
  -e RUST_LOG=info \
  vmuser232922/synapse-rust:v3.1

# ============================================================================
# 验证部署
# ============================================================================

# 检查容器状态
docker ps --format "table {{.Names}}\t{{.Status}}"

# 检查迁移文件是否存在
docker exec synapse-rust ls -la /app/migrations/ | head -10

# 检查数据库表数量 (应该 > 170)
docker exec synapse-postgres psql -U synapse -d synapse -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';"

# 测试 API
curl http://localhost:8008/_matrix/client/versions

# 查看服务器日志
docker logs synapse-rust 2>&1 | tail -20

# ============================================================================
# 常见问题
# ============================================================================

# Q: 数据库只有4个表怎么办？
# A: 服务器启动时会自动运行迁移。如果迁移失败，检查日志:
#    docker logs synapse-rust 2>&1 | grep -i error

# Q: 如何手动运行迁移？
# A: 进入容器并执行迁移脚本:
#    docker exec -it synapse-rust /app/scripts/run-migrations.sh

# Q: 迁移文件目录在哪？
# A: 容器内 /app/migrations/，镜像已包含所有迁移 SQL 文件

# ============================================================================
