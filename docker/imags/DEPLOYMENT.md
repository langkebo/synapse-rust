# 离线部署说明

> **更新时间**: 2026-02-04
> **镜像版本**: synapse-rust:dev (300049d26c35)
> **镜像大小**: 514MB / 113MB (压缩后)

本目录包含已导出的离线镜像包与部署说明，便于在无外网环境快速部署 Synapse Rust。

---

## 📦 镜像信息

| 属性 | 值 |
|------|-----|
| 镜像名称 | synapse-rust:dev |
| 镜像ID | 300049d26c35 |
| 镜像大小 | 514MB |
| 压缩大小 | 108MB |
| 保存时间 | 2026-02-04 13:22 |
| 文件名 | `synapse-rust_dev_20260204_132223.tar` |

---

## 🚀 快速部署

### 1. 导入镜像

```bash
# 切换到imags目录
cd /path/to/docker/imags

# 导入镜像
docker load -i synapse-rust_dev_20260204_132223.tar

# 验证导入
docker images | grep synapse-rust
```

### 2. 启动依赖服务

确保 PostgreSQL 15+ 与 Redis 7.0+ 可用：

```bash
# Docker启动PostgreSQL (可选)
docker run -d \
  --name postgres_synapse \
  -e POSTGRES_USER=synapse_user \
  -e POSTGRES_PASSWORD=synapse_pass \
  -e POSTGRES_DB=synapse \
  -p 5432:5432 \
  postgres:15

# Docker启动Redis (可选)
docker run -d \
  --name redis_synapse \
  -p 6379:6379 \
  redis:7
```

### 3. 环境配置

创建 `.env` 文件：

```bash
# 数据库配置
DATABASE_URL=postgres://synapse_user:synapse_pass@localhost:5432/synapse

# Redis配置
REDIS_URL=redis://localhost:6379

# 服务器配置
SERVER_NAME=your-server.com
HOST=0.0.0.0
PORT=8008
JWT_SECRET=your-jwt-secret-min-32-chars

# 联邦配置 (可选)
FEDERATION_ENABLED=true
SIGNING_KEY=BASE64_32_BYTES_SEED

# CORS配置 (可选)
RUST_ENV=development
ALLOWED_ORIGINS=https://your-domain.com
```

### 4. 启动服务

```bash
# 方式一：使用Docker运行
docker run -d \
  --name synapse_rust \
  --network host \
  -e DATABASE_URL="${DATABASE_URL}" \
  -e REDIS_URL="${REDIS_URL}" \
  -e SERVER_NAME="${SERVER_NAME}" \
  -e HOST="${HOST}" \
  -e PORT="${PORT}" \
  -e JWT_SECRET="${JWT_SECRET}" \
  -e FEDERATION_ENABLED="${FEDERATION_ENABLED:-false}" \
  -e SIGNING_KEY="${SIGNING_KEY}" \
  -e RUST_ENV="${RUST_ENV:-production}" \
  -e ALLOWED_ORIGINS="${ALLOWED_ORIGINS}" \
  -v $(pwd)/config:/app/config \
  -v $(pwd)/media:/data/media \
  synapse-rust:dev

# 方式二：使用Docker Compose (推荐)
# 见 docker/docker-compose.yml
```

---

## ⚙️ 配置说明

### 联邦功能配置

联邦功能依赖 `federation.signing_key` 配置，该字段为 **base64 编码的 32 字节 seed**。

```bash
# 生成签名密钥
openssl rand -base64 32
```

在 `homeserver.yaml` 中配置：

```yaml
federation:
  enabled: true
  signing_key: "BASE64_32_BYTES_SEED_HERE"
```

### CORS安全配置

生产环境应配置允许的来源：

```bash
# 开发环境 (允许所有来源)
RUST_ENV=development

# 生产环境 (配置白名单)
RUST_ENV=production
ALLOWED_ORIGINS=https://app.example.com,https://admin.example.com

# 或使用正则表达式模式
CORS_ORIGIN_PATTERN=^https://.*\.example\.com$
```

---

## 🔧 故障排除

### 端口冲突

```bash
# 检查8008端口占用
lsof -i :8008

# 更换端口
PORT=8080
```

### 数据库连接失败

```bash
# 验证数据库连接
psql -h localhost -U synapse_user -d synapse

# 检查DATABASE_URL格式
echo $DATABASE_URL
```

### 镜像无法启动

```bash
# 查看日志
docker logs synapse_rust

# 重新导入镜像
docker rmi synapse-rust:dev
docker load -i synapse-rust_dev_20260204_132223.tar
```

---

## 📋 验证清单

- [ ] 镜像成功导入
- [ ] PostgreSQL连接正常
- [ ] Redis连接正常
- [ ] 服务启动成功
- [ ] API端点可访问 (`http://localhost:8008/_matrix/client/versions`)
- [ ] CORS配置正确 (如需要)
- [ ] 联邦功能正常 (如启用)

---

## 📚 相关文档

- 项目文档: `docs/`
- 测试结果: `docs/TEST_RESULTS_SUMMARY.md`
- API文档: `docs/api-SDK/`
- GitHub仓库: https://github.com/langkebo/synapse

---

## 📝 版本历史

| 日期 | 镜像版本 | Git提交 | 说明 |
|------|---------|---------|------|
| 2026-02-04 | dev (300049d) | d41ae76 | 代码质量优化与清理 |
| 2026-02-01 | dev | 750c9da | 初始优化版本 |

---

**问题反馈**: 请在 GitHub Issues 中报告: https://github.com/langkebo/synapse/issues
