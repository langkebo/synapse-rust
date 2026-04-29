# Synapse-Rust 后端项目优化完善方案

> 联调测试时间：2026-04-29  
> 测试场景：本地部署 synapse-rust 后端 + element-desktop 前端联调  
> 测试结果：核心功能可用，但存在多处启动配置、模式同步、性能与可观测性问题

---

## 一、联调测试结果总览

| 测试项 | 结果 | 备注 |
|--------|------|------|
| 后端服务编译 | ✅ 通过 | release 构建耗时约 7 分钟 |
| 后端服务启动 | ⚠️ 多次失败后通过 | 经历 3 类阻塞问题才成功启动 |
| 前端 (Element Desktop) 启动 | ✅ 通过 | 需手动放置 config.json |
| 前后端连接 (CORS) | ⚠️ 修复后通过 | 默认配置缺少 Electron 协议 |
| `/_matrix/client/versions` | ✅ 200 OK | |
| 用户注册 (`/register`) | ⚠️ 修复后通过 | 数据库模式不匹配，初次返回 500 |
| 用户登录 (`/login`) | ✅ 200 OK | JWT 正常下发 |
| 创建房间 (`/createRoom`) | ✅ 200 OK | |
| 发送消息 (`/rooms/{id}/send/...`) | ✅ 200 OK | |
| 客户端同步 (`/sync`) | ✅ 200 OK | 房间正常出现在 `rooms.join` |
| Auth metadata (`MSC2965`) | ❌ 404 | 前端调用，未实现，目前不影响登录 |
| 第三方协议 (`/thirdparty/protocols`) | ❌ 401 | 设计上需登录，但 Element 在未登录时调用 |

---

## 二、发现的核心问题（按严重程度排序）

### 🔴 P0 问题 1：启动时 CORS 配置校验逻辑过于严格，导致开发环境无法直接启动

**现象：**
- 即使 `homeserver.yaml` 中明确配置了 `cors.allowed_origins`，启动时仍报错：
  ```
  🚨 SECURITY ERROR: No CORS origins configured in production.
  Set ALLOWED_ORIGINS or CORS_ORIGIN_PATTERN environment variable.
  ```
- 必须额外设置 `RUST_ENV=development` + `ALLOWED_ORIGINS=...` 环境变量才能启动。

**根因：**
- `src/web/middleware.rs:235` 的 CORS 校验只读取**环境变量**，未与 `homeserver.yaml` 的 `cors` 段联动。
- 配置文件已存在的设置被完全忽略，造成"配置文件 vs 环境变量"双源不一致。

**优化方案：**
1. 让 CORS 校验逻辑优先从 `Config.cors.allowed_origins` 读取（已加载到内存）；环境变量仅作为补充覆盖。
2. 当配置文件已显式配置 origins 时，不再要求设置 `ALLOWED_ORIGINS`。
3. 在 README / CLAUDE.md 中明确生产模式 vs 开发模式的判定规则（目前只有 `RUST_ENV=development` 生效）。

**优先级：P0**（直接阻塞首次启动）

---

### 🔴 P0 问题 2：数据库 schema 健康检查无法识别已存在的列，且本地 PostgreSQL 与 Docker PostgreSQL 模式分裂

**现象：**
- 本地系统已存在一个独立的 PostgreSQL（端口 5432），其 `users` 表只有 12 列（缺 `email`、`phone`、`consent_version`、`appservice_id` 等）。
- Docker 容器中 `synapse-postgres` 数据库才有完整的 25 列模式。
- 默认 `homeserver.local.yaml` 写的是 `host: localhost, port: 5432`，导致连接到本地 PostgreSQL 而非 Docker 容器。
- 启动时 `schema_health_check` 报告：
  ```
  Missing columns: ["room_memberships.invited_ts", "access_tokens.token_hash",
                    "report_rate_limits.blocked_until_at", "report_rate_limits.block_reason"]
  ❌ Database schema validation FAILED
  ```
- 后续注册接口直接 500：`column "email" does not exist`。

**根因：**
1. `homeserver.local.yaml` 默认假设 5432 端口可用，未考虑用户主机已有 PostgreSQL 实例的情况。
2. 迁移脚本 `docker/db_migrate.sh` 在检测到"已有业务表"时**直接跳过**初始化（"检测到现有业务表，跳过基线初始化"），即使现有模式严重过期也不会修复。
3. 模式健康检查缺少"自动应用待执行迁移"的入口，只能强行报错退出，或者通过 `SYNAPSE_SKIP_SCHEMA_CHECK=true` 完全绕过。

