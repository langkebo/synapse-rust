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

如需在 `split_minimal` 冒烟通过后直接追加一轮 `appservice D2` 基线归档，可使用：

```bash
RUN_APPSERVICE_P0_D2=1 \
APPSERVICE_P0_D2_LABEL=baseline \
APPSERVICE_D2_RESOURCE_SUMMARY="split_minimal baseline; 待补 CPU/RSS/连接池/慢查询摘要" \
bash docker/run_split_minimal_smoke.sh
```

该命令会在自动获取 `ADMIN_TOKEN` 后，继续调用 `scripts/run_appservice_p0_d2.sh`，并默认把样本归档到 `artifacts/appservice/<date>/baseline/`。

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

### 4.1 健康检查端点

```bash
# 基础探活
curl -f http://localhost:8008/_matrix/client/versions

# 数据库连接
curl -f http://localhost:8008/_synapse/admin/v1/health

# Worker 拓扑
curl -f http://localhost:8008/_synapse/worker/v1/topology/validate
```

### 4.2 Prometheus Metrics

#### 4.2.1 指标出口

| 出口 | 端点 | 说明 |
|------|------|------|
| Worker Metrics | `http://127.0.0.1:9091/metrics` | Worker 队列指标 |
| App Metrics | `http://localhost:8008/_synapse/admin/v1/telemetry/metrics` | 应用遥测 |
| AppService Stats（聚合） | `http://localhost:8008/_synapse/admin/v1/appservices/statistics` | 全部应用服务统计 |
| AppService Stats（单 AS） | `http://localhost:8008/_synapse/client/r0/admin/appservice/<id>/statistics` | 单个 AS 统计 |

#### 4.2.2 Worker 关键指标

| 指标名 | 含义 | 典型异常 |
|--------|------|----------|
| `synapse_worker_queue_length` | Worker 任务队列长度 | 持续上涨 → worker 进程异常或消费速度不足 |
| `synapse_worker_consumer_lag` | 消费者滞后（待处理条目） | 持续上涨 → Redis 阻塞或下游 DB 慢 |
| `synapse_worker_consumer_pending` | 消费者 pending 计数 | 长期不归零 → 任务未 ack 或 worker crash |

#### 4.2.3 Prometheus 抓取配置

参考 `docker/prometheus.yml`，最小抓取配置示例：

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: "synapse_app"
    static_configs:
      - targets: ["synapse-rust:8008"]
    metrics_path: /_synapse/admin/v1/telemetry/metrics
  - job_name: "synapse_worker"
    static_configs:
      - targets: ["synapse-rust:9091"]
    metrics_path: /metrics
  - job_name: "otel-collector"
    static_configs:
      - targets: ["otel-collector:8889"]
```

#### 4.2.4 推荐告警规则

```yaml
groups:
  - name: synapse_rust
    rules:
      - alert: WorkerQueueBacklog
        expr: synapse_worker_queue_length > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Worker queue backlog > 1000 for 5m"

      - alert: WorkerConsumerLag
        expr: synapse_worker_consumer_lag > 500
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Worker consumer lag > 500 for 5m"

      - alert: HighHttp5xxRate
        expr: |
          sum(rate(http_requests_total{status=~"5.."}[10m]))
          / sum(rate(http_requests_total[10m])) > 0.01
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "HTTP 5xx rate > 1% for 10m"

      - alert: DbPoolUtilizationHigh
        expr: |
          synapse_db_connections_in_use
          / synapse_db_connections_max > 0.80
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "DB connection pool utilization > 80% for 5m"
```

### 4.3 OpenTelemetry

参考 `docker/docker-compose.otel.yml` 与 `docker/otel-collector-config.yaml`。OTel Collector 默认接收 OTLP（gRPC `:4317` / HTTP `:4318`），并把 traces 导出到 Jaeger、metrics 通过 Prometheus exporter 暴露在 `:8889`。

启用方式（`homeserver.yaml`）：

```yaml
telemetry:
  enabled: true
  metrics_enabled: true
  # tracing_enabled: true
  # otlp_endpoint: http://otel-collector:4317
```

或通过环境变量覆盖：

```bash
SYNAPSE_TELEMETRY__ENABLED=true
SYNAPSE_TELEMETRY__METRICS_ENABLED=true
```

启动完整可观测性栈：

```bash
cd docker
docker compose -f docker-compose.yml -f docker-compose.otel.yml up -d
```

### 4.4 日志配置

#### 4.4.1 RUST_LOG 级别

`tracing` 支持 `error` / `warn` / `info` / `debug` / `trace` 五级，按模块细粒度覆盖：

```bash
# 生产推荐：业务 info，依赖库降为 warn
export RUST_LOG="synapse=info,synapse_worker=info,sqlx=warn,hyper=warn"

# 排障临时提升
export RUST_LOG="synapse=debug,synapse_worker=debug,sqlx=warn"

