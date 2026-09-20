# synapse-rust

用 Rust 实现的 Matrix Homeserver（当前处于能力收敛、测试语义校正与证据对齐阶段）。

## 🚀 快速开始

### 方式一：一键启动（推荐）

```bash
./scripts/dev_start.sh
```

该脚本会自动：
- 检查并生成 `.env` 文件
- 验证环境变量配置
- 启动 Docker 服务

### 方式二：Docker Compose

```bash
# 1. 生成环境变量
./scripts/generate_env.sh > .env

# 2. 启动服务
cd docker
docker compose up -d --build
```

### 验证服务

```bash
curl -f http://localhost:8008/_matrix/client/versions
```

## 📋 环境变量配置

项目使用环境变量管理所有配置。详细说明请参考：

- 📖 [环境变量配置](#环境变量覆盖配置)
- 🔧 [配置验证脚本](scripts/validate_config.sh)
- 🔑 [密钥生成脚本](scripts/generate_env.sh)

### 必需的环境变量

```bash
# 加密密钥（生成：openssl rand -hex 32）
OLM_PICKLE_KEY=<64位十六进制>

# 数据库和服务密钥
SYNAPSE_DB_PASSWORD=<强密码>
SYNAPSE_JWT_SECRET=<至少32字符>
SYNAPSE_MACAROON_SECRET=<至少32字符>
SYNAPSE_FORM_SECRET=<至少32字符>
SYNAPSE_REGISTRATION_SECRET=<至少32字符>
SYNAPSE_SECURITY_SECRET=<至少32字符>
```

### 快速生成所有密钥

```bash
./scripts/generate_env.sh > .env
# 编辑 .env 文件根据需要调整配置
```

## 功能概览

- Matrix / Synapse 相关能力已广泛铺设，当前以“代码证据 + 测试证据 + CI 语义”持续收敛
- PostgreSQL 持久化（`sqlx`）
- Redis 缓存
- 可选 Elasticsearch 搜索（用于私聊消息搜索）
- Docker Compose 一键部署（synapse + postgres + redis + nginx）

## 快速开始（Docker）

仓库有两个 compose 栈，**服务集合与用途不同，故意并存**，但读的是同一份配置：

| 栈 | 文件 | 用途 |
|---|---|---|
| 开发/CI | `docker/docker-compose.yml` | 就地 `build`、服务名 `synapse-rust`/`db`/`redis`、带容器 entrypoint 迁移、无 nginx。`backend-validation` 与 `e2ee-interop` 两个 CI 工作流按这些服务名起栈 |
| 生产 | `docker/deploy/docker-compose.yml` | 全栈（postgres/redis/**migrator**/synapse/nginx），80/443/8448 + SSL，只读根文件系统，用 `docker/deploy/deploy.sh` 部署 |

开发栈：

```bash
cd docker
docker compose up -d --build
```

默认情况下，容器入口会在应用启动前自动调用统一迁移入口 `docker/db_migrate.sh migrate`。如果需要手动执行或复核迁移，请统一使用：

```bash
cd docker
docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust migrate
docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust validate
```

验证服务：

```bash
curl -f http://localhost:8008/_matrix/client/versions
```

### 配置文件

- **唯一真相源**：`docker/config/`（`homeserver.yaml`、`rate_limit.yaml`、`postgres.conf`、`appservices/`）
- 两个 compose 栈都挂载这一份（生产栈用 `../config`，开发栈用 `./config`），`docker/Dockerfile` 也从同一路径打进镜像；**不存在第二份副本**
- 可通过环境变量覆盖配置（见下方 “环境变量”）

注意：仓库内的示例配置包含示例域名与示例密钥，部署前务必替换：

- `server.name`
- `security.secret`
- 以及数据库/Redis 的账号密码与访问策略

### Elasticsearch（可选）

当前配置结构需要包含 `search` 字段；如果不使用 ES，也需要显式禁用：

```yaml
search:
  elasticsearch_url: "http://localhost:9200"
  enabled: false
```

## 本地运行（Rust）

要求：本地已启动 PostgreSQL 与 Redis。

```bash
export SYNAPSE_CONFIG_PATH=homeserver.yaml
# 迁移必须显式给出目标（见下方「统一口径」）：这里指向你本地那台 PostgreSQL
export DATABASE_URL=postgres://synapse:synapse@localhost:5432/synapse
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
cargo run --release
```

统一口径：

- 部署与升级的唯一迁移执行入口是 `docker/db_migrate.sh {init|migrate|status|validate}`；仓库里不存在第二套迁移执行路径
- 该入口必须能确定目标：只认 `DATABASE_URL`，或 `DB_HOST`/`DB_PORT`/`DB_NAME`/`DB_USER`/`DB_PASSWORD`。裸跑会落到 `docker/.env` 的兜底值 `localhost:5432`，而两个 compose 栈都不把 5432 发布到宿主 —— 那个端口上是宿主自装的 PostgreSQL，脚本因此在动手前就拒绝执行（H-14）；确实要打宿主实例时显式放行 `SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1`
- 新环境统一以 `migrations/00000000_unified_schema_v12.sql` 作为**唯一**基线；目录下不存在时间戳增量文件、扩展文件或 `.undo.sql` 回滚文件（`migrations/README.md` 是这条约定的权威说明）
- Docker 容器只是由入口脚本自动调用该迁移入口，不构成第二套迁移方案
- 服务启动默认只执行 schema health check，不执行运行时迁移
- 仅在显式开启 `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 且未设置 `SYNAPSE_SKIP_DB_INIT` 时，才进入运行时兼容初始化路径
- CI 会在构建前执行 `scripts/check_migration_consistency.py`，并统一通过 `docker/db_migrate.sh`（psql 逐文件、autocommit）把 `migrations/` 应用到干净库以检测 schema 漂移

## 环境变量（覆盖配置）

配置读取逻辑：优先读配置文件（`SYNAPSE_CONFIG_PATH` 指定），并支持 `SYNAPSE_` 前缀的环境变量覆盖（使用 `__` 表示层级）。

- `SYNAPSE_CONFIG_PATH`：配置文件路径（默认 `homeserver.yaml`）
- `SYNAPSE_DATABASE__HOST` / `SYNAPSE_DATABASE__PORT` / `SYNAPSE_DATABASE__USERNAME` / `SYNAPSE_DATABASE__PASSWORD` / `SYNAPSE_DATABASE__NAME`
- `SYNAPSE_REDIS__HOST` / `SYNAPSE_REDIS__PORT` / `SYNAPSE_REDIS__ENABLED`
- `SYNAPSE_SEARCH__ELASTICSEARCH_URL` / `SYNAPSE_SEARCH__ENABLED`
- `RUST_LOG`：日志过滤（例：`info,synapse_rust=debug`）

## 文档

- 路由契约清单（权威）：`docs/synapse-rust/ROUTE_CONTRACT.md`
- API 覆盖率报告（vs Synapse v1.153.0）：`docs/synapse-rust/API_COVERAGE_REPORT.md`
- 上游 Synapse 能力差距分析：`docs/synapse-rust/ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md`
- 依赖升级追踪：`docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md`
- 管理员注册指南：`docs/synapse-rust/admin-registration-guide.md`
- 代码审查报告（最新）：`artifacts/code_review_report_2026-08-11.md`
- 文档索引：`docs/INDEX.md`
- 测试语义与 CI 门禁：`TESTING.md`
- API 文档：由路由 ledger 生成，见 `docs/openapi/client.yaml`（用 `scripts/api_test/generate_openapi.py` 重新生成）
- 数据库迁移指引：`migrations/README.md`

## 私密聊天功能集成指南 (Private Chat Features)

本项目对标准 Matrix 协议进行了增强，支持高隐私的私密聊天功能。前端无需调用额外的专有 API，只需遵循标准 Matrix 规范并使用特定的配置即可自动激活。

### 1. 启用私密聊天 (Trusted Private Chat)

创建房间时，通过指定 `preset` 为 `trusted_private_chat`，后端将自动配置一系列高隐私保护策略。

**前端实现：**

```javascript
// 创建私密聊天
client.createRoom({
    preset: "trusted_private_chat", // 关键：激活私密模式
    visibility: "private",
    invite: ["@target_user:domain.com"],
    is_direct: true,
    name: "Private Chat",
    initial_state: []
});
```

**后端自动行为：**
- **加入规则**：自动设置为 `invite`（仅限邀请）。
- **历史可见性**：自动设置为 `invited`（仅成员可见）。
- **访客访问**：自动设置为 `forbidden`。
- **隐私标记**：自动发送 `com.hula.privacy` 状态事件，用于通知客户端启用防截屏等保护。

### 2. 防截屏功能 (Anti-Screenshot)

当房间被标记为私密聊天时，后端会下发特定的状态事件。前端需监听此事件并启用防截屏保护（如 Android `FLAG_SECURE`）。

**前端实现：**

监听 `com.hula.privacy` 状态事件：

```javascript
// 伪代码示例
const privacyEvent = room.currentState.getStateEvents("com.hula.privacy", "");
if (privacyEvent && privacyEvent.getContent().action === "block_screenshot") {
    // 启用防截屏
    AndroidInterface.enableSecureFlag(); 
    // 或在 Web 端显示水印/遮罩
}
```

### 3. 阅后即焚 (Burn After Reading)

支持对单条消息启用阅后即焚。无需专用 API，通过消息元数据（Metadata）驱动。

**前端实现：**

1.  **发送消息**：在 `content` 中添加 `burn_after_read: true`。

```javascript
client.sendMessage(roomId, {
    msgtype: "m.text",
    body: "This message will self-destruct.",
    burn_after_read: true // 关键：标记为阅后即焚
});
```

2.  **触发销毁**：当用户阅读消息后，发送标准的已读回执 (`m.read`)。

```javascript
// 当消息出现在视口中时
client.sendReadReceipt(event);
```

**后端自动行为：**
- 后端收到 `m.read` 回执后，检测到目标消息带有 `burn_after_read` 标记。
- 启动 **30秒** 倒计时。
- 倒计时结束后，自动执行 `Redaction`（物理删除）操作，消息内容将被永久清除。

## 最近修复

### 2026-07-31

- **修复 rendezvous 认证问题**: MSC4108 QR 登录流程现在正确允许未认证访问 (`synapse-web/src/routes/msc4108_rendezvous.rs`, `synapse-web/src/routes/rendezvous.rs`)
- **修复 widget 权限硬编码**: 移除 `is_member = true` 死代码，简化权限检查逻辑 (`synapse-web/src/routes/widget.rs`)
- **修复 push_notification 响应包装**: 移除不必要的 JSON 包装，直接返回数组 (`synapse-web/src/routes/push_notification.rs`)

## 项目任务与状态追踪

> ⚠️ 任务追踪已整合到 GitHub Issues 和项目看板

- **任务看板**: [HuLa Project Board](https://github.com/hu-matrix/hula/projects)
- **代码审查报告**: [code_review_report_2026-08-11.md](artifacts/code_review_report_2026-08-11.md)
- **API 覆盖率**: [API_COVERAGE_REPORT.md](docs/synapse-rust/API_COVERAGE_REPORT.md)
- **文档索引**: [INDEX.md](docs/INDEX.md)
- **测试接线清单**: [.trae/specs/analyze-synapse-gap-and-optimization/test-execution-inventory.md](.trae/specs/analyze-synapse-gap-and-optimization/test-execution-inventory.md)
