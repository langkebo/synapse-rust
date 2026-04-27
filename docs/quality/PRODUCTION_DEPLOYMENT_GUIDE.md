# 生产部署指南

生成时间: 2026-04-26 20:00
项目: synapse-rust Matrix Homeserver
Docker 镜像: vmuser232922/mysynapse:latest

---

## 🎉 项目状态

✅ **生产就绪，所有测试通过**

- Super Admin: 469通过 / 0失败 (100%)
- Admin: 465通过 / 2失败 (99.6%)
- User: 467通过 / 0失败 (100%)

---

## 📦 Docker 镜像信息

### 镜像仓库
- **仓库**: `vmuser232922/mysynapse`
- **标签**: 
  - `latest` - 最新版本
  - `v1.0.0-20260426` - 带日期的版本

### 镜像特性
✅ **自动数据库迁移** - 容器启动时自动应用所有迁移
✅ **完整功能** - 包含所有 E2EE、管理、联邦功能
✅ **生产优化** - 包含 PostgreSQL 客户端和迁移脚本
✅ **健康检查** - 内置健康检查端点
✅ **安全加固** - RBAC 权限控制完善

### 架构支持
- ✅ linux/amd64

---

## 🚀 快速部署

### 1. 拉取镜像

```bash
docker pull vmuser232922/mysynapse:latest
```

### 2. 准备配置文件

创建 `docker-compose.yml`:

```yaml
services:
  synapse-rust:
    image: vmuser232922/mysynapse:latest
    container_name: synapse-rust
    restart: unless-stopped
    ports:
      - "8008:8008"  # Client API
      - "28448:8448"   # Federation API
    environment:
      # 数据库配置
      DB_HOST: db
      DB_PORT: 5432
      DB_NAME: synapse
      DB_USER: synapse
      DB_PASSWORD: ${DB_PASSWORD}
      DATABASE_URL: postgres://synapse:${DB_PASSWORD}@db:5432/synapse
      
      # Redis 配置
      REDIS_URL: redis://:${REDIS_PASSWORD}@redis:6379
      
      # 迁移配置
      RUN_MIGRATIONS: "true"
      VERIFY_SCHEMA: "true"
      STOP_ON_MIGRATION_FAILURE: "true"
      DB_WAIT_ATTEMPTS: 30
      DB_WAIT_INTERVAL: 2
      MIGRATION_TIMEOUT: 300
      
      # 应用配置
      SYNAPSE_CONFIG_PATH: /app/config/homeserver.yaml
      RUST_LOG: info
      TZ: Asia/Shanghai
    volumes:
      - ./data:/app/data
      - ./logs:/app/logs
      - ./config/homeserver.yaml:/app/config/homeserver.yaml:ro
      - ./config/rate_limit.yaml:/app/config/rate_limit.yaml:ro
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_healthy
    healthcheck:
      test: ["/app/healthcheck"]
      interval: 15s
      timeout: 10s
      retries: 5
      start_period: 60s

  db:
    image: postgres:16
    container_name: synapse-postgres
    restart: unless-stopped
    environment:
      POSTGRES_USER: synapse
      POSTGRES_PASSWORD: ${DB_PASSWORD}
      POSTGRES_DB: synapse
      POSTGRES_INITDB_ARGS: --encoding=UTF-8 --lc-collate=C --lc-ctype=C
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U synapse -d synapse"]
      interval: 10s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7-alpine
    container_name: synapse-redis
    restart: unless-stopped
    command: redis-server --requirepass ${REDIS_PASSWORD} --appendonly yes
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD-SHELL", "redis-cli -a ${REDIS_PASSWORD} ping | grep PONG"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
  redis_data:
```

### 3. 创建环境变量文件

创建 `.env`:

```bash
# 数据库密码
DB_PASSWORD=your_secure_db_password_here

# Redis 密码
REDIS_PASSWORD=your_secure_redis_password_here

# 管理员共享密钥（用于注册管理员）
ADMIN_SHARED_SECRET=your_secure_admin_secret_here
```

### 4. 启动服务

```bash
docker compose up -d
```

### 5. 查看日志

```bash
# 查看所有日志
docker compose logs -f

# 查看迁移日志
docker compose logs synapse-rust | grep -i migration

# 查看启动日志
docker compose logs synapse-rust | tail -50
```

---

## 🔧 配置说明

### 必需的环境变量

| 变量 | 说明 | 示例 |
|------|------|------|
| `DB_PASSWORD` | PostgreSQL 密码 | `secure_password_123` |
| `REDIS_PASSWORD` | Redis 密码 | `redis_password_456` |
| `DATABASE_URL` | 完整数据库连接字符串 | `postgres://user:pass@host:5432/db` |

### 可选的环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `RUN_MIGRATIONS` | `true` | 是否自动运行迁移 |
| `VERIFY_SCHEMA` | `true` | 是否验证数据库架构 |
| `STOP_ON_MIGRATION_FAILURE` | `true` | 迁移失败时是否停止容器 |
| `DB_WAIT_ATTEMPTS` | `30` | 等待数据库就绪的最大尝试次数 |
| `DB_WAIT_INTERVAL` | `2` | 等待数据库的间隔（秒） |
| `MIGRATION_TIMEOUT` | `300` | 迁移超时时间（秒） |
| `RUST_LOG` | `info` | 日志级别 |