# 仅看错误
export RUST_LOG="error"
```

#### 4.4.2 日志格式

- 当前为 `tracing` 默认的纯文本输出，**JSON 结构化日志暂未支持**。
- 若需结构化采集，建议在日志代理侧（Filebeat / Vector / Fluentd）做正则解析，或等待 `tracing-subscriber` JSON formatter 落地。

#### 4.4.3 日志轮转

- 裸机：使用 `logrotate` 管理标准输出重定向文件。
- Docker：使用 Docker logging driver（`json-file` + `max-size` / `max-file`，或 `journald` / `fluentd`）。

```bash
# docker-compose.yml 片段
logging:
  driver: json-file
  options:
    max-size: "100m"
    max-file: "10"
```

#### 4.4.4 安全审计日志（security_audit target）

关键安全事件通过 `tracing::warn!(target: "security_audit", ...)` 输出，便于独立采集与告警。当前覆盖：

- OIDC localpart 冲突（builtin/external OIDC 登录时用户名映射冲突）
- Token 撤销（access token / refresh token 主动撤销）
- 管理员动作（管理员注册、用户停用、shadow ban 等）

仅采集安全审计事件：

```bash
export RUST_LOG="security_audit=info"
```

与业务日志合并采集：

```bash
export RUST_LOG="synapse=info,security_audit=info,sqlx=warn"
```

### 4.5 告警阈值参考

| 指标 | 阈值 | 持续时间 | 级别 | 处置 |
|------|------|----------|------|------|
| worker queue length | >1000 | 5min | warning | 检查 worker 进程存活 |
| consumer lag | >500 | 5min | warning | 检查 Redis 连接 |
| HTTP 5xx rate | >1% | 10min | critical | 检查 DB/Redis/上游 |
| DB pool utilization | >80% | 5min | warning | 扩容连接池 |
| replication position drift | >3 cycles | 5min | warning | 检查 worker 拓扑 |
| Redis 内存使用率 | >80% | 5min | warning | 扩容 Redis / 清理冷 key |
| appservice `scheduler_pending_event_count` | >5000 | 5min | warning | 检查 AS 可达性与 transaction backlog |
| 响应时间 p95 | >2000ms | 5min | warning | 检查慢查询与缓存命中率 |

---

## 五、故障定位

### 5.0 故障快速定位表

| 症状 | 可能原因 | 第一步排查 | 相关命令 |
|------|----------|------------|----------|
| 服务无法启动 | DB 连接失败 / schema 校验失败 | 检查日志 | `docker logs synapse-rust` |
| API 返回 500 | DB 连接池耗尽 / Redis 不可达 | 检查 metrics | `curl /_synapse/admin/v1/telemetry/metrics` |
| /sync 超时 | 事件积压 / sync worker 不可达 | 检查 worker 心跳 | `curl /_synapse/worker/v1/topology/validate` |
| 联邦失败 | DNS / TLS / signing key | 检查 federation 日志 | `curl /_matrix/federation/v1/version` |
| 邮件不投递 | SMTP 未配置 / worker 未运行 | 检查 worker 日志 | `docker logs synapse_worker` |
| OIDC 登录失败 | IdP 不可达 / callback URL 不匹配 | 检查 OIDC config | `curl /.well-known/openid-configuration` |
| Worker 任务堆积 | worker 进程挂掉 / Redis 阻塞 | 检查 queue metrics | `curl :9091/metrics` |

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

### 邮件投递 Smoke Test

当启用 email captcha 或邮件通知时，需验证 SMTP 投递链路：

1. **启动 worker 进程**（邮件发送仅在 worker 中执行）：
   ```bash
   SYNAPSE_CONFIG_PATH=homeserver.yaml cargo run --bin synapse_worker --release
   ```

2. **配置 SMTP**（homeserver.yaml）：
   ```yaml
   smtp:
     enabled: true
     host: "smtp.example.com"
     port: 587
     use_tls: true
     username: "noreply@example.com"
     password: "${SMTP_PASSWORD}"
     from: "noreply@example.com"
   ```

3. **触发邮件 captcha**：
   ```bash
   curl -X POST http://localhost:8008/_matrix/client/v3/register/email/requestToken \
     -H "Content-Type: application/json" \
     -d '{"client_secret":"test-secret","email":"test@example.com","send_attempt":1}'
   ```

4. **验证 worker 日志**：worker 日志应显示 `SendEmail` 任务被消费并成功发送。若 SMTP 未配置，captcha service 返回 `not_implemented` 错误而非 panic。

5. **本地测试用 MailHog**：
   ```bash
   docker run -p 1025:1025 -p 8025:8025 mailhog/mailhog
   # 配置 smtp.host=127.0.0.1, smtp.port=1025, smtp.use_tls=false
   ```

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

### 5.6 OIDC / SSO 登录失败

```bash
# 1. 检查 OIDC 配置是否启用
grep -A5 '^oidc:' homeserver.yaml
# 预期: enabled: true, issuer/client_id 非空

# 2. 验证 IdP discovery 文档可达
curl -f <issuer>/.well-known/openid-configuration | jq .issuer
# 预期: 返回的 issuer 与配置一致

