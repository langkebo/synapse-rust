# synapse-rust 本地体验改进

> 一个集中修复"开箱即用"体验缺陷的 PR：把"我能不能在 5 分钟内跑通后端 + 让 Element Desktop 连上"从需要一串环境变量、跳过 schema 校验、修改硬编码的过程，简化成一行 `cargo run`。

---

## 背景

在一次 `synapse-rust` 后端 + `element-desktop` 前端的本地联调里，我们记录到 10 个会阻塞或拖慢首跑体验的问题，跨越启动校验、CORS、schema 检查、维护任务、JWT、协议端点、日志噪声、迁移脚本等多个层面。

这个 PR 把这 10 项一次性修掉，**不引入任何新功能**，所有改动都向后兼容（旧的 `ALLOWED_ORIGINS` 环境变量、旧的 `AuthService::new` 入口都仍然可用）。

修复后的体验：

```bash
# 一次性拉起 Postgres
docker run -d --name synapse-postgres-15433 \
  -e POSTGRES_USER=synapse -e POSTGRES_PASSWORD=synapse -e POSTGRES_DB=synapse \
  -p 15433:5432 postgres:16

# 应用迁移（脚本现在会从 DATABASE_URL 自动反推 host/port）
DATABASE_URL="postgres://synapse:synapse@localhost:15433/synapse" \
  bash docker/db_migrate.sh migrate

# 启动后端
SYNAPSE_CONFIG_PATH=homeserver.minimal.yaml \
  cargo run --release --bin synapse-rust

# 启动前端 — 不需要任何额外配置
cd ../element-desktop && pnpm start
```

---

## 改动汇总

按问题严重程度分三组，每组都附复现命令与修复后的预期。

### 🔴 P0 — 阻塞首次启动

| # | 问题 | 修复 |
|---|------|------|
| **P0-1** | `homeserver.yaml` 已配置 `cors.allowed_origins`，但启动仍报 `🚨 SECURITY ERROR: No CORS origins configured`，必须额外设置 `ALLOWED_ORIGINS` 环境变量 | CORS 校验现在会先读 `Config.cors.allowed_origins`，环境变量仅作为补充覆盖。校验失败时的提示同时给出"改 yaml"和"改 ENV"两种修法 |
| **P0-2** | schema 健康检查发现缺列时只 panic，不告诉运维下一步该怎么做；本地环境 5432 端口常被系统级 PostgreSQL 占用，导致后端连到错误的库 | 失败信息包含可直接复制粘贴的 `DATABASE_URL=... bash docker/db_migrate.sh migrate` 修复指引；`homeserver.local.yaml` 默认改为 `15433` 端口避让，并加注释说明原因 |
| **P0-3** | 默认 `cors.allowed_origins` 缺少 `vector://vector`、`file://`，Element Desktop 首次连接时所有请求被 CORS 拦截 | `homeserver.yaml`、`docker/config/homeserver.yaml` 默认补全 `vector://vector`、`vector://riot`、`file://` |

### 🟡 P1 — 影响稳定性与协议兼容

| # | 问题 | 修复 |
|---|------|------|
| **P1-4** | `tokio::time::interval` 首次 `tick()` 立即触发 → 启动后立刻执行 `VACUUM ANALYZE` 7 张表（最慢 33.9s）+ `pg_stat_user_tables` 全表扫描（17.5s），拖慢首批用户请求 | 引入 `STARTUP_GRACE_PERIOD = 60s`、`MAINTENANCE_STARTUP_DELAY = 300s`；`vacuum_analyze` 现在先查 `n_mod_since_analyze`，<1000 修改的表跳过 |
| **P1-5** | 配置里 `server.expire_access_token_lifetime: 86400` 完全没生效，实际下发的 token 过期时间只有 1 小时（来自 `security.expiry_time: 3600`） | 新增 `Config::access_token_lifetime_seconds()` 帮助函数，优先使用 `server.expire_access_token_lifetime`，两个字段不一致时打印 WARN；`AuthService::new_with_lifetime` 让调用方显式传入 canonical 寿命，旧入口保持兼容 |
| **P1-6** | 前端启动时调用 `/_matrix/client/unstable/org.matrix.msc2965/auth_metadata`，返回 `404`（路由不存在），客户端无法判断是否走 OIDC | 实现该 endpoint：OIDC 未启用时按规范返回 `400 + M_UNRECOGNIZED`，让客户端正确回退到密码登录；OIDC 启用时返回完整 metadata |

### 🟢 P2 — 可维护性

| # | 问题 | 修复 |
|---|------|------|
| **P2-7** | 未登录调用 `/_matrix/client/v3/thirdparty/protocols` 返回 401（每次冷启动都会产生一条错误日志） | 改为允许匿名访问。该端点返回的是服务器级元数据，不涉及用户隐私，与 synapse-python 行为一致 |
| **P2-8** | `docker/db_migrate.sh` 通过 `DATABASE_URL` 调用时，`DB_HOST` / `DB_PORT` / `DB_USER` 仍来自 `.env`，导致 docker 容器探测和密码屏蔽用错值 | 启动时若 `DATABASE_URL` 已显式提供，反向解析回各 `DB_*` 字段并打印 `使用调用方提供的 DATABASE_URL（覆盖 .env 默认值）` |
| **P2-9** | 缺少一份"最小可启动"的本地配置；`homeserver.local.yaml` 仍残留 OIDC/SAML/SMTP/TURN 等模块 placeholder | 新增 `homeserver.minimal.yaml`：仅保留必需字段，所有可选模块默认 `enabled: false`，无任何 `${VAR:?...}` 占位符，开箱即用 |
| **P2-10** | `logging.level: debug` 时每条 SQL 都被打印 2 次（`query.summary` + `db.statement`），淹没业务日志 | 当 `RUST_LOG` 未设置且配置级别为 `trace`/`debug` 时，自动追加 `sqlx::query=warn,sqlx_core=warn,hyper=info,tower_http::trace=info` 噪声压制规则；运维仍可通过 `RUST_LOG` 完全覆盖 |

