# Matrix 服务器环境配置指南

> 本文档描述如何配置 Matrix 服务器环境，> 配置时间: 2026-03-10

---

## 配置概述

### 服务器架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    cjystx.top (服务发现)                          │
│  .well-known/matrix/server → {"m.server": "matrix.cjystx.top:443"}   │
│  .well-known/matrix/client → {"m.homeserver": {...}}             │
├─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│               matrix.cjystx.top (Matrix 服务)                     │
│  端口: 443 (HTTPS), 8448 (Federation)                             │
│  用户格式: @user:cjystx.top                                      │
└─────────────────────────────────────────────────────────────────┘
```

---

## 1. 本地 Hosts 文件配置

### 手动执行步骤

由于需要管理员权限，请手动执行以下命令：

```bash
# 方法1: 直接编辑 /etc/hosts
sudo nano /etc/hosts

# 添加以下内容:
127.0.0.1 cjystx.top
127.0.0.1 matrix.cjystx.top

# 方法2: 使用脚本
cd /Users/ljf/Desktop/hu/synapse-rust
chmod +x scripts/add_hosts.sh
sudo ./scripts/add_hosts.sh

# 刷新 DNS 缓存
sudo dscacheutil -flushcache
```

### 验证配置

```bash
# 验证域名解析
ping -c 1 cjystx.top
ping -c 1 matrix.cjystx.top

# 应该解析到 127.0.0.1
```

---

## 2. SSL 证书配置

### 已生成的证书文件

| 文件 | 路径 | 用途 |
|------|------|------|
| CA 证书 | `docker/ssl/ca.crt` | 根证书 |
| 服务器证书 | `docker/ssl/server.crt` | 服务器证书 |
| 私钥 | `docker/ssl/server.key` | 服务器私钥 |
| 完整链 | `docker/ssl/fullchain.pem` | Nginx 使用 |
| 私钥 | `docker/ssl/privkey.pem` | Nginx 使用 |

### 证书详情

```
主题: CN=matrix.cjystx.top
SAN 域名:
  - cjystx.top
  - matrix.cjystx.top
  - localhost
  - *.cjystx.top
有效期: 2026-03-10 至 2027-03-10
```

### 安装 CA 证书到系统信任库

```bash
# macOS
sudo security add-trusted-cert -d -r trustRoot docker/ssl/ca.crt

# 或者手动添加到钥匙串
open docker/ssl/ca.crt  # 双击安装
```

---

## 3. Nginx 配置

### 配置文件位置

`docker/nginx/nginx.conf`

### 关键配置

```nginx
# cjystx.top - 服务发现
server {
    listen 443 ssl;
    server_name cjystx.top;
    
    ssl_certificate /etc/nginx/ssl/fullchain.pem;
    ssl_certificate_key /etc/nginx/ssl/privkey.pem;
    
    # .well-known 发现服务
    location /.well-known/matrix/server {
        return 200 '{"m.server": "matrix.cjystx.top:443"}';
    }
    
    location /.well-known/matrix/client {
        return 200 '{"m.homeserver":{"base_url":"https://matrix.cjystx.top"}}';
    }
}

# matrix.cjystx.top - Matrix 服务
server {
    listen 443 ssl;
    listen 8448 ssl;
    server_name matrix.cjystx.top;
    
    ssl_certificate /etc/nginx/ssl/fullchain.pem;
    ssl_certificate_key /etc/nginx/ssl/privkey.pem;
    
    location ~ ^(/_matrix|/_synapse/client) {
        proxy_pass http://synapse-rust:8008;
    }
}
```

---

## 4. Homeserver 配置

### 配置文件: `homeserver.yaml`

```yaml
server_name: "cjystx.top"

server:
  name: "cjystx.top"
  public_baseurl: "https://matrix.cjystx.top"

federation:
  server_name: "cjystx.top"
  federation_port: 8448
```

---

## 5. Docker Compose 配置

### 启动服务

```bash
cd /Users/ljf/Desktop/hu/synapse-rust/docker

# 使用本地配置启动
docker-compose -f docker-compose.local.yml up -d

# 查看日志
docker-compose -f docker-compose.local.yml logs -f synapse-rust
```

---

## 6. 测试验证

### 测试 .well-known 发现

```bash
# 测试服务发现
curl -k https://cjystx.top/.well-known/matrix/server
# 期望: {"m.server": "matrix.cjystx.top:443"}

curl -k https://cjystx.top/.well-known/matrix/client
# 期望: {"m.homeserver":{"base_url":"https://matrix.cjystx.top"}}
```

### 测试 Matrix API

```bash
# 测试版本端点
curl -k https://matrix.cjystx.top/_matrix/client/versions

# 测试健康检查
curl -k https://matrix.cjystx.top/health
```

### 测试 SSL 证书

```bash
# 检查证书
openssl s_client -connect matrix.cjystx.top:443 -showcerts

# 验证证书链
openssl verify -CAfile docker/ssl/ca.crt docker/ssl/server.crt
```

---

## 7. 完整启动流程

```bash
# 1. 配置 hosts 文件 (需要 sudo)
sudo bash -c 'echo "127.0.0.1 cjystx.top" >> /etc/hosts'
sudo bash -c 'echo "127.0.0.1 matrix.cjystx.top" >> /etc/hosts'

# 2. 刷新 DNS 缓存
sudo dscacheutil -flushcache

# 3. 安装 CA 证书 (可选)
sudo security add-trusted-cert -d -r trustRoot docker/ssl/ca.crt

# 4. 启动 Docker 服务
cd /Users/ljf/Desktop/hu/synapse-rust/docker
docker-compose -f docker-compose.local.yml up -d

# 5. 验证服务
curl -k https://matrix.cjystx.top/health
```

---

## 8. 故障排除

### 常见问题

1. **证书不被信任**
   - 安装 CA 证书到系统信任库
   - 或使用 `-k` 参数跳过证书验证

2. **域名无法解析**
   - 检查 /etc/hosts 配置
   - 刷新 DNS 缓存

3. **连接被拒绝**
   - 检查 Docker 容器状态
   - 检查端口是否被占用

4. **403 Forbidden**
   - 检查 Nginx 配置
   - 检查 Synapse 日志

---

## 9. 配置文件清单

| 文件 | 状态 | 说明 |
|------|------|------|
| `/etc/hosts` | ⏳ 待配置 | 需要手动添加 |
| `docker/ssl/*.pem` | ✅ 已生成 | SSL 证书 |
| `docker/nginx/nginx.conf` | ✅ 已配置 | Nginx 反向代理 |
| `homeserver.yaml` | ✅ 已配置 | Synapse 配置 |

---

*配置指南完成 - 2026-03-10*