# 3. 检查 callback URL 匹配
# IdP 控制台配置的 redirect_uri 必须与 homeserver.yaml 的 callback_url 一致
# 格式: https://<server_name>/_matrix/client/v3/oidc/callback

# 4. 查看 security_audit 日志（localpart 冲突）
docker logs synapse-rust 2>&1 | grep "security_audit"
# 关注: oidc_localpart_collision_refused 事件

# 5. 验证 PKCE 流程
# 确认 IdP 支持 S256 code_challenge_method
curl <issuer>/.well-known/openid-configuration | jq .code_challenge_methods_supported
# 预期: 包含 "S256"
```

**常见 OIDC 问题**:

| 问题 | 原因 | 解决方案 |
|------|------|----------|
| `M_UNAUTHORIZED` localpart collision | IdP 返回的 preferred_username 已被非 OIDC 用户占用 | 使用其他用户名，或手动绑定映射 |
| state expired | auth session 超过 10 分钟 | 重新发起 SSO 登录 |
| PKCE verification failed | code_verifier 与 code_challenge 不匹配 | 检查 IdP 是否正确回传 code_verifier |
| IdP discovery 不可达 | 网络/DNS 问题或 IdP 宕机 | 检查 IdP 状态和网络连通性 |
| builtin OIDC 密钥丢失 | 重启后临时密钥失效 | 配置 `builtin_oidc.signing_key_path` 持久化密钥 |

**生产 IdP 对接测试计划**（P1-11 待完成项）:

1. **单 IdP 压测**: 使用 Keycloak/Auth0，模拟 100 并发 SSO 登录，验证 auth session 一次性消费和 token 交换稳定性
2. **多 IdP 并发**: 配置 2+ external OIDC provider（当前架构仅支持单 issuer，需评估多 IdP 需求）
3. **回调安全验证**: 测试 javascript:/data:/localhost/raw IP 等 unsafe redirect URL 是否被正确拦截
4. **localpart 冲突恢复**: 验证 security_audit 事件触发后，管理员可手动创建 oidc_user_mapping 解决冲突

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

### Soak Test 运行手册

#### 目的
长时间持续验证多实例部署的拓扑稳定性、心跳连续性、replication position 一致性。

#### 前置条件
- 已启动 split_minimal 部署（`docker/run_split_minimal_smoke.sh`）
- admin endpoint 可达（默认 http://127.0.0.1:18008）
- 已获取 admin token

#### 基本运行
```bash
# 运行 1 小时 soak test（默认）
ADMIN_AUTH_HEADER="Bearer <token>" \
ADMIN_ENDPOINT=http://127.0.0.1:18008 \
CLIENT_ENDPOINT=http://127.0.0.1:28008 \
FEDERATION_ENDPOINT=http://127.0.0.1:28448 \
bash scripts/deployment_soak_test.sh
```

#### 自定义参数
| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| SOAK_DURATION_SECONDS | 3600 | 总运行时长（秒） |
| SOAK_INTERVAL_SECONDS | 60 | 检查间隔（秒） |
| SOAK_DRIFT_TOLERANCE | 3 | 连续漂移容忍次数 |
| SOAK_OUTPUT_DIR | (空) | 设置后输出 JSON + Markdown 报告 |

#### 输出报告
设置 `SOAK_OUTPUT_DIR` 后，每个 run 生成：
- `soak_report_<timestamp>.json` — 结构化结果（cycles、checks、drift events）
- `soak_report_<timestamp>.md` — 人类可读摘要（含 per-cycle 表格）

#### 漂移解读
- **topology_drift warn**: worker 数量变化，检查是否有实例重启
- **heartbeat_continuity warn**: worker 心跳 >5min 未更新，检查进程存活
- **replication_position warn**: stream position 缺失，检查 replication 连接
- 连续 drift 超过 `SOAK_DRIFT_TOLERANCE` 次会提前退出

#### 优雅关闭
发送 SIGTERM/SIGINT 可优雅关闭，脚本会完成当前 cycle 后退出（exit code 130）。

---

## 八、相关文档

- Worker 拓扑基线：[WORKER_TOPOLOGY_BASELINE_2026-06-14.md](WORKER_TOPOLOGY_BASELINE_2026-06-14.md)
- AppService 运维手册：[APPSERVICE_OPERATIONS.md](APPSERVICE_OPERATIONS.md)
- Matrix 协议支持面：[SUPPORTED_MATRIX_SURFACE.md](SUPPORTED_MATRIX_SURFACE.md)
- 迁移索引：[MIGRATION_INDEX.md](../db/MIGRATION_INDEX.md)
- 部署指南：[../../docker/deploy/DEPLOY_GUIDE.md](../../docker/deploy/DEPLOY_GUIDE.md)
- 生产部署指南：[../../docker/deploy/PRODUCTION_DEPLOYMENT_GUIDE.md](../../docker/deploy/PRODUCTION_DEPLOYMENT_GUIDE.md)
