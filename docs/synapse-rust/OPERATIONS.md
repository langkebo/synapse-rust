# synapse-rust 运维手册

> 版本: v1.0
> 日期: 2026-06-18
> 基线: 对标上游 `element-hq/synapse` 的安装、升级、反向代理、故障定位结构
> 维护: 运维负责人 + 架构负责人

---

## 一、安装与部署

### 1.1 环境要求

| 组件 | 最低版本 | 说明 |
|------|----------|------|
| Rust | 1.93.0 | 编译工具链 |
| PostgreSQL | 14+ | 主数据库 |
| Redis | 7+ | 缓存与任务队列 |
| Docker | 24+ | 容器化部署（可选） |

### 1.2 Docker Compose 部署（推荐）

```bash
cd docker
docker compose up -d --build
```

验证服务：

```bash
curl -f http://localhost:8008/_matrix/client/versions
```

### 1.3 裸机部署

```bash
# 1. 启动 PostgreSQL 与 Redis
# 2. 配置 homeserver.yaml
export SYNAPSE_CONFIG_PATH=homeserver.yaml

# 3. 执行数据库迁移
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate

# 4. 启动服务
cargo run --release
```

### 1.4 Worker 模式部署

使用 `split_minimal` 拓扑拆分为 master + background worker：

```bash
cd docker
docker compose -f docker-compose.split-minimal.yml up -d --build
```

详细 worker 拓扑说明见 [WORKER_TOPOLOGY_BASELINE_2026-06-14.md](WORKER_TOPOLOGY_BASELINE_2026-06-14.md)。

### 1.5 配置文件

| 配置项 | 说明 | 默认值 |
|--------|------|--------|
| `server.name` | Matrix 服务器域名 | 必填 |
| `server.server_name` | 联邦标识 | 继承 `server.name` |
| `database.host` | PostgreSQL 地址 | `localhost` |
| `redis.host` | Redis 地址 | `localhost` |
| `smtp.enabled` | 是否启用 SMTP | `false` |
| `search.enabled` | 是否启用 Elasticsearch | `false` |

环境变量覆盖（`SYNAPSE_` 前缀，`__` 表示层级）：

```bash
SYNAPSE_DATABASE__HOST=db.example.com
SYNAPSE_REDIS__HOST=cache.example.com
SYNAPSE_SMTP__ENABLED=true
```

---

## 二、升级

### 2.1 升级流程

1. **备份数据库**：`bash scripts/backup_database.sh`
2. **拉取新版本**：`git pull && cargo build --release`
3. **停止旧服务**：`docker compose down` 或停止裸机进程
4. **执行数据库迁移**：`bash docker/db_migrate.sh migrate`
5. **验证迁移**：`bash docker/db_migrate.sh validate`
6. **启动新服务**：`docker compose up -d --build` 或 `cargo run --release`
7. **冒烟验证**：`bash scripts/deployment_smoke_test.sh`

### 2.2 回滚

1. **停止服务**
2. **恢复数据库备份**
3. **回滚代码**：`git checkout <previous_tag>`
4. **重新编译**：`cargo build --release`
5. **启动服务**

### 2.3 迁移说明

- 迁移入口统一为 `docker/db_migrate.sh migrate`
- 基线迁移文件：`migrations/00000000_unified_schema_v7.sql`
- 增量迁移按时间戳命名：`migrations/YYYYMMDDHHMMSS_description.sql`
- 迁移索引：`migrations/MIGRATION_INDEX.md`
- 服务启动默认只执行 schema health check，不做运行时迁移
- 回滚脚本命名：`migrations/YYYYMMDDHHMMSS_description.undo.sql`

---

## 三、反向代理配置

### 3.1 Nginx 基础配置

```nginx
server {
    listen 443 ssl http2;
    server_name matrix.example.com;

    ssl_certificate     /etc/ssl/matrix.example.com.crt;
    ssl_certificate_key /etc/ssl/matrix.example.com.key;

    # 客户端 API
    location /_matrix {
        proxy_pass http://127.0.0.1:8008;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /_synapse {
        proxy_pass http://127.0.0.1:8008;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
    }
}
```

### 3.2 Split-Minimal Worker 反向代理

```nginx
upstream synapse_master {
    server 127.0.0.1:8008;
}

upstream synapse_background {
    server 127.0.0.1:8009;
}

server {
    listen 443 ssl http2;
    server_name matrix.example.com;

    # Matrix 客户端 API → master
    location /_matrix/client {
        proxy_pass http://synapse_master;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
    }

    # 后台任务 → background worker
    location /_synapse/worker {
        proxy_pass http://synapse_background;
        proxy_set_header Host $host;
    }
}
```

完整示例：`docker/nginx/split-minimal.conf`

---

## 四、监控与告警

### 4.1 健康检查

```bash
# 基础探活
curl -f http://localhost:8008/_matrix/client/versions

# 数据库连接
curl -f http://localhost:8008/_synapse/admin/v1/health

# Worker 拓扑
curl -f http://localhost:8008/_synapse/worker/v1/topology/validate
```

### 4.2 Metrics

| 出口 | 端点 | 说明 |
|------|------|------|
| Worker Metrics | `http://127.0.0.1:9091/metrics` | Worker 队列指标 |
| App Metrics | `http://localhost:8008/_synapse/admin/v1/telemetry/metrics` | 应用遥测 |
| AppService Stats | `http://localhost:8008/_synapse/admin/v1/appservices/statistics` | 应用服务统计 |

