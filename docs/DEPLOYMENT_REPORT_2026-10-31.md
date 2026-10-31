# 部署报告：全环境清理 + 零缓存重建 + 全量部署（2026-10-31）

- 部署时间：2026-10-31 17:12（Asia/Shanghai）
- 代码基线：`46222ba1b`（`fix(service): /sync 不再清空 presence status_msg`）
- 镜像：`synapse-rust:6.3.0-46222ba1b`
  - `org.opencontainers.image.revision=46222ba1b`
  - `org.opencontainers.image.version=6.3.0`
- 部署方式：`docker/deploy/deploy.sh --all`（内置 `--no-cache`、`--target tools`）
- 结论：25 个部署步骤全部 SUCCESS；全功能 API 集成测试 530 PASS / 70 SKIP / 0 FAIL。

---

## 1. 环境清理

| 项目 | 清理前 | 清理后 | 动作 |
|------|--------|--------|------|
| 本地 `target/` | 596 GB | 已删除 | `rm -rf target`（`.dockerignore` 已排除，不影响构建上下文） |
| Docker 构建缓存 | 3.685 GB | 35 MB | `docker builder prune -a`（回收 3.65 GB） |
| Docker 镜像 | 19 个 / 2.725 GB | 14 个 / 2.095 GB | `docker image prune -a`（回收 630.8 MB） |
| Docker 容器 / 网络 | — | 已清理 | `container prune`、`network prune` |
| Docker 卷 | — | 仅清未使用卷 | `volume prune`；`postgres_data`、`redis_data`、`synapse-test-db` 保留 |
| 运行时临时文件 | — | 已清空 | `docker/deploy/test-results/*`、历史部署日志 |

保留项：数据库卷、`backups/`（106 MB）、媒体文件（668 KB）。

> 说明：部署后 `docker system df` 显示 Build Cache 3.684 GB，为本次 `--no-cache` 构建新产生的
> layer/apt mount 缓存，非旧残留。

## 2. 零缓存重建

- 命令：`docker/deploy/deploy.sh --all`
- 构建参数：`--no-cache`、`--target tools`
- 构建耗时：717 s
- 回滚源：清理阶段删除了旧本地标签，脚本自动以运行中的 `synapse-app` 镜像生成
  `synapse-rust:rollback-20261031_171247`（预期行为，WARNING 非故障）
- 部署前备份：`docker/deploy/backups/synapse_backup_20261031_171249.tar.gz`（DB + 媒体 + 配置）
- 数据库迁移：统一基线 `00000000_unified_schema_v12`，架构校验通过
- 容器日志扫描：未发现 ERROR，且无未知 WARNING

## 3. 服务编排与容器状态

13 个服务容器全部运行；除 `synapse-nginx-proxy`（自身无 healthcheck）与保留的
`synapse-test-db` 外，其余 11 个均 `Up (healthy)`：

`synapse-app`、`synapse-postgres`、`synapse-redis`、`synapse-nginx`、`synapse-prometheus`、
`synapse-grafana`、`synapse-alertmanager`、`synapse-alert-handler`、`synapse-pushgateway`、
`synapse-node-exporter`、`synapse-postgres-exporter`。

端口监听 15/15：80、443、8448、8008、9090、5432、3000、9092、9093、8080、8081、8443、
3478、9100、9091。

> Prometheus 映射到 `127.0.0.1:9092`；`9090` 由 OrbStack 自身占用，非本项目服务。

## 4. 功能验证

### 4.1 接口连通性

| 端点 | 状态码 |
|------|--------|
| `http://127.0.0.1:8008/_matrix/client/versions` | 200 |
| `https://127.0.0.1/_matrix/client/versions`（nginx TLS） | 200 |
| `http://127.0.0.1:8008/health` | 200（`status: ok`，`server_holds_room_keys: false`） |
| `https://127.0.0.1:8448/_matrix/federation/v1/version` | 200 |
| `https://127.0.0.1/health`（nginx） | 200 |
| `http://127.0.0.1:3000/api/health`（Grafana） | 200 |
| `http://127.0.0.1:9092/-/ready`（Prometheus） | 200 |

### 4.2 全功能 API 集成测试

- 命令：`docker/deploy/api-integration_test.sh --profile full`
- 结果：**530 PASS / 70 SKIP / 0 FAIL**，结尾 `✓ No hard failures detected.`
- 70 个 SKIP 均为设计内原因（destructive test、requires federation destination data、
  network unreachable、SSO/OIDC not configured 等）

首轮 5 个失败（6 条记录）均为测试脚本与服务端契约不一致，已在同一提交修复：

| 用例 | 根因 | 修复 |
|------|------|------|
| List Registration Tokens / Get Active Registration Tokens | SEC-05：凭据返回型只读端点强制 MFA | 只读敏感前缀补 `registration_tokens` |
| Admin Shutdown Room | 服务端 `deny_unknown_fields` 仅接受 `room_id` | 请求体去掉 `user_id` |
| Get Event Perspective | 服务端要求 `event_id` 查询参数 | 补 `?event_id=...` |
| Federation Backfill ×2 | `v` 应为房间版本、`from` 应为游标 | 改为 `?v=12&from=<event_id>&limit=10` |

## 5. 可追溯产物

原始日志与测试清单位于被 `.gitignore` 忽略的目录，仅保留在部署机上：

- 部署日志：`docker/deploy/logs/deploy_20261031_171247.log`（144 KB，25 步全量）
- 测试通过清单：`docker/deploy/test-results/api-integration.passed.txt`（530 行）
- 测试跳过清单：`docker/deploy/test-results/api-integration.skipped.txt`（70 行）
- 测试失败清单：`docker/deploy/test-results/api-integration.failed.txt`（0 行）
- 部署前备份：`docker/deploy/backups/synapse_backup_20261031_171249.tar.gz`

## 6. 复现步骤

```bash
# 1) 清理
rm -rf target
docker builder prune -af
docker image prune -af
docker container prune -f
docker network prune -f
docker volume prune -f
rm -rf docker/deploy/test-results/*

# 2) 零缓存重建 + 全量部署（脚本默认 --no-cache --target tools）
docker/deploy/deploy.sh --all

# 3) 核对镜像 revision 与 HEAD
git rev-parse --short=9 HEAD
docker inspect synapse-app --format '{{index .Config.Labels "org.opencontainers.image.revision"}}'

# 4) 全功能验证
docker/deploy/api-integration_test.sh --profile full
```
