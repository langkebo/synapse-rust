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

## 进阶操作手册（实战要点）

下面是日常运维 / 升级镜像 / 应用迁移时高频踩到的细节，按场景整理。

### Compose 文件叠加顺序

`synapse-rust` 服务在 `docker-compose.yml` 中默认会 `${FEDERATION_PORT:-28448}:8448`
往主机直发布；但生产部署期望 nginx 终结 TLS 后再反代到容器内 8448，所以
`docker-compose.web.yml` 用 `ports: !reset null` 清掉了主机端口映射，并把
`element-web` / nginx 加进来。**任何只用 `-f docker-compose.yml` 的命令都会重新
绑定主机 8448，与 nginx 抢端口启动失败。** 完整命令应当是：

```bash
docker compose \
  --env-file ./docker/.env \
  -f ./docker/docker-compose.yml \
  -f ./docker/docker-compose.override.yml \
  -f ./docker/docker-compose.web.yml \
  up -d
```

> 仅重启 `synapse-rust` 容器（保留 nginx / postgres / redis 不动）：在上面的命
> 令后追加 `--no-deps synapse-rust`。

### 升级镜像（推荐流程）

1. **不要覆盖旧 tag** —— 把 `docker/.env` 里的 `SYNAPSE_IMAGE_TAG` 递增（如
   `0.1.4-amd64 → 0.1.5-amd64`），保留回滚通道。
2. **先打包后切换** —— 在主机为 `arm64` 的 macOS 上构建 `linux/amd64` 必须显式
   指定 platform，否则得到的是宿主架构的镜像即便 tag 写着 `-amd64`：

   ```bash
   docker buildx build \
     --builder amd64builder \
     --platform linux/amd64 \
     --file docker/Dockerfile \
     --target tools \
     --tag vmuser232922/mysynapse:0.1.5-amd64 \
     --tag vmuser232922/mysynapse:latest \
     --load \
     .
   ```

   构建结束后用 `docker image inspect <img> --format '{{.Architecture}}/{{.Os}}'`
   核对一次。
3. **应用新迁移** —— 镜像里 `/app/migrations` 包含全部 SQL；在容器还是旧版本时
   先把新增 SQL 直接灌进 DB（向前兼容，不影响旧实例）：

   ```bash
   docker exec -i synapse-postgres \
     psql -U synapse -d synapse \
     < migrations/<新增的迁移文件>.sql
   ```

4. **再切换镜像**（compose 会重建 `synapse-rust` 一个容器，其他保留）：

   ```bash
   docker compose --env-file ./docker/.env \
     -f ./docker/docker-compose.yml \
     -f ./docker/docker-compose.override.yml \
     -f ./docker/docker-compose.web.yml \
     up -d --no-deps synapse-rust
   ```

5. **验证健康**（同时跑下面三条，全 200 就算正常）：

   ```bash
   for p in /health /_matrix/client/versions /_matrix/federation/v1/version; do
       printf "%-40s %s\n" "$p" \
         "$(curl -sk -o /dev/null -w '%{http_code}' https://matrix.test$p)"
   done
   ```

### 本地访问 `https://matrix.test`

部署里默认配置 `SERVER_NAME=matrix.test` / `PUBLIC_BASEURL=https://matrix.test`，
浏览器/curl 需要把 `matrix.test` 解析到本机：

```bash
sudo sh -c 'echo "127.0.0.1 matrix.test element.test" >> /etc/hosts'
```

nginx 默认证书是自签的，curl 调用要加 `-k`，浏览器首次访问需要忽略证书警告。

### 数据库迁移管理

- **目录布局**：`migrations/` 下成对存放 `*.sql`（apply）和 `*.undo.sql`（rollback），
  文件名格式 `YYYYMMDDNNNNNN_description.sql`，编号在同日内递增。
- **向运行中的 DB 应用单条迁移**：见上面"升级镜像"流程的第 3 步。
- **冷启动 / 全量迁移**：`bash docker/db_migrate.sh migrate`（详见
  `docker/db_migrate.sh` 注释，是迁移真源）。
- **校验 schema**：`bash docker/db_migrate.sh validate`，启动期 `synapse-rust`
  也会自动调用 `run_schema_health_check`，缺关键表/列会停启动。
- **跳过启动期校验**（仅 dev/调试）：`SYNAPSE_SKIP_SCHEMA_CHECK=true`。

### 镜像里包含什么

`docker/Dockerfile` `tools` 阶段最终拷贝到 `/app/`：

| 目录 / 文件        | 说明                                                  |
| ------------------ | ----------------------------------------------------- |
| `/app/synapse-rust`  | release 二进制                                        |
| `/app/healthcheck`   | 健康检查二进制（compose `healthcheck` 调用它）        |
| `/app/migrations/`   | 全量迁移 SQL（含 `*.undo.sql`）                       |
| `/app/scripts/`      | `db_migrate.sh`、`container-migrate.sh` 等运维脚本     |
| `/app/entrypoint.sh` | 启动入口                                              |
| `/app/config_defaults/` | `rate_limit.yaml` 默认值（`docker/config/` 覆盖之）  |

> 镜像目标是 `target: tools`（基于 `debian:bookworm-slim`），保留 shell + 迁移
> 工具，不是最小化的 distroless 运行时。生产可换 `target: runtime` 进一步瘦身，
> 但会失去 `docker exec` 调试能力。

### 推送镜像到 Docker Hub

```bash
# 1. 登录（推荐 stdin 喂密码，避免命令历史泄漏）
echo "$DOCKERHUB_PASSWORD" | docker login -u vmuser232922 --password-stdin

# 2. 推送两个 tag
docker push vmuser232922/mysynapse:0.1.5-amd64
docker push vmuser232922/mysynapse:latest

# 3. 登出（清除本地凭据）
docker logout
```

### 已知坑位汇总

| 现象                                         | 根因                                                         | 处理                                            |
| -------------------------------------------- | ------------------------------------------------------------ | ----------------------------------------------- |
| 启动报 `port 8448 already allocated`         | compose 命令缺 `docker-compose.web.yml`，nginx + 容器抢 8448 | 加上 web override 文件                          |
| 镜像 `0.1.x-amd64` 实际是 arm64              | 在 Apple Silicon 上未指定 `--platform linux/amd64`           | 用 `docker buildx ... --platform linux/amd64`   |
| `cargo check` 报 `Cargo.toml` 找不到         | 当前工作目录不是 `synapse-rust/`                             | 用 `--manifest-path` 显式指定                   |
| `docker compose exec ...` 报服务 not running | `-f` 没带齐 override 文件，compose 视为不存在该服务          | 加全 `-f`，或 `cd docker && docker compose ...` |
| 启动期日志 `Database schema validation FAILED: missing tables` | 新代码引入了未应用的迁移                                     | 先跑 `db_migrate.sh migrate`，再启动            |
| 设备/SAML 配置改完重启丢失                   | 旧镜像没有 `saml_config_overrides` 表                        | 升级到 `≥0.1.5` 并应用对应迁移                  |

---

**部署完成后访问**: `https://matrix.test`（本机 `/etc/hosts` 已映射时）或
`http://localhost:8008`（直连容器、跳过 nginx）。