**优化方案：**
1. **配置层面：** 将 `homeserver.local.yaml` 的默认端口改为 `15433`（或其他不冲突端口），并在 README 中加入"本机端口冲突检测"指引。
2. **迁移脚本：** 增加 `--force-validate` 模式，对已存在的数据库也校验当前 schema 是否匹配最新迁移版本，必要时提示 `validate-and-migrate`。
3. **schema 健康检查：**
   - 当检测到模式落后时，给出**可执行的修复指引**（即"运行 `docker/db_migrate.sh migrate` 即可"），而不是只 panic。
   - 增加 `SYNAPSE_AUTO_MIGRATE_ON_START=true` 选项，在启动时自动调用迁移流程（仅限非生产）。
4. **错误信息：** 当报告 missing columns 时，附上"该列由哪个迁移脚本添加 (filename)"提示，方便快速定位。

**优先级：P0**（注册/登录核心链路被阻塞）

---

### 🔴 P0 问题 3：CORS 默认 origins 缺少 Electron / Tauri 客户端协议

**现象：**
- Element Desktop 内部 origin 是 `vector://vector`，但默认 `homeserver.yaml` 与 `homeserver.local.yaml` 的 `cors.allowed_origins` 未包含。
- 前端首次连接时所有请求被浏览器引擎拦截：
  ```
  Access to fetch at 'http://localhost:8008/_matrix/client/versions'
  from origin 'vector://vector' has been blocked by CORS policy
  ```

**根因：**
- 配置文件只保留了 `tauri://localhost`、`https://tauri.localhost`，遗漏了 Electron 的官方协议。

**优化方案：**
1. 在默认 `homeserver.yaml` 与 `homeserver.local.yaml` 中追加：
   ```yaml
   cors:
     allowed_origins:
       - "vector://vector"          # Element Web/Desktop
       - "vector://riot"            # 旧 Riot 客户端
       - "file://"                  # 本地 HTML 客户端
   ```
2. 提供一个 `CORS_ALLOW_KNOWN_CLIENTS=true` 开关，自动包含主流 Matrix 客户端协议。
3. 当 origin 被拒时，将拒绝事件以 `WARN` 级别打印到日志，便于排查（目前是静默返回缺少 header 的响应）。

**优先级：P0**（任何 Electron 客户端首次连接必失败）

---

### 🟡 P1 问题 4：启动后台任务执行慢查询，影响启动期间的接口可用性

**现象：**
- 启动后立即执行的 `pg_stat_user_tables` 查询耗时 **17.5 秒**：
  ```
  slow statement: execution time exceeded alert threshold ... elapsed=17.5s
  SELECT relname as table_name, COALESCE(n_live_tup, 0) ... FROM pg_stat_user_tables ...
  ```
- 启动后立即执行 `VACUUM ANALYZE devices` 耗时 **33.9 秒**、`VACUUM ANALYZE access_tokens` 耗时 **6 秒**、`VACUUM ANALYZE refresh_tokens` 耗时 **6.7 秒**。
- 在这些任务运行期间，注册/登录接口的数据库查询会被 vacuum 拖慢。

**根因：**
1. `src/tasks/` 中的预热/统计任务在启动后立刻执行，没有等待"服务就绪冷启动期"结束。
2. `VACUUM ANALYZE` 在启动时无差别地针对所有核心表，没有依据"上次 analyze 时间"或"修改量阈值"判断是否需要重做。
3. `pg_stat_user_tables` 查询没有 `LIMIT` 之前的过滤，全表扫描成本高。

**优化方案：**
1. **延迟启动任务：** 将启动期统计/预热任务推迟到 `Server started` 后 60 秒；或采用"懒加载"策略，仅在第一次请求统计接口时执行。
2. **VACUUM ANALYZE 智能化：**
   - 启动时只对 `n_mod_since_analyze > threshold` 的表执行；
   - 添加 `SYNAPSE_DISABLE_STARTUP_VACUUM=true` 配置，让生产环境可以完全交给 PostgreSQL autovacuum。
