# 离线部署说明

> **更新时间**: 2026-02-04
> **镜像版本**: synapse-rust:dev (300049d26c35)
> **镜像大小**: 514MB / 113MB (压缩后)
> **生产环境**: 支持域名 cjystx.top 和 matrix.cjystx.top

本目录包含已导出的离线镜像包与部署说明，便于在无外网环境快速部署 Synapse Rust 生产环境。

---

## 目录结构

```
docker/
├── imags/                      # 离线镜像目录
│   ├── synapse-rust_dev_*.tar  # Docker 镜像包
│   └── DEPLOYMENT.md           # 本部署文档
├── config/
│   └── homeserver.yaml         # Matrix 服务器配置 (用户名格式: @user:cjystx.top)
├── nginx/
│   ├── nginx.conf              # Nginx 主配置
│   └── conf.d/                 # 虚拟主机配置
│       └── default.conf
├── scripts/
│   └── deploy.sh               # 生产环境部署脚本
└── docker-compose.yml          # Docker Compose 配置
```

---

## 架构概览

```
                    ┌─────────────────────────────────────────────────────┐
                    │                    生产环境架构                      │
                    └─────────────────────────────────────────────────────┘

    ┌──────────────┐         ┌──────────────────────────────────────────┐
    │   用户客户端   │         │              Nginx 反向代理               │
    │  (Element 等) │────────▶│   (端口: 80/443)                         │
    └──────────────┘         │   - SSL/TLS 终结                           │
                             │   - 服务发现 (.well-known)                  │
                             │   - 反向代理                                │
                             └────────────────┬───────────────────────────┘
                                              │
                                              ▼
                              ┌──────────────────────────────────────────┐
                              │           Matrix Synapse                  │
                              │           (端口: 8008)                    │
                              │   - 用户认证与授权                         │
                              │   - 房间管理                               │
                              │   - 消息传递                               │
                              │   - 联邦通信 (联邦端口: 8448)              │
                              └────────────────┬───────────────────────────┘
                                               │
                    ┌──────────────────────────┼──────────────────────────┐
                    ▼                          ▼                          ▼
        ┌─────────────────────┐   ┌─────────────────────┐   ┌─────────────────────┐
        │   PostgreSQL 15+    │   │     Redis 7.0+      │   │    文件存储          │
        │   (端口: 5432)      │   │    (端口: 6379)     │   │   媒体文件/上传      │
        │   - 用户数据        │   │   - 会话缓存        │   └─────────────────────┘
        │   - 消息历史        │   │   - 速率限制        │
        │   - 房间状态        │   │   - 键值缓存        │
        └─────────────────────┘   └─────────────────────┘
```

---

## 生产环境域名规划

| 域名 | 用途 | 服务发现 | API 访问 |
|------|------|----------|----------|
| `cjystx.top` | 主域名 (服务发现) | ✓ | ✗ |
| `matrix.cjystx.top` | Matrix 服务器 | ✗ | ✓ |
| `www.cjystx.top` | 主域名 WWW | ✗ | ✗ |

### 服务发现端点

- **客户端发现**: `https://cjystx.top/.well-known/matrix/client`
- **服务器发现**: `https://cjystx.top/.well-known/matrix/server`
- **Matrix API**: `https://matrix.cjystx.top/_matrix/*`

---

## 镜像信息

| 属性 | 值 |
|------|-----|
| 镜像名称 | synapse-rust:dev |
| 镜像 ID | 300049d26c35 |
| 镜像大小 | 514MB |
| 压缩大小 | 108MB |
| 保存时间 | 2026-02-04 13:22 |
| 文件名 | `synapse-rust_dev_20260204_132223.tar` |

---

## 快速部署

### 1. 环境准备

```bash
# 系统要求
- Docker 20.10+
- Docker Compose 2.0+ 或 docker compose plugin
- curl 或 wget (用于健康检查)
- openssl (用于证书管理)
- git (用于克隆仓库)

# 检查 Docker 版本
docker --version  # 应 >= 20.10.0
docker-compose --version  # 应 >= 2.0.0
```

### 2. 导入镜像

```bash
# 切换到 imags 目录
cd /home/hula/synapse_rust/docker/imags

# 导入镜像
docker load -i synapse-rust_dev_20260204_132223.tar

# 验证导入
docker images | grep synapse-rust
# 输出应类似:
# synapse-rust   dev    300049d26c35   2 weeks ago   514MB
```

### 3. 创建目录结构

