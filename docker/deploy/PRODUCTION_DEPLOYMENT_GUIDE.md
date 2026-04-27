# 生产环境部署指南

> **版本**: v2.0.0  
> **更新日期**: 2026-04-27  
> **状态**: ✅ 生产就绪

---

## 目录

1. [部署前检查](#部署前检查)
2. [快速部署](#快速部署)
3. [生产环境优化](#生产环境优化)
4. [安全加固](#安全加固)
5. [性能调优](#性能调优)
6. [监控告警](#监控告警)
7. [备份恢复](#备份恢复)
8. [故障排查](#故障排查)

---

## 部署前检查

### 1. 系统要求

| 项目 | 最低配置 | 推荐配置 | 说明 |
|------|---------|---------|------|
| CPU | 2核 | 4核+ | 支持 amd64 架构 |
| 内存 | 4GB | 8GB+ | 建议 16GB 用于大规模部署 |
| 磁盘 | 50GB | 200GB+ SSD | 媒体文件会快速增长 |
| 网络 | 100Mbps | 1Gbps | 联邦通信需要稳定带宽 |

### 2. 软件依赖

```bash
# Docker 20.10+
docker --version

# Docker Compose 2.0+
docker compose version

# 检查架构
uname -m  # 应输出 x86_64
```

### 3. 网络端口

| 端口 | 协议 | 用途 | 必需 |
|------|------|------|------|
| 80 | HTTP | Web 访问 | 是 |
| 443 | HTTPS | 安全访问 | 是 |
| 8448 | HTTPS | Matrix 联邦 | 是（如启用联邦）|
| 9090 | HTTP | Prometheus 指标 | 否 |

---

## 快速部署

### 步骤 1: 准备环境

```bash
# 克隆或进入项目目录
cd /path/to/synapse-rust/docker/deploy

# 复制环境变量模板
cp .env.example .env

# 编辑配置（必须修改密码和域名）
vim .env
```

### 步骤 2: 配置环境变量

**必须修改的配置**:

```bash
# 服务器配置
SERVER_NAME=matrix.example.com
PUBLIC_BASEURL=https://matrix.example.com

# 数据库密码（强密码）
POSTGRES_PASSWORD=<生成的强密码>

# Redis 密码（强密码）
REDIS_PASSWORD=<生成的强密码>

# 密钥（使用 openssl rand -hex 32 生成）
ADMIN_SHARED_SECRET=<生成的密钥>
JWT_SECRET=<生成的密钥>
REGISTRATION_SHARED_SECRET=<生成的密钥>
SECRET_KEY=<生成的密钥>
MACAROON_SECRET=<生成的密钥>
FORM_SECRET=<生成的密钥>
```

**生成密钥脚本**:

```bash
# 自动生成所有密钥
./scripts/generate-secrets.sh
```

### 步骤 3: 部署服务

```bash
# 一键部署
./deploy.sh

# 或手动部署
docker compose up -d
```

### 步骤 4: 验证部署

```bash
# 检查服务状态
docker compose ps

# 查看日志
docker compose logs -f synapse

# 健康检查
curl http://localhost:8008/_matrix/client/versions
```

---

## 生产环境优化

### 1. Docker Compose 配置优化

**已优化项**:

- ✅ 资源限制（CPU、内存、PID）
- ✅ 健康检查（所有服务）
- ✅ 日志轮转（防止磁盘占满）
- ✅ 安全加固（只读文件系统、权限最小化）
- ✅ 网络隔离（独立网络）
- ✅ 数据持久化（命名卷）

**资源限制配置**:

```yaml
# .env 文件中配置
SYNAPSE_CPU_LIMIT=2.0
SYNAPSE_MEMORY_LIMIT=2048m
POSTGRES_CPU_LIMIT=1.5
POSTGRES_MEMORY_LIMIT=1536m
REDIS_CPU_LIMIT=1.0
REDIS_MEMORY_LIMIT=512m
```

### 2. PostgreSQL 优化

**配置文件**: `config/postgres.conf`

```ini
# 连接配置
max_connections = 200
shared_buffers = 512MB
effective_cache_size = 2GB
maintenance_work_mem = 128MB
work_mem = 8MB

# WAL 配置
wal_buffers = 16MB
checkpoint_completion_target = 0.9
max_wal_size = 2GB
min_wal_size = 1GB

# 查询优化
random_page_cost = 1.1  # SSD
effective_io_concurrency = 200
```

### 3. Redis 优化

**已配置**:

- ✅ AOF 持久化
- ✅ 内存淘汰策略（allkeys-lru）
- ✅ 定期快照（RDB）
- ✅ 最大内存限制

---

## 安全加固

### 1. 容器安全

**已实施**:

- ✅ 只读文件系统（read_only: true）
- ✅ 权限最小化（cap_drop: ALL）
- ✅ 禁止特权提升（no-new-privileges）
- ✅ PID 限制（防止 fork 炸弹）
- ✅ 非 root 用户运行

### 2. 网络安全

```bash
# 配置防火墙（UFW 示例）
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw allow 8448/tcp
sudo ufw enable

# 限制 Prometheus 端口访问（仅本地）
# 在 docker-compose.yml 中已配置为不对外暴露
```

### 3. SSL/TLS 配置

**使用 Let's Encrypt**:

```bash
# 安装 certbot
sudo apt install certbot

# 获取证书
sudo certbot certonly --standalone -d matrix.example.com

# 复制证书到 SSL 目录
sudo cp /etc/letsencrypt/live/matrix.example.com/fullchain.pem ./ssl/
sudo cp /etc/letsencrypt/live/matrix.example.com/privkey.pem ./ssl/

# 配置自动续期
sudo crontab -e
# 添加: 0 3 * * * certbot renew --quiet && docker compose restart nginx
```

### 4. 密钥管理

**最佳实践**:

- ✅ 使用强随机密钥（32字节）
- ✅ 定期轮换密钥
- ✅ 不提交 .env 到版本控制
- ✅ 使用密钥管理服务（生产环境）

---

## 性能调优

### 1. 数据库连接池

```bash
# .env 配置
SYNAPSE__DATABASE__POOL_SIZE=20
SYNAPSE__DATABASE__MAX_SIZE=50
SYNAPSE__DATABASE__MIN_IDLE=10
SYNAPSE__DATABASE__CONNECTION_TIMEOUT=60
```

### 2. Redis 缓存

```bash
# .env 配置
REDIS_MAX_MEMORY=256mb
REDIS_EVICTION_POLICY=allkeys-lru
```

### 3. 日志级别

```bash
# 生产环境使用 info 或 warn
RUST_LOG=info

# 调试时使用 debug
RUST_LOG=debug
```

### 4. 媒体存储优化

```bash
# 定期清理旧媒体文件
docker compose exec synapse /app/scripts/cleanup-media.sh

# 配置媒体保留策略（homeserver.yaml）
media_retention:
  max_age: 90d  # 保留 90 天
```

---

## 监控告警

### 1. Prometheus 指标

**访问地址**: `http://localhost:9090/metrics`

**关键指标**:

- `synapse_http_requests_total` - HTTP 请求总数
- `synapse_database_pool_connections` - 数据库连接数
- `synapse_cache_hit_rate` - 缓存命中率
- `synapse_federation_requests_total` - 联邦请求数

### 2. 健康检查

```bash
# Synapse 健康检查
curl http://localhost:8008/_matrix/client/versions

# 数据库健康检查
docker compose exec postgres pg_isready

# Redis 健康检查
docker compose exec redis redis-cli ping
```

### 3. 日志监控

```bash
# 实时查看日志
docker compose logs -f synapse

# 查看错误日志
docker compose logs synapse | grep ERROR

# 导出日志
docker compose logs synapse > synapse.log
```

---

## 备份恢复

### 1. 数据库备份

```bash
# 手动备份
./scripts/backup.sh

# 自动备份（crontab）
0 2 * * * cd /path/to/deploy && ./scripts/backup.sh
```

**备份内容**:

- PostgreSQL 数据库
- Redis 数据
- 媒体文件
- 配置文件

### 2. 恢复数据

```bash
# 恢复数据库
./scripts/restore.sh /path/to/backup.sql

# 恢复媒体文件
tar -xzf media-backup.tar.gz -C ./media/
```

### 3. 灾难恢复

```bash
# 1. 停止服务
docker compose down

# 2. 恢复数据
./scripts/restore.sh /path/to/backup.sql

# 3. 启动服务
docker compose up -d

# 4. 验证
docker compose ps
```

---

## 故障排查

### 1. 服务无法启动

```bash
# 查看日志
docker compose logs synapse

# 检查配置
docker compose config

# 验证环境变量
docker compose exec synapse env | grep SYNAPSE
```

### 2. 数据库连接失败

```bash
# 检查数据库状态
docker compose exec postgres pg_isready

# 测试连接
docker compose exec synapse psql $DATABASE_URL -c "SELECT 1"

# 查看连接数
docker compose exec postgres psql -U postgres -d synapse -c "SELECT count(*) FROM pg_stat_activity"
```

### 3. 性能问题

```bash
# 查看资源使用
docker stats

# 查看慢查询
docker compose exec postgres psql -U postgres -d synapse -c "SELECT * FROM pg_stat_statements ORDER BY total_time DESC LIMIT 10"

# 查看缓存命中率
curl http://localhost:9090/metrics | grep cache_hit
```

### 4. 联邦问题

```bash
# 测试联邦连接
curl https://matrix.org:8448/_matrix/federation/v1/version

# 查看联邦日志
docker compose logs synapse | grep federation
```

---

## 维护任务

### 日常维护

```bash
# 每日检查
- 查看服务状态: docker compose ps
- 查看磁盘使用: df -h
- 查看日志错误: docker compose logs synapse | grep ERROR

# 每周维护
- 清理旧日志: docker system prune -f
- 备份数据库: ./scripts/backup.sh
- 更新镜像: docker compose pull

# 每月维护
- 清理媒体文件: ./scripts/cleanup-media.sh
- 数据库优化: docker compose exec postgres vacuumdb -U postgres -d synapse -z
- 安全更新: apt update && apt upgrade
```

---

## 升级指南

### 1. 升级前准备

```bash
# 备份数据
./scripts/backup.sh

# 记录当前版本
docker compose exec synapse /app/synapse-rust --version
```

### 2. 升级步骤

```bash
# 1. 拉取新镜像
docker pull vmuser232922/mysynapse:latest

# 2. 停止服务
docker compose down

# 3. 运行迁移
docker compose up migrator

# 4. 启动服务
docker compose up -d

# 5. 验证
docker compose ps
docker compose logs -f synapse
```

### 3. 回滚

```bash
# 1. 停止服务
docker compose down

# 2. 恢复备份
./scripts/restore.sh /path/to/backup.sql

# 3. 使用旧镜像
docker compose up -d
```

---

## 附录

### A. 环境变量完整列表

参考 `.env.example` 文件

### B. 端口映射

| 容器端口 | 主机端口 | 说明 |
|---------|---------|------|
| 8008 | 8008 | Synapse HTTP |
| 9090 | 9090 | Prometheus |
| 80 | 80 | Nginx HTTP |
| 443 | 443 | Nginx HTTPS |
| 8448 | 8448 | Federation |

### C. 数据卷

| 卷名 | 挂载点 | 说明 |
|------|--------|------|
| postgres_data | /var/lib/postgresql/data | 数据库数据 |
| redis_data | /data | Redis 数据 |
| synapse_data | /app/data | Synapse 数据 |
| nginx_logs | /var/log/nginx | Nginx 日志 |

---

**文档版本**: v2.0.0  
**最后更新**: 2026-04-27  
**维护者**: synapse-rust team