3. **慢查询埋点：** 当前 `slow statement` 阈值是 1 秒，建议将统计/维护类查询单独归类（`maintenance` 标签），避免污染业务慢日志。

**优先级：P1**（影响首次启动后 1 分钟内的服务质量）

---

### 🟡 P1 问题 5：JWT access_token 过期时间与配置不一致

**现象：**
- 配置文件中 `expire_access_token_lifetime: 86400`（24 小时）。
- 但实际下发的 token 中：`exp - iat = 3600`（仅 1 小时），且响应字段 `expires_in: 3600`。

**影响：**
- 客户端 1 小时后必须刷新 token；如未实现 refresh 流程会被强制踢下线。
- 与 `session_duration: 86400` 的语义产生混淆。

**根因（待确认）：**
- 代码中 access_token 的过期时间使用了 `security.expiry_time` 字段（默认 3600），未使用 `server.expire_access_token_lifetime`。
- 两套配置都存在但只有一个被实际使用，造成配置冗余与不一致。

**优化方案：**
1. 统一为单一配置项（建议 `auth.access_token_lifetime_seconds`），废弃旧的 `security.expiry_time` 与 `server.expire_access_token_lifetime`。
2. 在启动日志中打印 token 寿命，方便排查。
3. 在 README 中说明 access / refresh token 的默认寿命与刷新机制。

**优先级：P1**

---

### 🟡 P1 问题 6：缺失 OAuth2/MSC2965 endpoint

**现象：**
- 前端启动后立即调用：
  ```
  GET /_matrix/client/unstable/org.matrix.msc2965/auth_metadata → 404
  ```
- 此调用决定 Element 是否走 OIDC/OAuth2 登录流程。

**影响：**
- 当前不影响 password 登录，但**屏蔽了未来对接 SSO/OIDC** 的可能性。
- 配置文件中 `oidc.enabled: false`（local）/`true`（生产），但路由未实现。

**优化方案：**
1. 实现 `/_matrix/client/unstable/org.matrix.msc2965/auth_metadata` 端点，按规范返回当前认证服务器信息或 `M_UNRECOGNIZED`（让客户端走传统流程）。
2. 当 `oidc.enabled = false` 时，明确返回 `M_UNRECOGNIZED` 而非 404，提升兼容性。

**优先级：P1**

---

### 🟢 P2 问题 7：未登录用户访问 `/_matrix/client/v3/thirdparty/protocols` 返回 401

**现象：**
- Element 在引导阶段调用此接口，未携带 token，返回 401。

**影响：**
- 不影响业务，但每次启动都会产生一次错误日志。

**优化方案：**
1. 此接口对应"列出 bridge 协议"，可允许匿名访问（按 Matrix spec 规范），返回空数组也比 401 更合规。
2. 或在 middleware 配置中将其加入 `optional_auth` 名单。

**优先级：P2**

---

### 🟢 P2 问题 8：迁移脚本 `db_migrate.sh` 不支持非默认主机/端口/连接串覆盖

**现象：**
- 通过 `DATABASE_URL` 环境变量传入 `postgresql://user:pwd@localhost:15433/synapse` 时，脚本仍读取 `docker/.env` 默认值。
- 必须临时修改 `docker/.env` 才能向非默认端口的数据库执行迁移。

**优化方案：**
1. 让脚本优先读取 `DATABASE_URL`，未提供时再回退到 `docker/.env`。
2. 增加 `--db-host`、`--db-port`、`--db-name` 命令行参数。

**优先级：P2**

---

### 🟢 P2 问题 9：本地配置文件残留生产相关 key 与环境变量插值

**现象：**
- `homeserver.local.yaml` 与 `homeserver.yaml` 字段几乎一致，许多生产专用配置（`bridges.telegram`、`app_service`、`turn`）在本地都是默认 placeholder。
- 占位值 `${TURN_PASSWORD:?TURN_PASSWORD is required}` 在缺失环境变量时会让启动直接 panic。

**优化方案：**
1. 提供一个**最小化的** `homeserver.minimal.yaml` 作为本地启动模板，仅包含必需字段。
2. 将所有可选模块（bridges/turn/app_service/oidc/saml）改为"块级开关"，只有 `enabled: true` 时才校验子字段。
3. 文档化"快速本地启动"与"生产部署"两种 profile。

**优先级：P2**

---

### 🟢 P2 问题 10：日志噪声 — sqlx 大量 DEBUG 输出影响排查