```bash
# 创建必要目录
mkdir -p /home/hula/synapse_rust/docker/{config,nginx/conf.d,scripts,data,logs,ssl/cjystx.top,ssl/matrix.cjystx.top}

# 克隆项目 (如需要)
git clone https://github.com/langkebo/synapse.git
cd synapse/docker

# 复制配置文件
cp config/homeserver.yaml /home/hula/synapse_rust/docker/config/
cp nginx/nginx.conf /home/hula/synapse_rust/docker/nginx/
cp nginx/conf.d/*.conf /home/hula/synapse_rust/docker/nginx/conf.d/
cp scripts/deploy.sh /home/hula/synapse_rust/docker/scripts/

# 设置执行权限
chmod +x /home/hula/synapse_rust/docker/scripts/deploy.sh
```

### 4. 配置 SSL 证书

#### 方式一: 使用 acme.sh 自动申请 (推荐)

```bash
# 安装 acme.sh
curl https://get.acme.sh | sh

# 设置默认 CA (Let's Encrypt 或 ZeroSSL)
~/.acme.sh/acme.sh --set-default-ca --server letsencrypt

# 申请证书 (需要域名已解析到服务器)
# 主域名证书 (包含 www)
~/.acme.sh/acme.sh --issue -d cjystx.top -d www.cjystx.top \
    --webroot /var/www/html \
    --keylength ec-256

# Matrix 子域名证书
~/.acme.sh/acme.sh --issue -d matrix.cjystx.top \
    --webroot /var/www/html \
    --keylength ec-256

# 安装证书到 Nginx 目录
~/.acme.sh/acme.sh --installcert -d cjystx.top \
    --fullchainfile /home/hula/synapse_rust/docker/ssl/cjystx.top/fullchain.pem \
    --keyfile /home/hula/synapse_rust/docker/ssl/cjystx.top/privkey.pem

~/.acme.sh/acme.sh --installcert -d matrix.cjystx.top \
    --fullchainfile /home/hula/synapse_rust/docker/ssl/matrix.cjystx.top/fullchain.pem \
    --keyfile /home/hula/synapse_rust/docker/ssl/matrix.cjystx.top/privkey.pem

# 设置自动续期
~/.acme.sh/acme.sh --upgrade --auto-upgrade
~/.acme.sh/acme.sh --register-account -m admin@cjystx.top

# 设置权限
chmod 600 /home/hula/synapse_rust/docker/ssl/*/privkey.pem
chmod 644 /home/hula/synapse_rust/docker/ssl/*/fullchain.pem
```

#### 方式二: 使用自签名证书 (测试环境)

```bash
# 生成自签名证书
openssl req -x509 -nodes -days 365 -newkey ec:<(openssl ecparam -name prime256v1) \
    -keyout /home/hula/synapse_rust/docker/ssl/cjystx.top/privkey.pem \
    -out /home/hula/synapse_rust/docker/ssl/cjystx.top/fullchain.pem \
    -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=cjystx.top"

openssl req -x509 -nodes -days 365 -newkey ec:<(openssl ecparam -name prime256v1) \
    -keyout /home/hula/synapse_rust/docker/ssl/matrix.cjystx.top/privkey.pem \
    -out /home/hula/synapse_rust/docker/ssl/matrix.cjystx.top/fullchain.pem \
    -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=matrix.cjystx.top"

chmod 600 /home/hula/synapse_rust/docker/ssl/*/privkey.pem
chmod 644 /home/hula/synapse_rust/docker/ssl/*/fullchain.pem
```

### 5. 使用部署脚本启动

```bash
# 进入部署目录
cd /home/hula/synapse_rust/docker

# 初始化并申请 SSL 证书 (如尚未申请)
./scripts/deploy.sh ssl-init

# 启动所有服务
./scripts/deploy.sh start

# 检查服务状态
./scripts/deploy.sh status

# 执行健康检查
./scripts/deploy.sh health

# 查看日志
./scripts/deploy.sh logs synapse
```

---

## 配置说明

### homeserver.yaml 配置

#### 核心配置

```yaml
server:
  name: "cjystx.top"                          # 用户名格式: @user:cjystx.top
  host: "0.0.0.0"
  port: 8008
  public_host: "matrix.cjystx.top"             # 公开访问域名
  registration_shared_secret: "your-secret"
  admin_contact: "admin@cjystx.top"
  max_upload_size: 104857600                   # 100MB
  enable_registration: true
  enable_registration_captcha: false
```

#### 联邦配置

```yaml
federation:
  enabled: true
  allow_ingress: true
  server_name: "cjystx.top"                    # 联邦服务器名
  federation_port: 8448
  signing_key: "BASE64_32_BYTES_SEED_HERE"     # 生成: openssl rand -base64 32
  connection_pool_size: 10
  max_transaction_payload: 10485760            # 10MB
```

#### 数据库配置

