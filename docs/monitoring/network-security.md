# Prometheus 网络安全配置

## 方案概述

为 Prometheus 监控栈配置网络隔离和 API 认证，防止未授权访问。

### 架构变化

```
┌─────────────────────────────────────────────────────────────┐
│                    宿主机网络层                               │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  nginx 反向代理 (:8081)                                │   │
│  │    ├─ Basic Auth                                    │   │
│  │    ├─ /api/v1/* → Prometheus (9092)                 │   │
│  │    └─ /api/v2/* → Alertmanager (9093)               │   │
│  └──────────────────────────────────────────────────────┘   │
│                           ↓                                  │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  monitoring-network (Docker bridge)                   │   │
│  │                                                      │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐  │   │
│  │  │ prometheus   │  │ alertmanager │  │ grafana     │  │   │
│  │  │ 172.25.0.2   │  │ 172.25.0.3   │  │ 172.25.0.4  │  │   │
│  │  └──────────────┘  └──────────────┘  └─────────────┘  │   │
│  │                                                      │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  外部访问限制：                                              │
│  - 9092/9093/3000 端口仅限 127.0.0.1                          │
│  - 通过 nginx(:8081) 统一对外提供 HTTPS                        │
└─────────────────────────────────────────────────────────────┘
```

## 实施步骤

### 1. 创建独立 Docker 网络

```bash
docker network create --driver bridge \
  --subnet 172.25.0.0/24 \
  monitoring-network
```

### 2. 配置 nginx 反向代理

```nginx
# /etc/nginx/conf.d/prometheus.conf
upstream prometheus {
    server 172.25.0.2:9090;
}

upstream alertmanager {
    server 172.25.0.3:9093;
}

server {
    listen 8081;
    server_name _;

    # Basic Authentication
    auth_basic "Prometheus Monitoring";
    auth_basic_user_file /etc/nginx/.htpasswd;

    # Prometheus API
    location /prometheus/api/v1/ {
        proxy_pass http://prometheus:9090/api/v1/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # Alertmanager API
    location /alertmanager/api/v2/ {
        proxy_pass http://alertmanager:9093/api/v2/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # Grafana (可选，内部访问)
    location /grafana/ {
        proxy_pass http://grafana:3000/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### 3. 生成 htpasswd 文件

```bash
# 创建 admin 用户（密码自行设置）
htpasswd -c /etc/nginx/.htpasswd admin

# 创建 ops 用户（只读权限，需要额外配置）
htpasswd /etc/nginx/.htpasswd ops
```

### 4. 更新 docker-compose.monitoring.yml

```yaml
networks:
  monitoring:
    driver: bridge
    ipam:
      config:
        - subnet: 172.25.0.0/24

services:
  prometheus:
    networks:
      - monitoring
    ports:
      - "127.0.0.1:9092->9090"  # 仅回环绑定
    # 移除直接暴露，通过 nginx 访问

  alertmanager:
    networks:
      - monitoring
    ports:
      - "127.0.0.1:9093->9093"  # 仅回环绑定

  grafana:
    networks:
      - monitoring
    ports:
      - "127.0.0.1:3000->3000"  # 仅回环绑定

  nginx-proxy:
    image: nginx:alpine
    container_name: synapse-nginx-proxy
    ports:
      - "8081:8081"
    volumes:
      - ./nginx/prometheus.conf:/etc/nginx/conf.d/prometheus.conf:ro
      - ./nginx/.htpasswd:/etc/nginx/.htpasswd:ro
    depends_on:
      - prometheus
      - alertmanager
      - grafana
    networks:
      - monitoring
```

## 访问方式变更

### 修改前
```bash
# 直接访问
curl http://localhost:9092/api/v1/query?query=up
curl http://localhost:9093/api/v2/status
curl http://localhost:3000
```

### 修改后
```bash
# 通过 nginx 代理（需要认证）
curl -u admin:password http://localhost:8081/prometheus/api/v1/query?query=up
curl -u admin:password http://localhost:8081/alertmanager/api/v2/status
curl -u admin:password http://localhost:8081/grafana

# 或使用环境变量
export PROM_AUTH_USER=admin
export PROM_AUTH_PASS=password
curl -u $PROM_AUTH_USER:$PROM_AUTH_PASS http://localhost:8081/prometheus/api/v1/query?query=up
```

## 安全增强建议

### 1. HTTPS 配置（生产环境推荐）

```nginx
server {
    listen 443 ssl;
    server_name monitoring.example.com;

    ssl_certificate /etc/nginx/certs/fullchain.pem;
    ssl_certificate_key /etc/nginx/certs/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;

    # ... rest of config
}
```

### 2. IP 白名单

```nginx
server {
    listen 8081;
    
    # 只允许特定 IP 访问
    allow 10.0.0.0/8;      # 内网
    allow 192.168.0.0/16;  # 内网
    allow 127.0.0.1;       # 本地
    deny all;              # 拒绝其他所有

    # ... rest of config
}
```

### 3. 速率限制

```nginx
limit_req_zone $binary_remote_addr zone=prom_limit:10m rate=10r/s;

server {
    limit_req zone=prom_limit burst=20 nodelay;
    
    # ... rest of config
}
```

## 验证清单

- [ ] 创建 monitoring-network 网络
- [ ] 配置 nginx 反向代理
- [ ] 生成 htpasswd 文件
- [ ] 更新 docker-compose.yml
- [ ] 重启所有容器
- [ ] 验证直接访问被拒绝（9092/9093/3000）
- [ ] 验证 nginx 代理访问正常（8081）
- [ ] 验证 Basic Auth 生效
- [ ] （可选）配置 HTTPS
- [ ] （可选）配置 IP 白名单
- [ ] （可选）配置速率限制

## 回滚方案

如果需要回滚到原来的配置：

```bash
# 1. 停止 nginx 代理
docker stop synapse-nginx-proxy

# 2. 恢复直接访问
docker compose -f docker/docker-compose.monitoring.yml down
# 编辑 docker-compose.monitoring.yml，移除 ports 的 127.0.0.1: 前缀
docker compose -f docker/docker-compose.monitoring.yml up -d

# 3. 验证直接访问正常
curl http://localhost:9092/api/v1/query?query=up
```

---
*更新时间：2026-09-30*