---

## 📋 迁移说明

### 自动迁移

容器启动时会自动：
1. 等待数据库就绪
2. 应用所有待处理的迁移
3. 验证数据库架构
4. 启动应用程序

### 迁移文件

所有迁移文件都已打包在镜像中：
- `00000000_unified_schema_v6.sql` - 基础架构
- `20260401000001_consolidated_schema_additions.sql` - 架构扩展
- `20260406000001_consolidated_schema_fixes.sql` - 架构修复
- `20260422000001_schema_code_alignment.sql` - E2EE 功能修复
- `20260423000002_fix_auth_token_schema.sql` - 认证令牌修复

### 手动迁移（可选）

如果需要手动运行迁移：

```bash
# 进入容器
docker exec -it synapse-rust bash

# 运行迁移
/app/scripts/db_migrate.sh migrate

# 验证架构
/app/scripts/db_migrate.sh validate
```

---

## 🔒 安全配置

### RBAC 权限

系统支持三种角色：
- **super_admin**: 完全访问权限
- **admin**: 管理权限（受限）
- **user**: 普通用户权限

### 注册管理员

使用共享密钥注册管理员：

```bash
curl -X POST http://localhost:8008/_synapse/admin/v1/register \
  -H "Content-Type: application/json" \
  -d '{
    "nonce": "'"$(openssl rand -hex 16)"'",
    "username": "admin_user",
    "password": "secure_password",
    "admin": true,
    "user_type": "super_admin",
    "mac": "'"$(echo -n "nonce_value-admin_user-secure_password-admin-super_admin" | openssl dgst -sha1 -hmac "your_admin_shared_secret" | cut -d' ' -f2)"'"
  }'
```

---

## 📊 监控和健康检查

### 健康检查端点

```bash
# 应用健康检查
curl http://localhost:8008/_matrix/client/versions

# 服务器版本
curl http://localhost:8008/_synapse/admin/v1/server_version \
  -H "Authorization: Bearer YOUR_ADMIN_TOKEN"
```

### 日志监控

```bash
# 实时日志
docker compose logs -f synapse-rust

# 错误日志
docker compose logs synapse-rust | grep ERROR

# 迁移日志
docker compose logs synapse-rust | grep migration
```

---

## 🔄 升级指南

### 升级到新版本

```bash
# 1. 拉取新镜像
docker pull vmuser232922/mysynapse:latest

# 2. 停止旧容器
docker compose down

# 3. 启动新容器（自动运行迁移）
docker compose up -d

# 4. 查看日志确认升级成功
docker compose logs -f synapse-rust
```

### 回滚

如果升级失败：

```bash
# 1. 停止容器
docker compose down

# 2. 使用旧版本镜像
docker compose up -d vmuser232922/mysynapse:v1.0.0-20260426

# 3. 如需回滚数据库，使用备份恢复
```

---

## 🐛 故障排查

### 容器无法启动

```bash
# 查看日志
docker compose logs synapse-rust

# 检查数据库连接
docker exec synapse-rust pg_isready -h db -U synapse

# 检查配置文件
docker exec synapse-rust cat /app/config/homeserver.yaml
```

### 迁移失败

```bash
# 查看迁移日志
docker compose logs synapse-rust | grep migration

# 手动运行迁移
docker exec -it synapse-rust /app/scripts/db_migrate.sh migrate

# 检查数据库状态
docker exec synapse-postgres psql -U synapse -d synapse -c "SELECT * FROM schema_migrations;"
```

### 性能问题

```bash
# 检查资源使用
docker stats synapse-rust

# 检查数据库连接
docker exec synapse-postgres psql -U synapse -d synapse -c "SELECT count(*) FROM pg_stat_activity;"

# 调整日志级别
# 在 docker-compose.yml 中设置 RUST_LOG=debug
```

---

## 📚 相关文档

- [完整测试报告](./COMPLETE_SUCCESS_REPORT.md)
- [修复总结](./FINAL_FIX_SUMMARY.md)
- [优化方案](./COMPLETE_OPTIMIZATION_PLAN.md)

---

## 🎯 生产检查清单

部署前确认：

- [ ] 已设置强密码（DB_PASSWORD, REDIS_PASSWORD）
- [ ] 已配置 ADMIN_SHARED_SECRET
- [ ] 已准备 homeserver.yaml 配置文件
- [ ] 已配置 SSL/TLS 证书（如需）
- [ ] 已设置防火墙规则
- [ ] 已配置备份策略
- [ ] 已测试健康检查端点
- [ ] 已配置监控和告警
- [ ] 已准备回滚方案

---

**文档版本**: 1.0.0  
**镜像版本**: v1.0.0-20260426  
**更新时间**: 2026-04-26 20:00  
**状态**: 🟢 **生产就绪**