```yaml
database:
  host: "db"
  port: 5432
  username: "synapse"
  password: "synapse"
  name: "synapse_test"
  pool_size: 20
  max_size: 100
  min_idle: 5
  connection_timeout: 60
```

#### Redis 配置

```yaml
redis:
  host: "redis"
  port: 6379
  key_prefix: "synapse:"
  pool_size: 20
  enabled: true
```

#### 安全配置

```yaml
security:
  bcrypt_rounds: 12
  password:
    minimum_length: 8
    require_digit: true
    require_symbol: true
    require_uppercase: true
    require_lowercase: true
    blacklist:
      - "password"
      - "123456"
      - "qwerty"
      - "admin"
  sessions:
    maximum: 100
    idle_timeout: 86400
    expiry_time: 3600
```

### Nginx 配置

#### Nginx 主配置结构

```
/etc/nginx/
├── nginx.conf                    # 主配置 (工作进程、事件处理)
└── conf.d/
    └── default.conf              # Matrix 服务器配置
```

#### Nginx 配置关键参数

```nginx
# 上游服务器定义
upstream synapse_backend {
    server synapse:8008;
    keepalive 32;
}

# cjystx.top - 仅服务发现
server {
    listen 443 ssl http2;
    server_name cjystx.top www.cjystx.top;

    ssl_certificate /etc/nginx/ssl/cjystx.top/fullchain.pem;
    ssl_certificate_key /etc/nginx/ssl/cjystx.top/privkey.pem;

    # 服务发现端点
    location /.well-known/matrix/server {
        default_type application/json;
        return 200 '{"m.server":"matrix.cjystx.top:443"}';
    }

    location /.well-known/matrix/client {
        default_type application/json;
        return 200 '{"m.homeserver":{"base_url":"https://matrix.cjystx.top"}}';
    }
}

# matrix.cjystx.top - Matrix API
server {
    listen 443 ssl http2;
    server_name matrix.cjystx.top;

    ssl_certificate /etc/nginx/ssl/matrix.cjystx.top/fullchain.pem;
    ssl_certificate_key /etc/nginx/ssl/matrix.cjystx.top/privkey.pem;

    # 安全响应头
    add_header Strict-Transport-Security "max-age=63072000" always;
    add_header X-Frame-Options "SAMEORIGIN" always;

    location / {
        proxy_pass http://synapse_backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

---

## 使用说明

### 部署脚本命令

| 命令 | 说明 |
|------|------|
| `./deploy.sh start` | 启动所有服务 |
| `./deploy.sh stop` | 停止所有服务 |
| `./deploy.sh restart` | 重启所有服务 |
| `./deploy.sh status` | 查看服务状态 |
| `./deploy.sh logs [服务]` | 查看日志 (可选: nginx, synapse, db, redis) |
| `./deploy.sh health` | 执行健康检查 |
| `./deploy.sh ssl-init` | 初始化并申请 SSL 证书 |
| `./deploy.sh ssl-renewal` | 续期 SSL 证书 |
| `./deploy.sh backup` | 执行完整备份 |
| `./deploy.sh restore <文件>` | 从备份文件恢复 |
| `./deploy.sh scale [副本数]` | 扩展 Synapse 服务 |
| `./deploy.sh verify` | 验证域名配置 |
| `./deploy.sh check` | 检查依赖和配置 |

### Docker Compose 手动管理

```bash
# 启动所有服务
docker compose up -d

# 查看服务状态
docker compose ps

# 查看日志
docker compose logs -f synapse

# 重启单个服务
docker compose restart synapse

# 停止所有服务
docker compose down

# 停止并删除数据卷 (危险!)
docker compose down -v
```

---

## 验证清单

### 基础验证

- [ ] Docker 镜像成功导入
- [ ] SSL 证书已正确配置
- [ ] PostgreSQL 连接正常
- [ ] Redis 连接正常
- [ ] 服务启动成功
- [ ] API 端点可访问 (`https://matrix.cjystx.top/_matrix/client/versions`)

### 服务发现验证

```bash
# 检查 .well-known/matrix/server
curl https://cjystx.top/.well-known/matrix/server
# 应返回: {"m.server":"matrix.cjystx.top:443"}

# 检查 .well-known/matrix/client
curl https://cjystx.top/.well-known/matrix/client
# 应返回: {"m.homeserver":{"base_url":"https://matrix.cjystx.top"}}
```

### Matrix API 验证

```bash
# 检查 Matrix 版本
curl https://matrix.cjystx.top/_matrix/client/versions
# 应返回 JSON 格式的版本信息
```

### 联邦功能验证 (如启用)

```bash
# 检查联邦端口
nc -zv localhost 8448

# 检查联邦密钥
curl https://matrix.cjystx.top/_matrix/key/v2/server/unsign
```

