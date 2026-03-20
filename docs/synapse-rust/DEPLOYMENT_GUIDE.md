# synapse-rust 部署指南

> 生产环境部署文档

## 目录

1. [环境要求](#环境要求)
2. [快速开始](#快速开始)
3. [配置详解](#配置详解)
4. [数据库设置](#数据库设置)
5. [反向代理配置](#反向代理配置)
6. [Docker 部署](#docker-部署)
7. [监控与日志](#监控与日志)
8. [性能优化](#性能优化)
9. [安全加固](#安全加固)
10. [故障排除](#故障排除)

---

## 环境要求

### 硬件要求

| 配置 | 最低 | 推荐 |
|------|------|------|
| CPU | 2 核 | 4+ 核 |
| 内存 | 4 GB | 8+ GB |
| 磁盘 | 50 GB | 100+ GB |
| 网络 | 100 Mbps | 1 Gbps |

### 软件要求

- **操作系统**: Linux (Ubuntu 20.04+, Debian 11+), macOS
- **数据库**: PostgreSQL 13+
- **Rust**: 1.70+
- **依赖**: OpenSSL, pkg-config

---

## 快速开始

### 1. 克隆项目

```bash
git clone https://github.com/hula-team/synapse-rust.git
cd synapse-rust
```

### 2. 编译项目

```bash
# 开发模式
cargo build

# 生产模式
cargo build --release
```

### 3. 配置数据库

```bash
# 创建数据库
createdb synapse

# 运行数据库迁移
cargo run --bin migrate
```

### 4. 配置环境变量

```bash
# 复制示例配置
cp config.example.yaml config.yaml

# 编辑配置
nano config.yaml
```

### 5. 启动服务

```bash
# 开发模式
cargo run

# 生产模式
./target/release/synapse-rust -c config.yaml
```

---

## 配置详解

### 基础配置 (config.yaml)

```yaml
server:
  name: matrix.example.com
  port: 8008
  bind_address: "0.0.0.0"
  public_baseurl: https://matrix.example.com
  
database:
  host: localhost
  port: 5432
  name: synapse
  username: synapse
  password: your_password
  pool_size: 20
  
logging:
  level: info
  format: json
  
cache:
  enabled: true
  redis_url: redis://localhost:6379
  
security:
  secret: your-very-long-secret-key
  trusted_hosts:
    - matrix.example.com
```

### 高级配置

#### 认证配置

```yaml
auth:
  # 密码哈希算法
  password_hash: argon2
  
  # JWT 配置
  jwt:
    enabled: true
    secret: jwt-secret
    algorithm: HS256
    
  # OIDC 配置
  oidc:
    enabled: false
    providers: []
    
  # SAML 配置
  saml:
    enabled: false
```

#### 性能配置

```yaml
performance:
  # 连接池
  db_pool_size: 20
  db_max_overflow: 10
  
  # 缓存
  cache:
    enabled: true
    default_ttl: 3600
    
  # 限流
  rate_limit:
    enabled: true
    window_ms: 1000
    max_requests: 50
```

---

## 数据库设置

### PostgreSQL 配置

```bash
# postgresql.conf 优化
shared_buffers = 256MB
effective_cache_size = 1GB
maintenance_work_mem = 64MB
checkpoint_completion_target = 0.9
wal_buffers = 16MB
default_statistics_target = 100
random_page_cost = 1.1
effective_io_concurrency = 200
work_mem = 4MB
min_wal_size = 1GB
max_wal_size = 4GB
```

### 数据库用户创建

```sql
CREATE USER synapse WITH PASSWORD 'your_password';
CREATE DATABASE synapse OWNER synapse;
GRANT ALL PRIVILEGES ON DATABASE synapse TO synapse;
```

---

## 反向代理配置

### Nginx 配置

```nginx
server {
    listen 443 ssl http2;
    server_name matrix.example.com;
    
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    
    client_max_body_size 50M;
    
    location / {
        proxy_pass http://127.0.0.1:8008;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # WebSocket 支持
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}

# 联邦端点
server {
    listen 8448 ssl http2;
    server_name matrix.example.com;
    
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    
    location / {
        proxy_pass http://127.0.0.1:8008;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

### Caddy 配置

```
matrix.example.com {
    reverse_proxy localhost:8008
    
    # WebSocket 支持
    websocket {
        header_upstream Connection "Upgrade"
        header_upstream Upgrade $http_upgrade
    }
}
```

---

## Docker 部署

### Dockerfile

```dockerfile
FROM rust:1.70 as builder

WORKDIR /build
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/synapse-rust /app/
COPY config.example.yaml /app/config.yaml

EXPOSE 8008 8448

CMD ["/app/synapse-rust", "-c", "/app/config.yaml"]
```

### docker-compose.yml

```yaml
version: '3.8'

services:
  synapse:
    build: .
    ports:
      - "8008:8008"
      - "8448:8448"
    volumes:
      - ./config.yaml:/app/config.yaml
      - ./data:/app/data
    environment:
      - RUST_LOG=info
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8008/_matrix/client/versions"]
      interval: 30s
      timeout: 10s
      retries: 3

  postgres:
    image: postgres:15
    environment:
      POSTGRES_USER: synapse
      POSTGRES_PASSWORD: password
      POSTGRES_DB: synapse
    volumes:
      - postgres_data:/var/lib/postgresql/data
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    restart: unless-stopped

volumes:
  postgres_data:
```

---

## 监控与日志

### 日志配置

```yaml
logging:
  # 日志级别: trace, debug, info, warn, error
  level: info
  
  # 日志格式: text, json
  format: json
  
  # 日志输出
  outputs:
    - type: stdout
    - type: file
      path: /var/log/synapse.log
```

### Prometheus 指标

```yaml
telemetry:
  enabled: true
  port: 9090
  path: /_matrix/telemetry/v1/metrics
```

### 关键指标

| 指标 | 说明 |
|------|------|
| synapse_requests_total | 总请求数 |
| synapse_request_duration_seconds | 请求延迟 |
| synapse_db_query_duration_seconds | 数据库查询延迟 |
| synapse_cache_hits_total | 缓存命中 |
| synapse_cache_misses_total | 缓存未命中 |
| synapse_active_connections | 活跃连接数 |

---

## 性能优化

### 1. 数据库优化

- 使用连接池
- 添加适当索引
- 定期 VACUUM
- 使用 Prepared Statements

### 2. 缓存优化

```yaml
cache:
  enabled: true
  redis_url: redis://localhost:6379
  default_ttl: 3600
  max_entries: 10000
```

### 3. 限流配置

```yaml
rate_limit:
  enabled: true
  window_ms: 1000
  max_requests: 50
  
  # 管理员例外
  admin_bypass:
    enabled: true
```

### 4. Gzip 压缩

```nginx
gzip on;
gzip_types text/plain text/css application/json application/javascript;
gzip_min_length 1000;
```

---

## 安全加固

### 1. TLS 配置

```nginx
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers HIGH:!aNULL:!MD5;
ssl_prefer_server_ciphers on;
```

### 2. 防火墙

```bash
# 只开放必要端口
ufw allow 22/tcp   # SSH
ufw allow 80/tcp   # HTTP
ufw allow 443/tcp # HTTPS
ufw allow 8448/tcp # Federation
```

### 3. 安全 headers

```nginx
add_header X-Frame-Options DENY;
add_header X-Content-Type-Options nosniff;
add_header X-XSS-Protection "1; mode=block";
add_header Referrer-Policy "no-referrer";
```

### 4. 注册限制

```yaml
registration:
  enabled: false  # 关闭公共注册
  require_invite: true  # 需要邀请
```

---

## 故障排除

### 常见问题

#### 1. 数据库连接失败

```bash
# 检查 PostgreSQL 状态
systemctl status postgresql

# 检查连接
psql -h localhost -U synapse -d synapse
```

#### 2. 内存使用过高

```bash
# 查看内存使用
top -p $(pidof synapse-rust)

# 调整连接池
# config.yaml 中减小 db_pool_size
```

#### 3. 性能问题

```bash
# 查看慢查询日志
# PostgreSQL: 设置 log_min_duration_statement = 1000

# 检查缓存命中率
curl http://localhost:9090/_matrix/telemetry/v1/metrics
```

#### 4. 联邦同步问题

```bash
# 检查联邦端口
nc -zv matrix.example.com 8448

# 查看联邦日志
tail -f /var/log/synapse.log | grep federation
```

### 日志位置

| 类型 | 位置 |
|------|------|
| 应用日志 | `/var/log/synapse.log` |
| 错误日志 | `/var/log/synapse_error.log` |
| 访问日志 | `/var/log/synapse_access.log` |

---

## 维护

### 备份

```bash
# 数据库备份
pg_dump -h localhost -U synapse synapse > backup.sql

# 配置文件备份
cp config.yaml config.yaml.backup
```

### 更新

```bash
# 拉取更新
git pull

# 重新编译
cargo build --release

# 重启服务
systemctl restart synapse-rust
```

### 清理

```bash
# 清理旧媒体文件
cargo run --bin cleanup-media -- --older-than 90d

# 清理历史数据
# 使用 Admin API: POST /_synapse/admin/v1/purge_history
```

---

*文档版本: 1.0*
*最后更新: 2026-03-19*
*支持: https://github.com/hula-team/synapse-rust/issues*