---

## 文件清单

```
新增 (1)
  homeserver.minimal.yaml             "开箱即用"本地配置 profile

配置 (3)
  homeserver.yaml                     P0-3
  homeserver.local.yaml               P0-2 注释 + P0-3
  docker/config/homeserver.yaml       P0-3

脚本 (1)
  docker/db_migrate.sh                P2-8

代码 (11)
  src/auth/mod.rs                     P1-5  (new_with_lifetime 入口)
  src/common/config/mod.rs            P1-5  (access_token_lifetime_seconds)
  src/common/logging.rs               P2-10 (sqlx 噪声压制)
  src/server.rs                       P0-1 + P0-2
  src/services/container.rs           P1-5
  src/storage/maintenance.rs          P1-4  (智能 vacuum)
  src/tasks/mod.rs                    P1-4  (启动延迟)
  src/web/middleware.rs               P0-1  (set_config_allowed_origins)
  src/web/routes/admin/register.rs    P1-5
  src/web/routes/assembly.rs          P1-6  (MSC2965 endpoint)
  src/web/routes/thirdparty.rs        P2-7
```

合计 **+~280 / -~50** 行（不含 `homeserver.minimal.yaml` 新文件）。

---

## 兼容性

- `AuthService::new` 旧签名保留，行为不变（仍读 `security.expiry_time`），新增 `new_with_lifetime` 不破坏外部调用。
- `Config::access_token_lifetime_seconds()` 在两个字段一致或仅一个被设置时无副作用；不一致时仅打印 WARN，不阻塞启动。
- `ALLOWED_ORIGINS` / `CORS_ORIGIN_PATTERN` 环境变量优先级保持不变，只是不再是**唯一**来源。
- 启动期任务延迟可通过未来加 `tasks.startup_grace_period_seconds` 配置项进一步可调（本 PR 不引入新配置项，先使用合理默认值）。

---

## 验证

启动后端（仅一行命令，无环境变量）：

```bash
SYNAPSE_CONFIG_PATH=homeserver.minimal.yaml cargo run --release --bin synapse-rust
```

期望日志关键行：

```
║  🌐 CORS Origins:
║    - vector://vector
║    - file://
║  ✅ CORS configuration looks secure
✅ Database schema validation PASSED
Listening on (Client API): 127.0.0.1:8008
```

冒烟链路：

```bash
# versions
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8008/_matrix/client/versions
# → 200

# MSC2965（OIDC 未启用时正确回退）
curl -s http://localhost:8008/_matrix/client/unstable/org.matrix.msc2965/auth_metadata | jq .errcode
# → "M_UNRECOGNIZED"

# thirdparty/protocols（不再 401）
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8008/_matrix/client/v3/thirdparty/protocols
# → 200

# 注册 → token 寿命应为 86400（来自 server.expire_access_token_lifetime）
curl -s -X POST http://localhost:8008/_matrix/client/v3/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"smoke","password":"Test@12345","auth":{"type":"m.login.dummy"}}' \
  | jq .expires_in
# → 86400

# Electron 协议 CORS preflight（不再被拦截）
curl -s -X OPTIONS http://localhost:8008/_matrix/client/versions \
  -H 'Origin: vector://vector' \
  -H 'Access-Control-Request-Method: GET' \
  -o /dev/null -w "%{http_code} %header{access-control-allow-origin}\n"
# → 200 vector://vector

# 启动后 60 秒内日志中应无 VACUUM 慢查询条目
grep "slow statement.*VACUUM" synapse.log | wc -l
# → 0
```

完整业务回归（注册 → 登录 → 创建房间 → 发消息 → 同步）已确认无回归。

---

## 不在本 PR 范围

- schema 健康检查的"指明缺失列由哪个迁移文件添加"映射（已记录在 `BACKEND_OPTIMIZATION_PLAN.md` 的"长期改进"节）
- `tasks.startup_grace_period_seconds` 配置项化（本 PR 用编译期常量）
- `/_synapse/admin/v1/startup_report` 一站式自检页面
- 真正的 OIDC 实现（本 PR 仅修了 endpoint 缺失）

---

## 提交结构建议

如需拆分为多个 commit：

```
fix(cors): respect cors.allowed_origins from homeserver.yaml      (P0-1, P0-3)
fix(schema): provide actionable migration hint on validation fail (P0-2)
perf(tasks): defer expensive periodic jobs past cold-start window (P1-4)
fix(auth): honor server.expire_access_token_lifetime              (P1-5)
feat(routes): implement MSC2965 auth_metadata endpoint            (P1-6)
fix(routes): allow anonymous access to thirdparty/protocols       (P2-7)
fix(scripts): db_migrate.sh respects DATABASE_URL                 (P2-8)
docs(config): add homeserver.minimal.yaml profile                 (P2-9)
chore(logging): suppress sqlx noise at debug level                (P2-10)
```

或保留为单 commit：

```
chore: streamline local-development boot experience

Fixes ten "first-time setup" papercuts so the server can start with
nothing more than `SYNAPSE_CONFIG_PATH=homeserver.minimal.yaml cargo run`,
and Element Desktop can connect with no environment overrides.
```