---

## 故障排除

### 端口冲突

```bash
# 检查 8008 端口占用
lsof -i :8008
lsof -i :443
lsof -i :80

# 查看 Docker 端口映射
docker ps --format "{{.Names}}\t{{.Ports}}" | grep -E "80|443|8008"
```

### 数据库连接失败

```bash
# 验证 PostgreSQL 连接
docker exec synapse-db pg_isready -U synapse

# 检查数据库日志
docker logs synapse-db

# 测试数据库连接
docker exec -it synapse-db psql -U synapse -d synapse_test
```

### SSL 证书问题

```bash
# 检查证书有效期
openssl x509 -enddate -noout -in /home/hula/synapse_rust/docker/ssl/matrix.cjystx.top/fullchain.pem

# 手动续期证书
~/.acme.sh/acme.sh --renew -d matrix.cjystx.top --force

# 重启 Nginx 加载新证书
docker restart nginx-proxy
```

### 服务无法启动

```bash
# 查看详细日志
docker logs synapse 2>&1 | tail -100

# 检查配置语法
docker exec nginx-proxy nginx -t

# 检查磁盘空间
df -h

# 检查内存使用
free -h
```

---

## 备份与恢复

### 自动备份

部署脚本支持自动备份:

```bash
# 执行完整备份
./scripts/deploy.sh backup
# 备份文件保存至: /home/hula/synapse_rust/docker/backup/

# 保留最近 7 天的备份
# 自动清理过期备份
```

### 手动备份

```bash
# 备份数据库
docker exec synapse-db pg_dump -U synapse synapse_test > backup_database_$(date +%Y%m%d).sql

# 备份配置和媒体文件
tar -czf backup_synapse_$(date +%Y%m%d).tar.gz \
    config/homeserver.yaml \
    data/ \
    media/
```

### 恢复操作

```bash
# 从备份文件恢复
./scripts/deploy.sh restore /home/hula/synapse_rust/docker/backup/synapse_backup_20260204_120000.tar.gz

# 或手动恢复
# 1. 停止服务
./scripts/deploy.sh stop

# 2. 恢复数据库
docker exec -i synapse-db psql -U synapse synapse_test < backup_database_20260204.sql

# 3. 恢复配置
tar -xzf backup_synapse_20260204.tar.gz -C /

# 4. 重启服务
./scripts/deploy.sh start
```

---

## 生产环境安全建议

### 防火墙配置

```bash
# 开放必要端口
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp

# 禁止外部访问内部服务
ufw deny 5432/tcp
ufw deny 6379/tcp
ufw deny 8008/tcp

# 启用防火墙
ufw enable
```

### SSL/TLS 强化

```nginx
# SSL 配置建议
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
ssl_prefer_server_ciphers off;
ssl_session_cache shared:SSL:10m;
ssl_session_timeout 1d;
```

### 监控与告警

建议配置以下监控指标:

- 服务可用性 (健康检查)
- SSL 证书有效期
- 磁盘使用率 (< 80%)
- 内存使用率 (< 80%)
- 数据库连接数
- API 响应时间
- 错误率

---

## 相关文档

| 文档 | 路径 | 说明 |
|------|------|------|
| 项目文档 | `docs/` | 完整项目文档 |
| 测试结果 | `docs/TEST_RESULTS_SUMMARY.md` | API 测试结果汇总 |
| API 文档 | `docs/api-SDK/` | API 接口文档 |
| Nginx 配置 | `docker/nginx/nginx.conf` | Nginx 详细配置 |
| 服务配置 | `docker/config/homeserver.yaml` | Matrix 服务器配置 |
| 部署脚本 | `docker/scripts/deploy.sh` | 自动化部署脚本 |
| GitHub 仓库 | https://github.com/langkebo/synapse | 项目源码 |

---

## 版本历史

| 日期 | 镜像版本 | Git 提交 | 说明 |
|------|---------|---------|------|
| 2026-02-04 | dev (300049d) | d41ae76 | 生产环境配置优化: Nginx 服务发现分离、用户名格式统一为 @user:cjystx.top、添加部署脚本 |
| 2026-02-04 | dev (300049d) | d41ae76 | 代码质量优化与冗余清理 |
| 2026-02-01 | dev | 750c9da | 初始优化版本 |

---

## 获取帮助

如遇到问题，请按以下步骤处理:

1. 查看日志: `./deploy.sh logs <服务>`
2. 执行健康检查: `./deploy.sh health`
3. 验证配置: `./deploy.sh verify`
4. 查阅故障排除章节
5. 在 GitHub Issues 报告问题: https://github.com/langkebo/synapse/issues

---

**项目维护者**: synapse-rust Team
**最后更新**: 2026-02-04
