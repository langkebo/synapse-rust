# synapse-rust

用 Rust 实现的 Matrix Homeserver（可运行，当前处于能力收敛与验证阶段）。

## 功能概览

- Matrix / Synapse 相关能力已广泛铺设，当前按能力域持续收敛与验证
- PostgreSQL 持久化（`sqlx`）
- Redis 缓存
- 可选 Elasticsearch 搜索（用于私聊消息搜索）
- Docker Compose 一键部署（synapse + postgres + redis + nginx）

## 快速开始（Docker）

推荐使用 `docker/docker-compose.yml`（已为容器环境配置 DB/Redis Host）。

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

- 容器部署默认读取：`docker/config/homeserver.yaml`
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
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
cargo run --release
```

统一口径：

- 部署与升级的唯一迁移执行入口是 `docker/db_migrate.sh migrate`
- Docker 容器只是由入口脚本自动调用该迁移入口，不构成第二套迁移方案
- 服务启动默认只执行 schema health check，不执行运行时迁移
- 仅在显式开启 `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 且未设置 `SYNAPSE_SKIP_DB_INIT` 时，才进入运行时兼容初始化路径

## 环境变量（覆盖配置）

配置读取逻辑：优先读配置文件（`SYNAPSE_CONFIG_PATH` 指定），并支持 `SYNAPSE_` 前缀的环境变量覆盖（使用 `__` 表示层级）。

- `SYNAPSE_CONFIG_PATH`：配置文件路径（默认 `homeserver.yaml`）
- `SYNAPSE_DATABASE__HOST` / `SYNAPSE_DATABASE__PORT` / `SYNAPSE_DATABASE__USERNAME` / `SYNAPSE_DATABASE__PASSWORD` / `SYNAPSE_DATABASE__NAME`
- `SYNAPSE_REDIS__HOST` / `SYNAPSE_REDIS__PORT` / `SYNAPSE_REDIS__ENABLED`
- `SYNAPSE_SEARCH__ELASTICSEARCH_URL` / `SYNAPSE_SEARCH__ENABLED`
- `RUST_LOG`：日志过滤（例：`info,synapse_rust=debug`）

## 文档

- 当前阶段正式分析与优化方案：`docs/synapse-rust/SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md`
- 第一版能力总表：`docs/synapse-rust/CAPABILITY_STATUS_BASELINE_2026-04-02.md`
- API 参考：`docs/synapse-rust/API_DOCUMENTATION.md`
- 工程收口计划：`docs/API-OPTION/engineering-optimization-plan.md`
- 部署指南：`docs/synapse-rust/DEPLOYMENT_GUIDE.md`
- 迁移索引：`migrations/MIGRATION_INDEX.md`
- 迁移治理：`docs/db/MIGRATION_GOVERNANCE.md`

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

## 项目任务与状态追踪

> ⚠️ 任务追踪已整合到 GitHub Issues 和项目看板

- **任务看板**: [HuLa Project Board](https://github.com/hu-matrix/hula/projects)
- **完成度报告**: [COMPLETION_REPORT.md](docs/synapse-rust/COMPLETION_REPORT.md)
- **当前正式分析**: [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](docs/synapse-rust/SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md)
- **能力状态基线**: [CAPABILITY_STATUS_BASELINE_2026-04-02.md](docs/synapse-rust/CAPABILITY_STATUS_BASELINE_2026-04-02.md)
- **测试接线清单**: [.trae/specs/analyze-synapse-gap-and-optimization/test-execution-inventory.md](.trae/specs/analyze-synapse-gap-and-optimization/test-execution-inventory.md)
