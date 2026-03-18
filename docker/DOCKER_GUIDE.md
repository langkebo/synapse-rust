# Docker 配置指南

> Synapse Rust Matrix 服务器 Docker 配置

## 目录结构

```
docker/
├── config/
│   ├── .env.example          # 环境变量模板
│   ├── .env                 # 本地环境配置
│   ├── homeserver.yaml      # Matrix 服务器配置
│   └── postgres.conf        # PostgreSQL 配置
├── data/                    # 数据目录
├── logs/                    # 日志目录
├── media/                   # 媒体文件
├── nginx/                   # Nginx 配置
│   ├── nginx.conf
│   └── .well-known/
├── ssl/                     # SSL 证书
├── db_migrate.sh            # 数据库迁移脚本
├── docker-compose.yml      # 生产环境配置
├── docker-compose.local.yml # 本地开发配置
├── docker-compose.dev.yml   # 开发环境配置
├── docker-entrypoint.sh     # 容器入口脚本
├── healthcheck.sh          # 健康检查脚本
├── verify_migration.sh     # 迁移验证脚本
└── README.md              # 本文档
```

## 快速开始

### 本地开发

```bash
# 1. 进入 docker 目录
cd docker

# 2. 复制环境变量模板
cp config/.env.example .env

# 3. 编辑 .env 配置
nano .env

# 4. 启动服务
docker compose -f docker-compose.local.yml up -d

# 5. 查看日志
docker compose -f docker-compose.local.yml logs -f
```

### 生产部署

```bash
# 1. 进入 docker 目录
cd docker

# 2. 复制环境变量模板
cp config/.env.example .env

# 3. 编辑 .env 配置 (必须修改密码)
nano .env

# 4. 构建镜像
docker build -f Dockerfile -t synapse-rust:latest ..

# 5. 启动服务
docker compose up -d
```

## 环境变量

### 必需变量

| 变量 | 说明 | 示例 |
|------|------|------|
| SERVER_NAME | 服务器域名 | cjystx.top |
| DB_PASSWORD | 数据库密码 | secure_password |
| SECRET_KEY | JWT 密钥 | (至少32字符) |
| MACAROON_SECRET | Macaroon 密钥 | (至少32字符) |
| FORM_SECRET | 表单密钥 | (至少32字符) |

### 可选变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| RUST_LOG | info | 日志级别 |
| DB_POOL_SIZE | 20 | 数据库连接池大小 |
| REDIS_ENABLED | true | 启用 Redis |
| RUN_MIGRATIONS | true | 自动执行迁移 |
| ALLOWED_ORIGINS | * | CORS 允许的来源 |

## 端口说明

| 端口 | 协议 | 说明 |
|------|------|------|
| 8008 | HTTP | Matrix Client API |
| 8448 | HTTP | Matrix Federation API |
| 9090 | HTTP | Prometheus 指标 |
| 5432 | TCP | PostgreSQL (本地) |
| 6379 | TCP | Redis (本地) |

## 常用命令

```bash
# 启动所有服务
docker compose up -d

# 停止所有服务
docker compose down

# 查看服务状态
docker compose ps

# 查看日志
docker compose logs -f synapse-rust

# 进入容器
docker compose exec synapse-rust bash

# 执行迁移
docker compose exec synapse-rust bash /app/scripts/run-migrations.sh

# 重启服务
docker compose restart synapse-rust

# 重建镜像
docker compose build --no-cache synapse-rust
```

## 健康检查

服务健康检查端点：

- **Synapse Rust**: `http://localhost:8008/_matrix/federation/v1/version`
- **PostgreSQL**: `pg_isready`
- **Redis**: `redis-cli ping`

## 数据持久化

数据存储在 Docker 卷中：

- `postgres_local_data` - PostgreSQL 数据
- `redis_local_data` - Redis 数据
- `synapse_data` - 应用数据
- `synapse_logs` - 日志文件

## SSL 证书

生产环境需要配置 SSL 证书：

```bash
# 生成自签名证书 (测试用)
openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout ssl/server.key -out ssl/server.crt
```

或使用 Let's Encrypt：

```bash
# 使用 certbot
certbot certonly --webroot -w /var/www/.well-known \
  -d matrix.cjystx.top
```

## 故障排除

### 数据库连接失败

```bash
# 检查数据库状态
docker compose logs db

# 测试连接
docker compose exec db psql -U synapse -d synapse
```

### 迁移失败

```bash
# 查看迁移日志
docker compose logs synapse-rust | grep -i migration

# 手动执行迁移
docker compose exec synapse-rust bash /app/scripts/run-migrations.sh
```

### 端口冲突

```bash
# 检查端口占用
lsof -i :8008

# 修改端口
# 编辑 docker-compose.local.yml 中的 ports 配置
```

## 性能优化

### PostgreSQL

```bash
# 调整 shared_buffers
POSTGRES_SHARED_BUFFERS=512MB
```

### Redis

```bash
# 调整最大内存
REDIS_MAX_MEMORY=512mb
REDIS_EVICTION_POLICY=allkeys-lru
```

### Synapse Rust

```bash
# 调整内存限制
SYNAPSE_MEMORY_LIMIT=4G
```

## 安全建议

1. **修改默认密码**: 一定要修改 `DB_PASSWORD` 和其他密钥
2. **限制 CORS**: 生产环境不要使用 `*`
3. **使用 SSL**: 启用 HTTPS
4. **定期备份**: 备份数据库卷
5. **监控日志**: 定期检查日志

## 参考链接

- [Matrix 规范](https://spec.matrix.org/)
- [Synapse Python (原项目)](https://github.com/element-hq/synapse)
- [Docker 文档](https://docs.docker.com/)
- [PostgreSQL 文档](https://www.postgresql.org/docs/)
