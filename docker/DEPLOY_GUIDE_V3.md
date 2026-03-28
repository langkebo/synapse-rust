# Synapse Rust 一键部署指南

> **版本**: v3.0.0
> **更新日期**: 2026-03-27
> **支持**: Ubuntu 20.04+ / Debian 11+ / CentOS 8+

---

## 目录

1. [环境要求](#环境要求)
2. [部署方式](#部署方式)
3. [一键部署](#一键部署)
4. [验证部署](#验证部署)
5. [NGINX 配置](#nginx-配置)
6. [常见问题](#常见问题)

---

## 环境要求

| 组件 | 最低要求 | 推荐 |
|------|----------|------|
| CPU | 2 核 | 4 核 |
| 内存 | 4 GB | 8 GB |
| 磁盘 | 20 GB | 50 GB |
| Docker | 20.10+ | latest |
| Docker Compose | 2.0+ | latest |

---

## 部署方式

### 方式一：本地开发/测试 (使用 Docker 网络)

如果不需要从主机访问容器端口，可以使用 Docker 内部网络：

```bash
cd synapse-rust/docker
docker compose -f docker-compose.prod.yml up -d
```

服务将在 Docker 内部网络中运行，可通过容器间通信访问。

### 方式二：服务器部署 (推荐)

在真正的服务器上部署，使用标准端口：

```bash
# 克隆项目
git clone https://github.com/vmuser232922/synapse-rust.git
cd synapse-rust/docker

# 运行一键部署
bash deploy.sh
```

部署脚本会自动：
1. 生成安全密码
2. 构建 Docker 镜像
3. 启动数据库和 Redis
4. 运行数据库迁移
5. 启动 Synapse 服务

---

## 一键部署

### 前置条件

1. 安装 Docker
```bash
# Ubuntu/Debian
curl -fsSL https://get.docker.com | sh

# CentOS
curl -fsSL https://get.docker.com | sh
```

2. 安装 Docker Compose
```bash
sudo apt update
sudo apt install -y docker-compose
```

### 部署步骤

```bash
# 1. 进入部署目录
cd synapse-rust/docker

# 2. 运行部署脚本
bash deploy.sh

# 3. 查看服务状态
docker compose -f docker-compose.prod.yml ps

# 4. 查看日志
docker compose -f docker-compose.prod.yml logs -f synapse-main
```

---

## 验证部署

### 1. 检查容器状态

```bash
docker compose -f docker-compose.prod.yml ps
```

输出应显示：
```
NAME                STATUS
synapse_main_prod   Up (healthy)
synapse_db_prod     Up (healthy)
synapse_redis_prod  Up (healthy)
```

### 2. 验证 API

```bash
# Matrix Client API
curl http://localhost:8008/_matrix/client/versions

# Admin API
curl http://localhost:8008/_synapse/admin/v1/health
```

### 3. 验证数据库

```bash
# 进入数据库
docker compose -f docker-compose.prod.yml exec db psql -U synapse -d synapse

# 检查表数量
SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';

# 检查迁移状态
SELECT version, success FROM schema_migrations ORDER BY applied_ts DESC LIMIT 10;
```

---

## NGINX 配置

### 标准反向代理配置

创建 `/etc/nginx/sites-available/synapse`：

```nginx
upstream synapse_backend {
    server 127.0.0.1:8008;
    keepalive 64;
}

server {
    listen 80;
    server_name your-domain.com;

    location / {
        return 301 https://$host$request_uri;
    }
}

server {
    listen 443 ssl http2;
    server_name your-domain.com;

    ssl_certificate /etc/letsencrypt/live/your-domain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/your-domain.com/privkey.pem;

    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
    ssl_prefer_server_ciphers off;

    client_max_body_size 100M;

    location /_matrix/ {
        proxy_pass http://synapse_backend;
        proxy_http_version 1.1;

        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }

    location /_synapse/ {
        proxy_pass http://synapse_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

### 启用配置

```bash
sudo ln -s /etc/nginx/sites-available/synapse /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

---

## 常见问题

### Q1: 端口被占用 (Mac OrbStack 环境)

**问题**: OrbStack 会占用 Docker 映射的端口。

**解决方案**: 使用 Docker 网络内部通信，不暴露端口到主机。

```bash
# 使用 docker exec 进入容器测试
docker compose -f docker-compose.prod.yml exec synapse-main curl http://localhost:8008/_matrix/client/versions
```

### Q2: 数据库连接失败

```bash
# 检查数据库日志
docker compose -f docker-compose.prod.yml logs db

# 验证连接
docker compose -f docker-compose.prod.yml exec db pg_isready -U synapse
```

### Q3: 迁移失败

```bash
# 查看迁移日志
docker compose -f docker-compose.prod.yml logs synapse-main | grep -i migration

# 手动运行迁移
docker compose -f docker-compose.prod.yml exec synapse-main /app/docker/db_migrate.sh
```

### Q4: 如何完全重新部署

```bash
# 1. 停止服务
docker compose -f docker-compose.prod.yml down

# 2. 删除数据卷 (警告：会删除所有数据)
docker compose -f docker-compose.prod.yml down -v

# 3. 重新部署
bash deploy.sh
```

---

## 服务管理

```bash
# 启动
docker compose -f docker-compose.prod.yml up -d

# 停止
docker compose -f docker-compose.prod.yml down

# 重启
docker compose -f docker-compose.prod.yml restart synapse-main

# 查看日志
docker compose -f docker-compose.prod.yml logs -f

# 进入容器
docker compose -f docker-compose.prod.yml exec synapse-main sh
```

---

## 备份与恢复

### 备份

```bash
# 备份数据库
docker compose -f docker-compose.prod.yml exec -T db pg_dump -U synapse synapse > backup_$(date +%Y%m%d).sql

# 备份配置
cp -r config backup_config_$(date +%Y%m%d)
```

### 恢复

```bash
# 恢复数据库
cat backup_20260327.sql | docker compose -f docker-compose.prod.yml exec -T db psql -U synapse -d synapse
```

---

## 端口说明

| 容器端口 | 主机端口 | 用途 |
|----------|----------|------|
| 8008 | 8008 | Matrix Client API |
| 8448 | 8448 | Matrix Federation API |
| 9090 | 9090 | Prometheus Metrics |

**注意**: 在 OrbStack 环境下，端口会被自动占用。请在真正的服务器上部署。
