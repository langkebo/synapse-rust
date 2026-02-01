# synapse-rust

用 Rust 实现的 Matrix Homeserver（开发中）。

## 功能概览

- Client-Server API（部分实现）
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
cargo run --release
```

服务启动时会自动执行数据库迁移（migrations）。

## 环境变量（覆盖配置）

配置读取逻辑：优先读配置文件（`SYNAPSE_CONFIG_PATH` 指定），并支持 `SYNAPSE_` 前缀的环境变量覆盖（使用 `__` 表示层级）。

- `SYNAPSE_CONFIG_PATH`：配置文件路径（默认 `homeserver.yaml`）
- `SYNAPSE_DATABASE__HOST` / `SYNAPSE_DATABASE__PORT` / `SYNAPSE_DATABASE__USERNAME` / `SYNAPSE_DATABASE__PASSWORD` / `SYNAPSE_DATABASE__NAME`
- `SYNAPSE_REDIS__HOST` / `SYNAPSE_REDIS__PORT` / `SYNAPSE_REDIS__ENABLED`
- `SYNAPSE_SEARCH__ELASTICSEARCH_URL` / `SYNAPSE_SEARCH__ENABLED`
- `RUST_LOG`：日志过滤（例：`info,synapse_rust=debug`）

## 文档

- API 参考：`docs/synapse-rust/api-reference.md`
- 实现指南：`docs/synapse-rust/implementation-guide.md`

## 项目任务与状态追踪

为确保项目按计划推进，我们使用自动化工具跟踪未完成的任务和文档TODO。

- **最新任务报告**: [Unfinished Tasks Report](docs/synapse-rust/unfinished_tasks_summary_20260201_114736.md)
- **JSON 数据源**: [unfinished_tasks.json](docs/synapse-rust/unfinished_tasks_20260201_114736.json)

### 生成任务报告

在提交代码前，建议运行以下脚本以更新任务清单：

```bash
python3 scripts/analyze_docs.py
```

此脚本会扫描 `docs/` 和 `src/` 目录，提取 `TODO`、`FIXME` 及其他关键词，并生成最新的 JSON 和 Markdown 报告。