### 4.3 关键告警阈值

| 指标 | 告警阈值 | 说明 |
|------|----------|------|
| `synapse_worker_queue_length` | > 1000 | 任务队列积压 |
| `synapse_worker_consumer_lag` | > 500 | 消费者滞后 |
| DB 连接池使用率 | > 80% | 数据库连接池压力 |
| Redis 内存使用率 | > 80% | Redis 缓存压力 |
| appservice `scheduler_pending_event_count` | > 5000 | AS 事件积压 |
| 响应时间 p95 | > 2000ms | API 延迟升高 |

### 4.4 日志

```bash
# 提高日志级别
export RUST_LOG=info,synapse_rust=debug

# JSON 格式日志（生产环境）
export RUST_LOG=info,synapse_rust=info
# 在 homeserver.yaml 中配置 tracing 输出格式
```

---

## 五、故障定位

### 5.1 服务无法启动

| 症状 | 排查步骤 |
|------|----------|
| 编译失败 | `cargo check --workspace --all-features --locked` |
| 配置错误 | `bash scripts/validate_config.sh` |
| 数据库连接失败 | 检查 `database.host` / `database.port`，确认 PostgreSQL 运行中 |
| Redis 连接失败 | 检查 `redis.host` / `redis.port`，确认 Redis 运行中 |
| 迁移失败 | `bash docker/db_migrate.sh validate`，检查迁移日志 |
| Schema 不完整 | 查看启动日志中的 `schema_health_check` 输出 |

### 5.2 API 返回 500

```bash
# 1. 查看服务日志
docker compose logs synapse-rust | tail -100

# 2. 检查数据库连接
curl -f http://localhost:8008/_synapse/admin/v1/health

# 3. 检查 Redis 连接
bash scripts/collect_redis_observability.sh

# 4. 检查数据库状态
bash scripts/collect_pg_observability.sh
```

### 5.3 联邦不工作

| 症状 | 排查 |
|------|------|
| 无法邀请外域用户 | 检查 `server.server_name` 配置，确认 `.well-known` 正确 |
| 外域事件无法同步 | 检查 `/_synapse/worker/v1/topology/validate` 中 federation 相关 worker 状态 |
| 签名验证失败 | 检查 `security.secret` 与服务端 TLS 证书 |

### 5.4 邮件发送失败

| 症状 | 排查 |
|------|------|
| 验证码邮件未收到 | 检查 `smtp.enabled=true`，确认 SMTP 配置正确 |
| Worker 未处理邮件 | 检查 `background` worker 运行状态，确认 Redis 任务队列连通 |
| SMTP 连接失败 | 检查 `smtp.host` / `smtp.port` / `smtp.tls`，测试网络连通性 |

### 5.5 性能问题

```bash
# 1. 检查数据库索引
bash scripts/collect_pg_observability.sh

# 2. 检查缓存命中率
bash scripts/collect_redis_observability.sh

# 3. 检查任务队列积压
curl http://localhost:9091/metrics | grep synapse_worker

# 4. 运行基准测试
bash scripts/run_benchmarks.sh
```

---

## 六、备份与恢复

### 6.1 数据库备份

```bash
bash scripts/backup_database.sh
```

### 6.2 数据库恢复

```bash
# 1. 停止服务
# 2. 恢复备份
psql -U synapse_user -d synapse_db < backup.sql
# 3. 启动服务
```

### 6.3 媒体文件备份

```bash
# 备份 media 目录
tar -czf media_backup.tar.gz /path/to/media_store/
```

---

## 七、运维脚本索引

| 脚本 | 用途 |
|------|------|
| `scripts/dev_start.sh` | 一键启动开发环境 |
| `scripts/generate_env.sh` | 生成环境变量配置 |
| `scripts/validate_config.sh` | 验证配置文件 |
| `scripts/backup_database.sh` | 备份数据库 |
| `scripts/deployment_smoke_test.sh` | 部署冒烟测试 |
| `scripts/deployment_soak_test.sh` | 部署持续验证 |
| `scripts/ci_backend_validation.sh` | CI 后端验证 |
| `scripts/run_ci_tests.sh` | 运行 CI 测试 |
| `scripts/collect_pg_observability.sh` | 收集 PostgreSQL 可观测性数据 |
| `scripts/collect_redis_observability.sh` | 收集 Redis 可观测性数据 |
| `docker/db_migrate.sh` | 数据库迁移入口 |
| `docker/run_split_minimal_smoke.sh` | Split-minimal 部署冒烟 |

---

## 八、相关文档

- Worker 拓扑基线：[WORKER_TOPOLOGY_BASELINE_2026-06-14.md](WORKER_TOPOLOGY_BASELINE_2026-06-14.md)
- AppService 运维手册：[APPSERVICE_OPERATIONS.md](APPSERVICE_OPERATIONS.md)
- Matrix 协议支持面：[SUPPORTED_MATRIX_SURFACE.md](SUPPORTED_MATRIX_SURFACE.md)
- 迁移索引：[MIGRATION_INDEX.md](../db/MIGRATION_INDEX.md)
- 部署指南：[../../docker/deploy/DEPLOY_GUIDE.md](../../docker/deploy/DEPLOY_GUIDE.md)
- 生产部署指南：[../../docker/deploy/PRODUCTION_DEPLOYMENT_GUIDE.md](../../docker/deploy/PRODUCTION_DEPLOYMENT_GUIDE.md)