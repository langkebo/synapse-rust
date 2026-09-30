# Nginx 安全加固指南

> 版本：v1.0（2026-09-30）
> 适用范围：synapse-rust Prometheus 监控栈安全加固

---

## 1. 安全加固概览

### 1.1 当前风险

| 风险项 | 严重性 | 说明 |
|--------|--------|------|
| Prometheus API 无认证 | Critical | 任何人可访问全部指标，包括用户数据 |
| Alertmanager 无认证 | High | 攻击者可查看/静默告警 |
| Grafana 无认证 | High | 攻击者可查看/修改仪表盘 |
| 端口暴露在外网 | Critical | 9092/9093/3000 可能从外网访问 |

### 1.2 加固方案

```
架构变化：

修改前：外部直接访问 9092/9093/3000
修改后：通过 nginx(:8081) 统一代理 + Basic Auth

┌─────────────────────────────────────────────┐
│  外部访问（需认证）                          │
│  curl -u user:pass http://localhost:8081    │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│  nginx 反向代理 (:8081)                     │
│  ├─ Basic Auth                              │
│  ├─ 速率限制 (10 req/s)                     │
│  └─ 请求日志                                │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│  Docker 内部网络（无外部暴露）               │
│  ├─ prometheus (:9090)                      │
│  ├─ alertmanager (:9093)                    │
│  └─ grafana (:3000)                         │
└─────────────────────────────────────────────┘
```

---

## 2. 实施步骤

### 2.1 创建认证文件

```bash
# 在项目目录下执行
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 创建 htpasswd 目录
mkdir -p docker/deploy/nginx/auth

# 创建 admin 用户（密码自行设置）
docker run --rm \
  -v "$(pwd)/docker/deploy/nginx/auth:/htpasswd" \
  httpd:2.4-alpine \
  htpasswd -bc /htpasswd/.htpasswd admin "$(openssl rand -base64 32)"

# 创建 ops 用户（只读权限）
docker run --rm \
  -v "$(pwd)/docker/deploy/nginx/auth:/htpasswd" \
  httpd:2.4-alpine \
  htpasswd -b /htpasswd/.htpasswd ops "$(openssl rand -base64 32)"

# 查看生成的文件
ls -la docker/deploy/nginx/auth/
cat docker/deploy/nginx/auth/.htpasswd
```

### 2.2 更新 docker-compose.monitoring.yml

添加 nginx 代理服务：

```yaml
services:
  # ... 现有服务 ...

  nginx-proxy:
    build:
      context: ./nginx
      dockerfile: Dockerfile
    container_name: synapse-nginx-proxy
    restart: unless-stopped
    cpus: 0.5
    mem_limit: 128m
    ports:
      - "127.0.0.1:8081:8081"
    volumes:
      - ./nginx/prometheus.conf:/etc/nginx/conf.d/prometheus.conf:ro
      - ./nginx/auth/.htpasswd:/etc/nginx/.htpasswd:ro
    depends_on:
      - synapse-prometheus
      - synapse-alertmanager
      - synapse-grafana
    networks:
      - synapse-network
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "5"
```

### 2.3 验证配置

```bash
# 重新构建监控栈
/opt/homebrew/bin/docker compose -f docker/docker-compose.monitoring.yml up -d nginx-proxy

# 验证 nginx 容器启动
/opt/homebrew/bin/docker ps | grep nginx

# 测试无认证被拒绝
curl -s http://localhost:8081/prometheus/api/v1/query?query=up
# 期望: HTTP 401

# 测试有认证成功
PROM_PASS=$(grep admin docker/deploy/nginx/auth/.htpasswd | cut -d: -f2 | xargs)
curl -s -u admin:$PROM_PASS "http://localhost:8081/prometheus/api/v1/query?query=up" | jq '.'
```

---

## 3. 环境变量配置

在 `.env` 文件中添加：

```bash
# Nginx 代理端口
NGINX_PROXY_PORT=8081

# Prometheus 访问凭据
PROM_USER=admin
PROM_PASS=your_secure_password_here

# 可选：只读权限用户（用于 k6 等只读工具）
PROM_READONLY_USER=ops
PROM_READONLY_PASS=your_readonly_password_here
```

---

## 4. 访问方式变更

### 4.1 Prometheus API

```bash
# 旧方式（不安全）
curl http://localhost:9092/api/v1/query?query=up

# 新方式（安全）
export PROM_AUTH="admin:your_password"
curl -u $PROM_AUTH "http://localhost:8081/prometheus/api/v1/query?query=up"
```

### 4.2 Alertmanager API

```bash
# 新方式
curl -u $PROM_AUTH "http://localhost:8081/alertmanager/api/v2/status"
```

### 4.3 Grafana Web UI

```bash
# 新方式
open "http://localhost:8081/grafana"
```

---

## 5. 安全检查清单

部署后验证：

- [ ] nginx-proxy 容器运行中
- [ ] htpasswd 文件已生成且权限正确（600）
- [ ] 直接访问 9092/9093/3000 被拒绝（仅 localhost）
- [ ] 访问 8081 无认证返回 401
- [ ] 访问 8081 有认证正常响应
- [ ] 速率限制生效（快速多次请求返回 503）
- [ ] 日志记录正常

---

## 6. HTTPS 配置（生产环境推荐）

### 6.1 生成自签名证书

```bash
# 生成 CA 密钥和证书
openssl genrsa -out /tmp/ca.key 4096
openssl req -new -x509 -key /tmp/ca.key -out /tmp/ca.crt -days 365 \
  -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=Synapse CA"

# 生成服务器证书
openssl genrsa -out /tmp/server.key 4096
openssl req -new -key /tmp/server.key -out /tmp/server.csr \
  -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=localhost"
openssl x509 -req -in /tmp/server.csr -CA /tmp/ca.crt -CAkey /tmp/ca.key \
  -CAcreateserial -out /tmp/server.crt -days 365

# 拷贝证书到配置目录
cp /tmp/server.crt docker/deploy/nginx/certs/
cp /tmp/server.key docker/deploy/nginx/certs/
```

### 6.2 更新 nginx 配置

```nginx
server {
    listen 443 ssl;
    server_name localhost;

    ssl_certificate /etc/nginx/certs/server.crt;
    ssl_certificate_key /etc/nginx/certs/server.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;

    # ... rest of config
}
```

---

## 7. 回滚方案

```bash
# 停止 nginx 代理
docker stop synapse-nginx-proxy

# 移除端口映射（直接访问）
# 编辑 docker-compose.monitoring.yml，添加：
#   - "9092:9090"
#   - "9093:9093"
#   - "3000:3000"

# 重启服务
docker compose -f docker/docker-compose.monitoring.yml up -d
```

---

*文档创建时间：2026-09-30 | 最后更新：2026-09-30*