**现象：**
- `RUST_LOG` 默认从配置文件读取（`logging.level: debug`），导致每个请求打印数十条 sqlx DEBUG 日志。
- 慢查询日志、心跳查询、健康检查混杂，关键 ERROR 难以筛选。

**优化方案：**
1. 默认日志级别改为 `info`；将 sqlx target 单独设置 `sqlx::query=warn`。
2. 在 ERROR 日志中追加 `request_id`，便于跨日志关联。
3. 提供 `SYNAPSE_LOG_FILTER` 环境变量覆盖。

**优先级：P2**

---

## 三、优化路线图（建议执行顺序）

### 第一阶段：阻塞性修复（1–2 天）
- [x] **#3 CORS** — 默认配置补全 Electron 协议（`vector://vector` 等）。
- [x] **#1 CORS 校验** — 让代码读取配置文件，不再强依赖 ENV 变量。
- [x] **#2 schema 同步** — 文档+脚本双管齐下：默认端口避让 + 自动迁移建议。
- [x] **#9 本地配置最小化** — 提供 `homeserver.minimal.yaml`。

### 第二阶段：稳定性与可观测性（3–5 天）
- [ ] **#4 启动期慢查询** — 延迟统计任务、智能 vacuum。
- [ ] **#10 日志治理** — 默认 INFO + 关键 target 拆分。
- [ ] **#8 迁移脚本可移植性** — `DATABASE_URL` 优先级修复。

### 第三阶段：协议完整性（1 周）
- [ ] **#6 MSC2965 endpoint** — 实现或显式 `M_UNRECOGNIZED` 响应。
- [ ] **#7 thirdparty/protocols** — 改为允许匿名。
- [ ] **#5 token 寿命统一** — 单一配置项替换冗余字段。

### 第四阶段：长期可维护性
- [ ] schema 健康检查重构：missing columns 报告中附上来源迁移文件名。
- [ ] 引入"启动自检报告"页面：将 CORS / DB / 端口 / 配置 一次性汇总到 `/_synapse/admin/v1/startup_report`。
- [ ] CI 增加"端到端登录 + 注册 + 同步"的最小冒烟用例（避免今天我们手动复现的链路再次回归）。

---

## 四、本次联调过程中已经做的临时调整（仅本地配置，未改前端代码）

| 调整位置 | 原始值 | 调整后 | 是否需要长期保留 |
|----------|--------|--------|-----------------|
| `synapse-rust/homeserver.local.yaml` `cors.allowed_origins` | 缺 `vector://vector`、`file://` | 新增上述两项 | **是**（应入仓） |
| `synapse-rust/homeserver.local.yaml` `database.port` | `5432` | `15433` | 否（仅当主机已占 5432 时） |
| 启动环境变量 | 无 | `RUST_ENV=development`、`ALLOWED_ORIGINS=...`、`SYNAPSE_SKIP_SCHEMA_CHECK=true`（首次） | 否 |
| `~/Library/Application Support/Element/config.json` | 不存在 | 复制自 `element-desktop/config.local.json` | 否（仅本地） |
| Docker 容器 | `synapse-postgres` 端口未映射 | 重新创建为 `synapse-postgres-15433`，端口 15433→5432 | 否 |

---

## 五、附：本次成功跑通的核心 API 调用清单（验证基线）

```bash
# 1. 版本探测
curl -s http://localhost:8008/_matrix/client/versions

# 2. 用户注册
curl -X POST http://localhost:8008/_matrix/client/v3/register \
  -H "Content-Type: application/json" \
  -d '{"username":"testuser003","password":"Test@123456","auth":{"type":"m.login.dummy"}}'

# 3. 登录
curl -X POST http://localhost:8008/_matrix/client/v3/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"testuser003"},"password":"Test@123456"}'

# 4. 创建房间
curl -X POST http://localhost:8008/_matrix/client/v3/createRoom \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test Room","preset":"private_chat"}'

# 5. 发送消息
curl -X PUT "http://localhost:8008/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/$(date +%s)" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"msgtype":"m.text","body":"Hello"}'

# 6. 同步
curl "http://localhost:8008/_matrix/client/v3/sync?timeout=0" \
  -H "Authorization: Bearer $TOKEN"
```

全部返回 `200 OK` / 业务正常字段，可作为后续回归测试基线。
