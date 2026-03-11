# Docker 本地环境配置指南

> 配置时间: 2026-03-10
> 服务器名: cjystx.top
> 用户格式: @user:cjystx.top

---

## 快速启动

### 步骤 1: 配置 hosts 文件

```bash
# 添加域名解析到 /etc/hosts
sudo bash -c 'cat >> /etc/hosts << EOF

# Matrix Server - cjystx.top
127.0.0.1 cjystx.top
127.0.0.1 matrix.cjystx.top
EOF'

# 刷新 DNS 缓存
sudo dscacheutil -flushcache
```

### 步骤 2: 启动服务

```bash
cd /Users/ljf/Desktop/hu/synapse-rust/docker
chmod +x start-local.sh
./start-local.sh
```

### 步骤 3: 验证服务

```bash
# 测试健康检查
curl -k https://matrix.cjystx.top/health

# 测试版本信息
curl -k https://matrix.cjystx.top/_matrix/client/versions

# 测试 .well-known 发现
curl -k https://cjystx.top/.well-known/matrix/server
# 期望输出: {"m.server": "matrix.cjystx.top:443"}
```

---

## 配置文件说明

### 已更新的文件

| 文件 | 说明 |
|------|------|
| `config/homeserver.yaml` | 主配置文件，server_name: cjystx.top |
| `config/homeserver.local.yaml` | 本地开发配置 |
| `docker-compose.local.yml` | Docker Compose 配置（含 Nginx） |
| `nginx/nginx.conf` | Nginx 反向代理配置 |
| `ssl/*.pem` | SSL 证书文件 |

### 服务端口映射

| 服务 | 容器端口 | 主机端口 |
|------|---------|---------|
| PostgreSQL | 5432 | 55432 |
| Redis | 6379 | 6379 |
| Synapse HTTP | 8008 | 8008 |
| Synapse Metrics | 9090 | 9090 |
| Nginx HTTP | 80 | 80 |
| Nginx HTTPS | 443 | 443 |
| Federation | 8448 | 8448 |

---

## 常用命令

### 启动服务

```bash
cd /Users/ljf/Desktop/hu/synapse-rust/docker
docker-compose -f docker-compose.local.yml up -d
```

### 停止服务

```bash
docker-compose -f docker-compose.local.yml down
```

### 查看日志

```bash
# 查看所有服务日志
docker-compose -f docker-compose.local.yml logs -f

# 查看 Synapse 日志
docker-compose -f docker-compose.local.yml logs -f synapse-rust

# 查看 Nginx 日志
docker-compose -f docker-compose.local.yml logs -f nginx
```

### 重启服务

```bash
docker-compose -f docker-compose.local.yml restart
```

### 重新构建

```bash
docker-compose -f docker-compose.local.yml up -d --build
```

---

## 测试 API

### 用户注册

```bash
curl -k -X POST https://matrix.cjystx.top/_matrix/client/v3/register \
  -H "Content-Type: application/json" \
  -d '{"username":"testuser","password":"Test@123456","device_id":"TEST_DEVICE"}'
```

### 用户登录

```bash
curl -k -X POST https://matrix.cjystx.top/_matrix/client/v3/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","user":"testuser","password":"Test@123456"}'
```

### 获取用户信息

```bash
curl -k https://matrix.cjystx.top/_matrix/client/v3/account/whoami \
  -H "Authorization: Bearer YOUR_ACCESS_TOKEN"
```

---

## 故障排除

### 1. 域名无法解析

```bash
# 检查 hosts 文件
cat /etc/hosts | grep cjystx

# 刷新 DNS 缓存
sudo dscacheutil -flushcache
```

### 2. SSL 证书错误

```bash
# 使用 -k 参数跳过证书验证
curl -k https://matrix.cjystx.top/health

# 或安装 CA 证书到系统信任库
sudo security add-trusted-cert -d -r trustRoot /Users/ljf/Desktop/hu/synapse-rust/docker/ssl/ca.crt
```

### 3. 容器无法启动

```bash
# 检查容器状态
docker-compose -f docker-compose.local.yml ps

# 检查容器日志
docker-compose -f docker-compose.local.yml logs synapse-rust

# 重新构建
docker-compose -f docker-compose.local.yml up -d --build --force-recreate
```

### 4. 端口被占用

```bash
# 检查端口占用
lsof -i :8008
lsof -i :443

# 停止占用端口的进程
kill -9 <PID>
```

---

## 下一步

完成配置后，执行 API 测试套件：

```bash
cd /Users/ljf/Desktop/hu/synapse-rust
./scripts/comprehensive_api_test_v2.sh
```

---

*配置指南完成 - 2026-03-10*
