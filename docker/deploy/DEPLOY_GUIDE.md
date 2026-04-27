# Synapse-Rust 部署指南

## 快速部署

### 1. 配置环境变量

```bash
cd docker/deploy

# 生成环境变量文件
cat > .env << 'EOF'
SERVER_NAME=localhost
PUBLIC_BASEURL=http://localhost:8008
POSTGRES_PASSWORD=$(openssl rand -base64 24)
REDIS_PASSWORD=$(openssl rand -base64 24)
ADMIN_SHARED_SECRET=$(openssl rand -base64 32)
JWT_SECRET=$(openssl rand -base64 32)
REGISTRATION_SHARED_SECRET=$(openssl rand -base64 32)
SECRET_KEY=$(openssl rand -base64 32)
MACAROON_SECRET=$(openssl rand -base64 32)
FORM_SECRET=$(openssl rand -base64 32)
FEDERATION_SIGNING_KEY=$(openssl rand -base64 32)
WORKER_REPLICATION_SECRET=$(openssl rand -base64 32)
SYNAPSE_IMAGE=vmuser232922/mysynapse:latest
EOF

# 实际生成密钥
sed -i.bak "s/\$(openssl rand -base64 24)/$(openssl rand -base64 24)/g" .env
sed -i.bak "s/\$(openssl rand -base64 32)/$(openssl rand -base64 32)/g" .env
rm .env.bak
```

### 2. 一键部署

```bash
./deploy-simple.sh
```

### 3. 验证部署

```bash
# 检查服务状态
docker compose ps

# 测试 API
curl http://localhost:8008/_matrix/client/versions

# 查看日志
docker compose logs -f synapse
```

## 配置说明

### 必需的环境变量

```bash
SERVER_NAME=localhost              # 服务器域名
PUBLIC_BASEURL=http://localhost:8008  # 公开访问 URL
POSTGRES_PASSWORD=<强密码>         # 数据库密码
REDIS_PASSWORD=<强密码>            # Redis 密码
ADMIN_SHARED_SECRET=<32字符>      # 管理员密钥
JWT_SECRET=<32字符>               # JWT 密钥
REGISTRATION_SHARED_SECRET=<32字符> # 注册密钥
SECRET_KEY=<32字符>               # 安全密钥
MACAROON_SECRET=<32字符>          # Macaroon 密钥
FORM_SECRET=<32字符>              # 表单密钥
```

### 生成安全密钥

```bash
# 生成所有密钥
openssl rand -base64 32  # 重复执行生成不同密钥
```

## 服务架构

```
┌─────────────┐
│   Nginx     │ :80, :443, :8448
└──────┬──────┘
       │
┌──────▼──────┐
│  Synapse    │ :8008, :9090
└──────┬──────┘
       │
   ┌───┴────┬────────┐
   │        │        │
┌──▼───┐ ┌─▼────┐ ┌─▼──────┐
│ PG   │ │Redis │ │Migrator│
└──────┘ └──────┘ └────────┘
```

## 常用命令

```bash
# 启动服务
docker compose up -d

# 停止服务
docker compose down

# 查看日志
docker compose logs -f synapse

# 重启服务
docker compose restart synapse

# 执行数据库迁移
docker compose up migrator

# 进入容器
docker compose exec synapse sh
```

## 端口说明

| 端口 | 服务 | 说明 |
|------|------|------|
| 8008 | Synapse | Client API |
| 8448 | Nginx | Federation (HTTPS) |
| 9090 | Synapse | Prometheus Metrics |
| 80 | Nginx | HTTP |
| 443 | Nginx | HTTPS |

## 故障排查

### 服务无法启动

```bash
# 查看日志
docker compose logs synapse

# 检查配置
docker compose config

# 验证环境变量
docker compose exec synapse env | grep SYNAPSE
```

### 数据库连接失败

```bash
# 检查数据库状态
docker compose exec postgres pg_isready

# 测试连接
docker compose exec postgres psql -U postgres -d synapse -c "SELECT 1"
```

### Redis 连接失败

```bash
# 检查 Redis 状态
docker compose exec redis redis-cli -a "${REDIS_PASSWORD}" ping
```

## 生产部署建议

1. **使用 HTTPS**: 配置 SSL 证书
2. **备份数据**: 定期备份 PostgreSQL 数据
3. **监控**: 使用 Prometheus 监控指标
4. **日志**: 配置日志轮转
5. **资源限制**: 根据负载调整 CPU 和内存限制

## 更新部署

```bash
# 拉取最新镜像
docker pull vmuser232922/mysynapse:latest

# 重新部署
./deploy-simple.sh
```

---

**部署完成后访问**: http://localhost:8008
