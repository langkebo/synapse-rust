# Synapse-Rust 部署回滚与交付报告

## 1. 技术债务审计与修复报告

| 债务类型 | 发现问题 | 修复方案 | 状态 |
| :--- | :--- | :--- | :--- |
| **容器安全与体积** | 镜像体积过大，包含多余的 shell、curl 和包管理器工具，增加了容器的受攻击面。 | 使用了多阶段构建，将运行时基础镜像替换为极简且无 shell 的 `gcr.io/distroless/cc-debian12:nonroot`，剥离了所有操作系统工具。 | ✅ 已修复 |
| **配置缺失与冗余** | `homeserver.yaml` 中配置分散、缺乏注释、未启用 worker 及相关的集群/重试机制。 | 重新编写了全局 [homeserver.yaml](docker/config/homeserver.yaml)，包含所有后端可选模板、完整中文注释并调优了 Worker 和队列参数。 | ✅ 已修复 |
| **可观测性不足** | 原日志为普通文本，难以集中收集与搜索；缺乏链路追踪机制。 | 在 `homeserver.yaml` 中启用了结构化日志（JSON 格式），并引入了完整的 OpenTelemetry 分布式追踪和 Prometheus 监控配置。 | ✅ 已修复 |
| **前端类型安全** | Hula 前端的 `MatrixSessionService.ts` 及其他文件存在大量的 `undefined` 方法调用和隐式 `any` 类型错误。 | 通过可选链 (`?.`)、空值合并及明确的 `unknown` 转型，修复了前端所有的构建时静态类型错误。 | ✅ 已修复 |
| **CI/CD 缺陷** | 部署脚本缺乏安全扫描和自动化镜像推送环节，存在高危镜像直接上线的风险。 | 更新了 `deploy.sh`，集成了 `trivy` 扫描拦截以及自动登录 Docker Hub 并推送到 `vmuser232922/synapse-rust` 的能力。 | ✅ 已修复 |

---

## 2. 性能测试与监控预期报告

随着本次架构优化和配置调优，预计项目达到以下性能与监控指标：

* **构建性能提升**：
  * 利用 `cargo-chef` 的缓存机制，在没有修改 `Cargo.toml` / `Cargo.lock` 依赖的情况下，构建时间从原先的数分钟缩减至几十秒。
  * 运行时镜像体积降至 **< 70MB**，减少了网络传输、节点调度以及存储成本。
* **高可用吞吐**：
  * 已在 `homeserver.yaml` 启用多 Worker 模式 (`pool_size: 4`) 和数据库连接池 (`max_size: 20`)，极大提升了并发消息处理的能力和抗压性。
* **监控仪表盘接入准备**：
  * 服务现已将 metrics 暴露于 `9090` 端口 (`/metrics`)。
  * OTLP 追踪导出至 `http://otel-collector:4317`。
  * **下一步**：您可以在 Grafana 中导入标准 Matrix/Synapse 监控大盘，结合 Elasticsearch 与 Jaeger 获得全站运行视角的透明度。

---

## 3. 部署脚本使用与回滚手册

### 一、 一键部署 (自动化 CI)

本项目现在可通过内置的 `deploy.sh` 进行端到端的构建、扫描和发布：

```bash
cd docker
./deploy.sh
```

**执行流程**：
1. 编译并打包精简的多阶段 Docker 镜像。
2. 使用 Trivy 对镜像 `vmuser232922/synapse-rust:latest` 进行漏洞扫描（遇到 `HIGH` 或 `CRITICAL` 漏洞会阻断部署）。
3. 自动登录并推送到私有/公有仓库。
4. 拉起 PostgreSQL 和 Redis 依赖并等待其就绪。
5. 启动 Synapse-main 主服务及 Worker 节点并检查运行状况。

### 二、 故障回滚操作 (Rollback)

如果线上新版本部署后出现不可逆的错误或崩溃，可以通过以下标准流程快速回滚至上一稳定版本。

**步骤 1：停止当前故障服务**
```bash
cd docker
docker compose -f docker-compose.prod.yml stop synapse-main synapse-worker
```

**步骤 2：切换至历史稳定的 Docker 镜像版本**
找到上一个稳定版的标签（假设为 `v1.0.0`），在服务器上修改 `docker-compose.prod.yml` 或者直接通过 CLI 启动旧版镜像：
```bash
# 临时运行旧版镜像覆盖当前环境
docker run -d --name synapse-main \
  --env-file .env \
  -p 8008:8008 \
  -v $(pwd)/config:/app/config \
  vmuser232922/synapse-rust:v1.0.0
```

**步骤 3：数据库状态回滚（若涉及结构变更）**
* 本次项目中，非必须尽量不要在数据库表结构上做破坏性修改（Drop Table / Drop Column）。
* 如果数据库发生了前向不兼容的 Migration，需配合数据库的全量备份（如 `pg_dump` 产物）进行恢复：
```bash
docker exec -i db psql -U synapse -d synapse < backup_previous.sql
```

**步骤 4：验证回滚**
确保服务正常重启后，执行：
```bash
docker logs --tail=100 synapse-main
curl -f http://localhost:8008/health || echo "Service Unhealthy"
```
确认无 `error` 或 `fatal` 日志，即可宣布回滚成功。
