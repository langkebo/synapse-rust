# Synapse Rust - Production Deployment Guide

## 📋 目录

- [概述](#概述)
- [架构设计](#架构设计)
- [快速部署](#快速部署)
- [生产环境配置](#生产环境配置)
- [域名和SSL配置](#域名和ssl配置)
- [服务发现配置](#服务发现配置)
- [监控和维护](#监控和维护)
- [故障排查](#故障排查)

---

## 概述

本指南描述了如何将 Synapse Rust Matrix Homeserver 部署到生产环境。

### 服务器信息

- **服务器域名**: `cjystx.top`
- **Matrix 服务器**: `matrix.cjystx.top`
- **用户格式**: `@user:cjystx.top`
- **Federation 端口**: `8448` (通过 Nginx 代理)
- **客户端 API 端口**: `8008`

### 核心组件

| 组件 | 版本 | 用途 |
|------|------|------|
| Synapse Rust | 0.1.0 | Matrix Homeserver |
| PostgreSQL | 16 | 主数据库 |
| Redis | 7 | 缓存和会话存储 |
| Nginx | Alpine | 反向代理和负载均衡 |

---

## 架构设计

```
                              ┌─────────────────────────────────────────┐
                              │              Internet                    │
                              └─────────────────────────────────────────┘
                                                 │
                                                 ▼
                              ┌─────────────────────────────────────────┐
                              │          Firewall (80, 443, 8448)       │
                              └─────────────────────────────────────────┘
                                                 │
                    ┌────────────────────────────┼────────────────────────────┐
                    │                            │                            │
                    ▼                            ▼                            ▼
        ┌───────────────────┐    ┌───────────────────┐    ┌───────────────────┐
        │   cjystx.top     │    │ matrix.cjystx.top │    │ matrix.cjystx.top │
        │   (端口 443)      │    │   (端口 8448)      │    │   (端口 443)      │
        │   服务发现        │    │   Federation      │    │   客户端 API      │
        └───────────────────┘    └───────────────────┘    └───────────────────┘
                    │                            │                            │
                    └────────────────────────────┼────────────────────────────┘
                                                 │
                                                 ▼
                              ┌─────────────────────────────────────────┐
                              │              Nginx                       │
                              │  - SSL/TLS 终端                          │
                              │  - 负载均衡                              │
                              │  - 健康检查                              │
                              │  - WebSocket 支持                        │
                              └─────────────────────────────────────────┘
                                                 │
                    ┌────────────────────────────┼────────────────────────────┐
                    │                            │                            │
                    ▼                            ▼                            ▼
        ┌───────────────────┐    ┌───────────────────┐    ┌───────────────────┐
        │  synapse-rust      │    │    PostgreSQL      │    │      Redis         │
        │  (端口 8008, 8448) │    │    (端口 5432)     │    │    (端口 6379)     │
        │  - 客户端 API      │    │  - 主数据库        │    │  - 缓存            │
        │  - Federation API  │    │  - 用户数据        │    │  - 会话            │
        └───────────────────┘    └───────────────────┘    └───────────────────┘
```

---

## 快速部署

### 前置要求

```bash
# 系统要求
- Docker Engine 20.x+
- Docker Compose V2
- 至少 2GB RAM
- 至少 20GB 磁盘空间
- 域名解析配置
```

### 部署步骤

#### 1. 准备部署目录

```bash
# 方式1: 从源码构建目录复制
cd /path/to/project
cp -r docker /opt/synapse-rust/
cd /opt/synapse-rust

# 方式2: 从版本控制克隆
git clone https://github.com/synapse-rust/synapse-rust.git
cd synapse-rust/docker
```

#### 2. 配置环境变量

```bash
# 创建环境变量文件
cat > .env << 'EOF'
# 数据库配置
DATABASE_URL=postgres://synapse:synapse@db:5432/synapse_test

# Redis配置
REDIS_URL=redis://redis:6379

# 服务器配置
SERVER_NAME=cjystx.top
SECRET_KEY=$(openssl rand -base64 32)

# 日志配置
RUST_LOG=info

# 域名配置
DOMAIN=cjystx.top
MATRIX_DOMAIN=matrix.cjystx.top
EOF

# 生成安全的密钥
export SECRET_KEY=$(openssl rand -base64 32)
echo "SECRET_KEY=$SECRET_KEY" >> .env
```

#### 3. 启动服务

```bash
# 构建并启动所有服务
docker compose up -d

# 等待服务健康
sleep 30

# 检查服务状态
docker compose ps
```

#### 4. 验证部署

```bash
# 1. 测试服务发现
curl https://cjystx.top/.well-known/matrix/server

# 2. 测试 Federation API
curl https://matrix.cjystx.top/_matrix/federation/v1/version

# 3. 测试客户端 API
curl http://localhost:8008/_matrix/client/versions

# 4. 检查数据库连接
docker exec synapse-postgres pg_isready -U synapse -d synapse_test

# 5. 检查 Redis 连接
docker exec synapse-redis redis-cli ping
```

---

## 生产环境配置

### 1. 数据库优化

#### PostgreSQL 配置

```yaml
# docker-compose.yml 中的数据库服务配置
db:
  image: postgres:16-alpine
  environment:
    - POSTGRES_USER=synapse
    - POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
    - POSTGRES_DB=synapse_test
  command: >
    postgres
    -c shared_buffers=256MB
    -c effective_cache_size=1GB
    -c work_mem=64MB
    -c maintenance_work_mem=256MB
    -c max_connections=200
    -c checkpoint_completion_target=0.9
    -c wal_buffers=16MB
    -c random_page_cost=1.1
  volumes:
    - postgres_data:/var/lib/postgresql/data
    - ./postgres/postgresql.conf:/etc/postgresql/postgresql.conf:ro
  deploy:
    resources:
      limits:
        memory: 2G
      reservations:
        memory: 1G
```

#### 推荐的 postgresql.conf

```ini
# /opt/synapse-rust/docker/postgres/postgresql.conf

# 内存配置
shared_buffers = 256MB
effective_cache_size = 1GB
work_mem = 64MB
maintenance_work_mem = 256MB

# 连接配置
max_connections = 200

# Write-Ahead Logging
wal_level = replica
checkpoint_completion_target = 0.9
wal_buffers = 16MB

# 查询优化
random_page_cost = 1.1
effective_io_concurrency = 200

# 自动清理
autovacuum = on
autovacuum_max_workers = 4
autovacuum_naptime = 30s

# 日志配置
log_min_duration_statement = 1000
log_line_prefix = '%t [%p]: [%l-1] user=%u,db=%d '
log_lock_waits = on
log_temp_files = 0

# 性能监控
shared_preload_libraries = 'pg_stat_statements'
pg_stat_statements.track = all
```

### 2. Redis 优化

```yaml
# docker-compose.yml 中的 Redis 配置
redis:
  image: redis:7-alpine
  command: redis-server
    --appendonly yes
    --maxmemory 512mb
    --maxmemory-policy allkeys-lru
    --tcp-backlog 511
    --tcp-keepalive 300
    --timeout 0
  deploy:
    resources:
      limits:
        memory: 1G
      reservations:
        memory: 256M
```

### 3. 应用配置

#### Homeserver 配置

```yaml
# /opt/synapse-rust/docker/config/homeserver.yaml

# 服务器身份
server_name: cjystx.top
report_stats: false

# 数据库
database:
  name: postgres
  host: db
  port: 5432
  user: synapse
  password: ${POSTGRES_PASSWORD}
  database: synapse_test
  pool_size: 20
  max_open_connections: 40

# Redis 缓存
redis:
  enabled: true
  host: redis
  port: 6379
  db: 0

# Federation
federation:
  enabled: true
  server_name: cjystx.top
  signing_key_retention: 7d
  verify_key: true

# 速率限制
rate_limiting:
  enabled: true
  window_size_ms: 1000
  default_rps: 50.0
  burst_count: 200

# 会话配置
auth:
  session_cookie_timeout: 86400000

# 房间配置
rooms:
  default_room_version: "10"
  history_visibility:
    default: joined
```

#### 环境变量

```bash
# /opt/synapse-rust/docker/.env

# 必须修改的值
POSTGRES_PASSWORD=your_secure_password_here
SECRET_KEY=your_256_bit_secret_key_here
REDIS_PASSWORD=your_redis_password_here

# 服务器配置
SERVER_NAME=cjystx.top
DOMAIN=cjystx.top
MATRIX_DOMAIN=matrix.cjystx.top

# 日志级别
RUST_LOG=info

# 性能配置
DATABASE_POOL_SIZE=20
REDIS_MAX_CONNECTIONS=100
```

### 4. 安全配置

#### 容器安全

```yaml
# docker-compose.yml 中的安全配置
synapse-rust:
  security_opt:
    - no-new-privileges:true
  read_only: true
  tmpfs:
    - /tmp:size=10M,mode=1777
 Cap_drop:
    - ALL
  cap_add:
    - NET_BIND_SERVICE
```

#### 网络安全

```yaml
# 网络隔离
networks:
  synapse_network:
    driver: bridge
    ipam:
      config:
        - subnet: 172.28.0.0/16
```

---

## 域名和 SSL 配置

### 1. DNS 记录

| 记录类型 | 主机名 | 值 | TTL |
|---------|--------|----|----|
| A | cjystx.top | 服务器 IP | 3600 |
| A | matrix.cjystx.top | 服务器 IP | 3600 |
| A | _matrix._tcp.cjystx.top | 服务器 IP 8448 | 3600 |

### 2. SSL 证书配置

#### 方式一: Let's Encrypt (推荐)

```bash
# 1. 安装 certbot
docker exec -it synapse-nginx apk add --no-cache certbot

# 2. 获取证书
docker exec -it synapse-nginx certbot certonly \
  --webroot \
  -w /var/www/html \
  -d cjystx.top \
  -d matrix.cjystx.top

# 3. 安装证书
docker exec -it synapse-nginx certbot install \
  --cert-path /etc/letsencrypt/live/cjystx.top/cert.pem \
  --key-path /etc/letsencrypt/live/cjystx.top/privkey.pem \
  --fullchain-path /etc/letsencrypt/live/cjystx.top/fullchain.pem

# 4. 更新 Nginx 配置
docker exec -it synapse-nginx nginx -s reload

# 5. 设置自动续期
crontab -e
# 添加:
# 0 0,12 * * * docker exec synapse-nginx certbot renew --quiet
```

#### 方式二: 自签名证书 (开发环境)

```bash
# 生成自签名证书
cd /opt/synapse-rust/docker/ssl

# 生成私钥
openssl genrsa -out server.key 4096

# 生成证书签名请求
openssl req -new -key server.key \
  -out server.csr \
  -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=cjystx.top"

# 生成自签名证书
openssl x509 -req -days 365 \
  -in server.csr \
  -signkey server.key \
  -out server.crt

# 生成完整证书链
cat server.crt > fullchain.pem
cat server.key >> fullchain.pem

# 设置权限
chmod 600 server.key
chmod 644 server.crt fullchain.pem
```

#### Nginx SSL 配置

```nginx
# /opt/synapse-rust/docker/nginx/nginx.conf 中的 SSL 配置片段

ssl_certificate /etc/nginx/ssl/server.crt;
ssl_certificate_key /etc/nginx/ssl/server.key;
ssl_trusted_certificate /etc/nginx/ssl/fullchain.pem;

# SSL 协议
ssl_protocols TLSv1.2 TLSv1.3;

# 加密套件
ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
ssl_prefer_server_ciphers off;

# SSL 会话
ssl_session_cache shared:SSL:10m;
ssl_session_timeout 1d;
ssl_session_tickets off;

# OCSP Stapling
ssl_stapling on;
ssl_stapling_verify on;
resolver 8.8.8.8 8.8.4.4 valid=300s;

# HSTS
add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
```

---

## 服务发现配置

### Matrix Server Discovery

#### /.well-known/matrix/server

```json
{
  "m.server": "matrix.cjystx.top:443",
  "m.identity_server": "https://vector.im"
}
```

#### /.well-known/matrix/client

```json
{
  "m.identity_server": "https://vector.im"
}
```

### DNS SRV 记录 (可选)

对于完全兼容的 Federation，建议添加 DNS SRV 记录：

```dns
# _matrix._tcp.cjystx.top
_matrix._tcp.cjystx.top. 3600 IN SRV 10 0 8448 matrix.cjystx.top.
```

### 验证服务发现

```bash
# 1. HTTP 检测
curl https://cjystx.top/.well-known/matrix/server
# 预期: {"m.server":"matrix.cjystx.top:443"}

# 2. DNS 检测
dig +short SRV _matrix._tcp.cjystx.top
# 预期: 10 0 8448 matrix.cjystx.top.

# 3. Federation 连接测试
curl https://matrix.cjystx.top/_matrix/federation/v1/version
# 预期: {"version":"synapse-rust"}
```

---

## 监控和维护

### 1. 健康检查

```bash
# 创建健康检查脚本
cat > /opt/synapse-rust/docker/scripts/healthcheck.sh << 'EOF'
#!/bin/bash

# 检查所有服务健康状态

PASS=0
FAIL=0

check_service() {
    local name=$1
    local url=$2
    
    if curl -sf "$url" > /dev/null 2>&1; then
        echo "✓ $name: OK"
        ((PASS++))
    else
        echo "✗ $name: FAIL"
        ((FAIL++))
    fi
}

echo "=== Synapse Rust Health Check ==="
echo ""

check_service "Client API" "http://localhost:8008/_matrix/client/versions"
check_service "Federation API" "http://localhost:8008/_matrix/federation/v1/version"
check_service "Database" "docker exec synapse-postgres pg_isready -U synapse"
check_service "Redis" "docker exec synapse-redis redis-cli ping"
check_service "Nginx" "curl -sf http://localhost/health"

echo ""
echo "=== Summary ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"

if [ $FAIL -gt 0 ]; then
    exit 1
fi
exit 0
EOF

chmod +x /opt/synapse-rust/docker/scripts/healthcheck.sh
```

### 2. 日志管理

```bash
# 配置日志轮转
cat > /opt/synapse-rust/docker/config/logrotate.conf << 'EOF'
/var/log/nginx/*.log {
    daily
    rotate 14
    compress
    delaycompress
    notifempty
    create 0640 www-data adm
    sharedscripts
    postrotate
        [ -f /var/run/nginx.pid ] && kill -USR1 `cat /var/run/nginx.pid`
    endscript
}

/app/logs/*.log {
    daily
    rotate 30
    compress
    delaycompress
    notifempty
    create 0640 synapse synapse
    sharedscripts
    postrotate
        [ -f /app/logs/synapse.pid ] && kill -USR1 `cat /app/logs/synapse.pid`
    endscript
}
EOF
```

### 3. 性能监控

```yaml
# docker-compose.yml 中的监控配置
services:
  synapse-rust:
    deploy:
      resources:
        limits:
          memory: 2G
          cpus: '2.0'
        reservations:
          memory: 1G
          cpus: '1.0'

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'
```

### 4. 备份策略

```bash
# 备份脚本
cat > /opt/synapse-rust/docker/scripts/backup.sh << 'EOF'
#!/bin/bash

BACKUP_DIR="/backup/synapse"
DATE=$(date +%Y%m%d_%H%M%S)
RETENTION=30

# 创建备份目录
mkdir -p "$BACKUP_DIR"

# 备份数据库
echo "Backing up PostgreSQL..."
docker exec synapse-postgres pg_dump -U synapse synapse_test | \
    gzip > "$BACKUP_DIR/postgresql_$DATE.sql.gz"

# 备份 Redis
echo "Backing up Redis..."
docker exec synapse-redis redis-cli BGSAVE
sleep 5
docker exec synapse-redis redis-cli LASTSAVE
docker exec synapse-redis redis-cli --rdb /tmp/redis.rdb
docker cp synapse-redis:/tmp/redis.rdb "$BACKUP_DIR/redis_$DATE.rdb"
docker exec synapse-redis rm /tmp/redis.rdb

# 清理旧备份
echo "Cleaning up old backups..."
find "$BACKUP_DIR" -name "*.gz" -mtime +$RETENTION -delete
find "$BACKUP_DIR" -name "*.rdb" -mtime +$RETENTION -delete

echo "Backup completed: $BACKUP_DIR"
EOF

chmod +x /opt/synapse-rust/docker/scripts/backup.sh

# 添加 cron 任务
# 0 3 * * * /opt/synapse-rust/docker/scripts/backup.sh
```

---

## 故障排查

### 常见问题

#### 1. 数据库连接失败

```bash
# 症状
# Error: could not connect to database

# 排查步骤
docker logs synapse-postgres
docker exec synapse-postgres pg_isready -U synapse
docker exec synapse-postgres psql -U synapse -c "SELECT 1"

# 解决方案
docker compose restart db
```

#### 2. 服务发现失败

```bash
# 症状
# Client cannot find the server

# 排查步骤
curl https://cjystx.top/.well-known/matrix/server
nslookup cjystx.top
dig +short SRV _matrix._tcp.cjystx.top

# 解决方案
# 1. 检查 DNS 配置
# 2. 检查 Nginx 配置
# 3. 重启 Nginx
docker compose restart nginx
```

#### 3. SSL 证书错误

```bash
# 症状
# SSL handshake failed

# 排查步骤
openssl s_client -connect matrix.cjystx.top:443
curl -v https://matrix.cjystx.top/_matrix/federation/v1/version

# 解决方案
# 1. 检查证书文件路径
# 2. 续期 Let's Encrypt 证书
docker exec synapse-nginx certbot renew --quiet
docker compose restart nginx
```

#### 4. 内存不足

```bash
# 症状
# OOM (Out of Memory) errors

# 排查步骤
docker stats
free -h
htop

# 解决方案
# 1. 增加容器内存限制
# 2. 优化 PostgreSQL 配置
# 3. 增加交换空间
```

### 日志查看

```bash
# 应用日志
docker compose logs -f synapse-rust

# 数据库日志
docker compose logs -f db

# Nginx 日志
docker exec synapse-nginx tail -f /var/log/nginx/synapse_access.log
docker exec synapse-nginx tail -f /var/log/nginx/error.log
```

### 重启策略

```bash
# 优雅重启
docker compose restart synapse-rust

# 强制重启（先停止再启动）
docker compose down
docker compose up -d

# 完全重置（包括数据卷）- ⚠️ 会丢失数据
docker compose down -v
docker compose up -d
```

---

## 联系和贡献

### 反馈问题

如遇到问题，请提供以下信息：

```bash
# 1. Docker 日志
docker compose logs --tail=100 synapse-rust > synapse_logs.txt

# 2. 系统信息
uname -a
docker version
docker compose version

# 3. 配置信息（脱敏后）
cat docker-compose.yml
cat config/homeserver.yaml
```

### 文档更新

欢迎改进本部署文档！请提交 Pull Request。

---

**最后更新**: 2024-02-06  
**版本**: 1.0.0
