# synapse-rust Docker 部署指南

> **版本**: v1.1.0  
> **更新日期**: 2026-04-13  
> **Docker 镜像**: `vmuser232922/mysynapse:latest`

---

## 目录

1. [概述](#概述)
2. [前置要求](#前置要求)
3. [快速部署](#快速部署)
4. [配置说明](#配置说明)
5. [部署步骤详解](#部署步骤详解)
6. [服务管理](#服务管理)
7. [SSL/TLS 配置](#ssltls-配置)
8. [备份与恢复](#备份与恢复)
9. [常见问题](#常见问题)
10. [故障排查](#故障排查)

---

## 概述

本部署方案使用 Docker Compose 编排以下服务：

| 服务 | 镜像 | 说明 |
|------|------|------|
| synapse | vmuser232922/mysynapse:latest | Matrix 主服务器 |
| postgres | postgres:16-alpine | PostgreSQL 数据库 |
| redis | redis:7-alpine | Redis 缓存 |
| nginx | nginx:alpine | 反向代理 |
| migrator | postgres:16-alpine | 基于 `psql` 的一次性迁移服务 |

### 目录结构

```text
docker/deploy/
├── docker-compose.yml      # Docker Compose 配置
├── .env.example            # 环境变量模板
├── deploy.sh               # 一键部署脚本
├── README.md               # 本文档
├── config/
│   └── homeserver.yaml     # Synapse 配置文件
├── nginx/
│   ├── nginx.conf          # Nginx 主配置
│   └── conf.d/
│       ├── default.conf    # HTTP 配置
│       ├── federation.conf # 联邦端口配置
│       └── ssl.conf.example # HTTPS 配置模板
├── scripts/
│   ├── init-db.sql         # 数据库初始化脚本
│   ├── generate-secrets.sh # 密钥自动生成脚本
│   ├── migrate.sh          # 迁移脚本
│   ├── backup.sh           # 备份脚本
│   └── restore.sh          # 恢复脚本
├── ssl/                    # SSL 证书目录
└── migrations/             # 数据库迁移文件目录
```

---

## 前置要求

### 系统要求

| 项目 | 最低要求 | 推荐配置 |
|------|---------|---------|
| 操作系统 | Linux (Ubuntu 20.04+) | Ubuntu 22.04 LTS |
| CPU | 2 核 | 4 核+ |
| 内存 | 4 GB | 8 GB+ |
| 磁盘 | 20 GB | 100 GB+ SSD |

### 软件依赖

| 软件 | 版本要求 | 安装命令 |
|------|---------|---------|
| Docker | 20.10+ | `curl -fsSL https://get.docker.com \| sh` |
| Docker Compose | 2.0+ | `sudo apt install docker-compose-plugin` |

### 网络要求

- 开放端口: 80 (HTTP), 443 (HTTPS), 8448 (Matrix Federation)
- 确保 DNS 解析正确指向服务器
- 联邦端口 8448 必须可从公网访问（用于服务器间通信）

### 检查依赖

```bash
# 检查 Docker 版本
docker --version

# 检查 Docker Compose 版本
docker compose version
```

---

## 快速部署

### 一键部署

```bash
# 1. 进入部署目录
cd docker/deploy

# 2. 复制环境变量模板
cp .env.example .env

# 3. 编辑 .env 文件，至少修改以下两项
nano .env
# SERVER_NAME=matrix.example.com
# PUBLIC_BASEURL=https://matrix.example.com
# 可按需调整:
# SYNAPSE__DATABASE__POOL_SIZE=20
# SYNAPSE__DATABASE__MAX_SIZE=50

# 4. 运行部署脚本 (自动生成安全密钥)
chmod +x deploy.sh
./deploy.sh
```

### 自动密钥生成

部署脚本会自动生成以下安全密钥：

| 密钥 | 说明 | 生成方式 |
|------|------|---------|
| `POSTGRES_PASSWORD` | 数据库密码 | 32位随机密码 |
| `REDIS_PASSWORD` | Redis 密码 | 32位随机密码 |
| `ADMIN_SHARED_SECRET` | 管理员密钥 | 64位十六进制 |
| `JWT_SECRET` | JWT 签名密钥 | Base64 编码 |
| `REGISTRATION_SHARED_SECRET` | 注册密钥 | 64位十六进制 |

手动生成密钥：

```bash
# 生成所有密钥
./scripts/generate-secrets.sh all

# 生成单个密钥
./scripts/generate-secrets.sh jwt
```

### 最小配置

编辑 `.env` 文件，只需修改以下配置：

```bash
# 服务器名称 (必须修改)
SERVER_NAME=matrix.example.com

# 公开访问 URL (必须修改)
PUBLIC_BASEURL=https://matrix.example.com

# 以下密钥由 deploy.sh 自动生成，无需手动配置
# POSTGRES_PASSWORD=...
# REDIS_PASSWORD=...
# ADMIN_SHARED_SECRET=...
# JWT_SECRET=...
# REGISTRATION_SHARED_SECRET=...
```

---

## 配置说明

### 环境变量

| 变量名 | 说明 | 默认值 | 必填 |
|--------|------|--------|------|
| `SERVER_NAME` | Matrix 服务器名称 | localhost | ✅ |
| `PUBLIC_BASEURL` | 公开访问 URL | http://localhost:8008 | ✅ |
| `POSTGRES_USER` | 数据库用户名 | synapse | |
| `POSTGRES_PASSWORD` | 数据库密码 | (自动生成) | |
| `POSTGRES_DB` | 数据库名称 | synapse | |
| `SYNAPSE__DATABASE__POOL_SIZE` | 数据库连接池预热大小 | 20 | |
| `SYNAPSE__DATABASE__MAX_SIZE` | 数据库连接池最大连接数 | 50 | |
| `SYNAPSE__DATABASE__MIN_IDLE` | 数据库连接池最小空闲连接 | 10 | |
| `SYNAPSE__DATABASE__CONNECTION_TIMEOUT` | 数据库连接获取超时（秒） | 60 | |
| `REDIS_PASSWORD` | Redis 密码 | (自动生成) | |
| `REDIS_PORT` | Redis 端口 | 6379 | |
| `SYNAPSE_PORT` | Synapse 内部端口 | 8008 | |
| `FEDERATION_PORT` | Matrix 联邦端口 | 8448 | |
| `ADMIN_SHARED_SECRET` | 管理员注册密钥 | (自动生成) | |
| `JWT_SECRET` | JWT 签名密钥 | (自动生成) | |
| `REGISTRATION_SHARED_SECRET` | 用户注册密钥 | (自动生成) | |
| `RUST_LOG` | 日志级别 | info | |
| `HTTP_PORT` | HTTP 端口 | 80 | |
| `HTTPS_PORT` | HTTPS 端口 | 443 | |

### 密钥生成

密钥会在部署时自动生成，也可以手动生成：

```bash
# 生成所有密钥
./scripts/generate-secrets.sh all

# 生成单个密钥
./scripts/generate-secrets.sh jwt

# 或使用 openssl
openssl rand -hex 32
```

---

### 配置优先级

数据库连接池配置已统一为单一来源：

1. `.env` / `.env.example` 中的 `SYNAPSE__DATABASE__*`
2. `docker-compose.yml` 将这些变量透传到 `synapse` 容器
3. 应用通过 `SYNAPSE__` 前缀覆盖 `config/homeserver.yaml` 中的默认值

如果不显式设置，`homeserver.yaml` 中的默认值与 `.env.example` 保持一致，不会再出现 Compose 与 YAML 各写一套的漂移。

---

## 部署步骤详解

### 步骤 1: 准备部署文件

```bash
# 创建部署目录
mkdir -p /opt/synapse
cd /opt/synapse

# 复制部署文件
# 将 docker/deploy 目录下的所有文件复制到此目录
```

### 步骤 2: 配置环境变量

```bash
# 复制模板
cp .env.example .env

# 编辑配置
nano .env
```

### 步骤 3: 准备迁移文件

如果 Docker 镜像不包含迁移文件，需要手动准备：

```bash
# 从项目复制迁移文件
cp -r /path/to/synapse-rust/migrations ./migrations/
```

### 步骤 4: 启动服务

```bash
# 4. 启动部署脚本
./deploy.sh

# 或手动启动
docker compose up -d

# 如需从宿主机直连 PostgreSQL/Redis，显式叠加开发态 override
docker compose -f docker-compose.yml -f docker-compose.dev-host-access.yml up -d
```

### 步骤 5: 验证部署

```bash
# 检查服务状态
docker compose ps

# 检查健康状态
curl http://localhost:8008/health

# 检查 API 版本
curl http://localhost:8008/_matrix/client/versions
```

### 步骤 6: 注册管理员

```bash
# 使用共享密钥注册管理员
curl -X POST http://localhost:8008/_synapse/admin/v1/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "password": "AdminPassword123",
    "admin": true,
    "mac": "<computed_mac>"
  }'
```

---

## 服务管理

### 基本命令

```bash
# 启动所有服务
docker compose up -d

# 停止所有服务
docker compose down

# 重启服务
docker compose restart

# 查看服务状态
docker compose ps

# 查看日志
docker compose logs -f synapse

# 查看特定服务日志
docker compose logs -f postgres
```

### 单独管理服务

```bash
# 只重启 Synapse
docker compose restart synapse

# 只重启 Nginx
docker compose restart nginx

# 重新加载 Nginx 配置
docker compose exec nginx nginx -s reload
```

### 进入容器

```bash
# 进入 Synapse 容器
docker compose exec synapse /bin/sh

# 进入 PostgreSQL 容器
docker compose exec postgres /bin/sh

# 连接数据库
docker compose exec postgres psql -U synapse -d synapse
```

---

## SSL/TLS 配置

### 使用 Let's Encrypt

```bash
# 安装 certbot
sudo apt install certbot

# 获取证书
sudo certbot certonly --standalone -d matrix.example.com

# 复制证书
cp /etc/letsencrypt/live/matrix.example.com/fullchain.pem ssl/cert.pem
cp /etc/letsencrypt/live/matrix.example.com/privkey.pem ssl/key.pem
```

### 配置 HTTPS

```bash
# 启用 HTTPS 配置
cd nginx/conf.d
mv default.conf default.conf.bak
mv ssl.conf.example default.conf

# 重启 Nginx
docker compose restart nginx
```

### 自动续期

```bash
# 添加定时任务
crontab -e

# 添加以下行
0 0 1 * * certbot renew --quiet && cp /etc/letsencrypt/live/matrix.example.com/fullchain.pem /opt/synapse/ssl/cert.pem && cp /etc/letsencrypt/live/matrix.example.com/privkey.pem /opt/synapse/ssl/key.pem && docker compose -f /opt/synapse/docker-compose.yml restart nginx
```

---

## 备份与恢复

### 备份

```bash
# 运行备份脚本
./scripts/backup.sh

# 备份文件位置
ls -la backups/
```

### 恢复

```bash
# 恢复备份
./scripts/restore.sh backups/synapse_backup_20260402_120000.tar.gz
```

### 手动备份

```bash
# 备份数据库
docker compose exec -T postgres pg_dump -U synapse synapse > backup.sql

# 备份媒体文件
docker run --rm -v synapse_media:/media -v $(pwd):/backup alpine tar czf /backup/media.tar.gz -C /media .
```

---

## 常见问题

### Q1: 数据库连接失败

**症状**: Synapse 无法启动，日志显示数据库连接错误

**解决方案**:

```bash
# 检查数据库状态
docker compose ps postgres

# 检查数据库日志
docker compose logs postgres

# 检查网络连接
docker compose exec synapse ping postgres
```

### Q2: 迁移失败

**症状**: migrator 容器退出码非 0

**解决方案**:

```bash
# 查看迁移日志
docker compose logs migrator

# 手动运行迁移
./scripts/migrate.sh run

# 检查迁移状态
./scripts/migrate.sh status
```

### Q3: Nginx 502 错误

**症状**: 访问服务返回 502 Bad Gateway

**解决方案**:

```bash
# 检查 Synapse 是否运行
docker compose ps synapse

# 检查 Synapse 健康状态
curl http://localhost:8008/health

# 重启服务
docker compose restart synapse nginx
```

### Q4: 内存不足

**症状**: 服务频繁重启，OOM 错误

**解决方案**:

```bash
# 增加系统内存
# 或限制容器内存
# 编辑 docker-compose.yml 添加:
services:
  synapse:
    deploy:
      resources:
        limits:
          memory: 2G
```

---

## 故障排查

### 查看日志

```bash
# 查看所有日志
docker compose logs

# 实时查看日志
docker compose logs -f

# 查看最近 100 行
docker compose logs --tail=100

# 查看特定时间范围
docker compose logs --since="2024-01-01T00:00:00"
```

### 健康检查

```bash
# 检查所有服务健康状态
docker compose ps

# 检查 Synapse 健康端点
curl -f http://localhost:8008/health

# 检查数据库连接
docker compose exec postgres pg_isready

# 检查 Redis 连接
docker compose exec redis redis-cli -a "$REDIS_PASSWORD" ping
```

### 网络排查

```bash
# 检查网络
docker network ls
docker network inspect synapse-network

# 测试服务间连接
docker compose exec synapse ping postgres
docker compose exec synapse ping redis
```

### 重置部署

```bash
# 停止并删除所有容器和数据卷
docker compose down -v

# 重新部署
./deploy.sh
```

---

## 服务架构

| 服务 | 镜像 | 端口 | 说明 |
|------|------|------|------|
| synapse | vmuser232922/mysynapse:latest | 8008, 8448 | Matrix 主服务器 |
| postgres | postgres:16-alpine | 容器内 5432 | PostgreSQL 数据库 |
| redis | redis:7-alpine | 容器内 6379 | Redis 缓存 |
| nginx | nginx:alpine | 80, 443, 8448 | 反向代理 |
| migrator | vmuser232922/mysynapse:latest | - | 数据库迁移服务 |

### 端口说明

| 端口 | 协议 | 用途 |
|------|------|------|
| 80 | HTTP | Web 访问入口 |
| 443 | HTTPS | 安全 Web 访问入口 |
| 8008 | HTTP | Synapse 客户端 API (内部) |
| 8448 | HTTPS | Matrix 联邦端口 (服务器间通信) |
| 5432 | TCP | PostgreSQL 数据库，仅在显式叠加 `docker-compose.dev-host-access.yml` 时暴露到宿主机 |
| 6379 | TCP | Redis 缓存，仅在显式叠加 `docker-compose.dev-host-access.yml` 时暴露到宿主机 |

### 联邦配置

Matrix 联邦允许不同服务器之间进行通信。要启用联邦功能：

1. **DNS 配置**: 确保您的域名有正确的 SRV 记录

   ```text
   _matrix._tcp.example.com. 3600 IN SRV 10 5 8448 matrix.example.com.
   ```

2. **端口开放**: 确保 8448 端口可从公网访问

3. **SSL 证书**: 联邦端口必须使用有效的 SSL 证书

4. **测试联邦**:

   ```bash
   # 测试联邦连接
   curl https://matrix.example.com:8448/_matrix/federation/v1/version
   ```

---

## 联系支持

- **项目地址**: https://github.com/your-org/synapse-rust
- **问题反馈**: https://github.com/your-org/synapse-rust/issues
- **文档**: https://matrix.org/docs/guides

---

## 更新日志

| 版本 | 日期 | 变更说明 |
|------|------|---------|
| v1.0.0 | 2026-04-02 | 初始版本 |
