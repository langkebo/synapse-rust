# Synapse Rust 一键部署指南

> **版本**: v2.0.0
> **更新日期**: 2026-03-27
> **支持**: Ubuntu 20.04+ / Debian 11+

---

## 目录

1. [环境要求](#环境要求)
2. [一键部署](#一键部署)
3. [NGINX 配置](#nginx-配置)
4. [SSL 证书配置](#ssl-证书配置)
5. [验证部署](#验证部署)
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

## 一键部署

### 方式一：使用部署脚本（推荐）

```bash
# 克隆项目
git clone https://github.com/vmuser232922/synapse-rust.git
cd synapse-rust

# 运行一键部署
bash docker/deploy.sh
```

### 方式二：使用 Docker Compose 手动部署

```bash
# 1. 进入部署目录
cd synapse-rust/docker

# 2. 创建环境变量文件
cp .env.example .env
# 编辑 .env 文件配置密码

# 3. 启动服务
docker compose -f docker-compose.prod.yml up -d

# 4. 查看状态
docker compose -f docker-compose.prod.yml ps
```

---

## NGINX 配置

### 标准反向代理配置

创建 `/etc/nginx/sites-available/synapse`：

```nginx
# Upstream 定义
upstream synapse_backend {
    server 127.0.0.1:8008;
    keepalive 64;
}

# HTTP -> HTTPS 重定向
server {
    listen 80;
    listen [::]:80;
    server_name cjystx.top;

    # Let's Encrypt 验证
    location /.well-known/acme-challenge/ {
        root /var/www/certbot;
    }

    # 其他请求重定向到 HTTPS
    location / {
        return 301 https://$host$request_uri;
    }
}

# HTTPS 服务器块
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name cjystx.top;

    # SSL 证书配置
    ssl_certificate /etc/letsencrypt/live/cjystx.top/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/cjystx.top/privkey.pem;
    ssl_trusted_certificate /etc/letsencrypt/live/cjystx.top/chain.pem;

    # SSL 安全配置
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;
    ssl_session_cache shared:SSL:10m;
    ssl_session_timeout 1d;
    ssl_session_tickets off;

    # 安全头
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;

    # 代理 Matrix Client API
    location /_matrix/ {
        proxy_pass http://synapse_backend;
        proxy_http_version 1.1;

        # 连接升级支持 WebSocket
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        # 请求头转发
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-Host $host;

        # 超时配置
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;

        # 缓冲区配置
        proxy_buffering off;
        proxy_request_buffering off;
    }

    # 代理 Synapse Admin API
    location /_synapse/ {
        proxy_pass http://synapse_backend;
        proxy_http_version 1.1;

        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # 限流 (可选)
        # limit_req zone=synapse_admin burst=100 nodelay;
    }

    # 媒体文件代理
    location /_matrix/media/ {
        proxy_pass http://synapse_backend;
        proxy_http_version 1.1;

        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;

        # 大文件超时延长
        proxy_connect_timeout 300s;
        proxy_send_timeout 300s;
        proxy_read_timeout 300s;
    }

    # 健康检查端点
    location /health {
        proxy_pass http://synapse_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        access_log off;
    }

    # 上传文件大小限制
    client_max_body_size 100M;
    client_body_timeout 300s;
}
```

### 启用 NGINX 配置

```bash
# 1. 创建配置链接
sudo ln -s /etc/nginx/sites-available/synapse /etc/nginx/sites-enabled/

# 2. 测试配置
sudo nginx -t

# 3. 重新加载 NGINX
sudo systemctl reload nginx
```

---

## SSL 证书配置

### 使用 Let's Encrypt (免费)

```bash
# 1. 安装 Certbot
sudo apt update
sudo apt install -y certbot python3-certbot-nginx

# 2. 获取证书
sudo certbot --nginx -d cjystx.top

# 3. 设置自动续期
sudo certbot renew --dry-run
```

### 手动配置 SSL

如果使用其他证书，修改 NGINX 配置中的：

```nginx
ssl_certificate /path/to/certificate.pem;
ssl_certificate_key /path/to/private_key.pem;
```

---

## 验证部署

### 1. 检查容器状态

```bash
docker compose -f docker-compose.prod.yml ps
```

输出应显示所有容器为 `healthy` 状态。

### 2. 验证 API

```bash
# 检查 Matrix 版本
curl http://localhost:8008/_matrix/client/versions

# 检查 Admin API
curl http://localhost:8008/_synapse/admin/v1/health
```

### 3. 验证数据库

```bash
# 进入数据库容器
docker compose -f docker-compose.prod.yml exec db psql -U synapse -d synapse

# 检查表数量
SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';

# 检查迁移状态
SELECT version, description, success FROM schema_migrations ORDER BY applied_ts DESC LIMIT 10;
```

### 4. 查看日志

```bash
# 查看主服务日志
docker compose -f docker-compose.prod.yml logs -f synapse-main

# 查看数据库日志
docker compose -f docker-compose.prod.yml logs -f db
```

---

## 常见问题

### Q1: 容器启动失败，显示端口已被占用

```bash
# 检查端口占用
lsof -i :8008
netstat -tlnp | grep 8008

# 如果是 OrbStack 或其他 VM 占用了端口
# 在 Docker Desktop/OrbStack 设置中更改端口映射
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
# 查看迁移错误
docker compose -f docker-compose.prod.yml logs synapse-main | grep -i migration

# 手动运行迁移
docker compose -f docker-compose.prod.yml exec synapse-main /app/scripts/run-migrations.sh
```

### Q4: NGINX 502 Bad Gateway

1. 检查 Synapse 容器是否运行：
   ```bash
   docker compose -f docker-compose.prod.yml ps synapse-main
   ```

2. 检查 Synapse 容器日志：
   ```bash
   docker compose -f docker-compose.prod.yml logs synapse-main
   ```

3. 验证端口绑定：
   ```bash
   docker port synapse-main
   ```

### Q5: WebSocket 连接失败

确保 NGINX 配置包含：

```nginx
proxy_set_header Upgrade $http_upgrade;
proxy_set_header Connection "upgrade";
```

### Q6: 如何完全重新部署

```bash
# 1. 停止所有服务
docker compose -f docker-compose.prod.yml down

# 2. 删除数据卷（警告：会删除所有数据）
docker compose -f docker-compose.prod.yml down -v

# 3. 清除 Docker 缓存
docker system prune -af

# 4. 重新部署
bash docker/deploy.sh
```

---

## 服务管理命令

```bash
# 启动服务
docker compose -f docker-compose.prod.yml up -d

# 停止服务
docker compose -f docker-compose.prod.yml down

# 重启服务
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
cp -r docker/config backup_config_$(date +%Y%m%d)
```

### 恢复

```bash
# 恢复数据库
cat backup_20260327.sql | docker compose -f docker-compose.prod.yml exec -T db psql -U synapse -d synapse
```

---

## 监控

推荐使用以下端点进行监控：

| 端点 | 用途 |
|------|------|
| `GET /_synapse/admin/v1/health` | 健康检查 |
| `GET /_matrix/client/versions` | API 版本 |
| `GET /health` | 简化健康检查 |