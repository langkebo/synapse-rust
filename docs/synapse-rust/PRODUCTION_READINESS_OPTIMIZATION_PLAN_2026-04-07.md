# synapse-rust 生产就绪优化方案（对标 Element Synapse）— 2026-04-07

本方案以 Element Synapse（Python/Twisted + Rust 组件）在大规模生产部署中的经验为参照，结合 synapse-rust 当前实现，给出可执行的“生产就绪”优化路径：以协议语义为基准，以门禁（测试/性能/安全/运维）为交付物，优先解决会在真实流量下放大成事故的薄弱点。

## 0. 生产就绪定义（Gate）

生产就绪不等于“功能齐全”，必须同时满足：

- 正确性：核心协议路径的契约/集成测试全部通过（Must 0 失败）
- 安全：内部控制面/敏感接口默认不可公网暴露；鉴权与最小权限可回归；敏感日志 0
- 可用性：关键后台任务（清理/保留/索引/队列）持续运行且可观测；故障可诊断
- 性能：同步/联邦/写事件三条主链路具备背压与可回归基准；允许受控 429
- 运维：配置有单一事实来源；支持 worker/多实例时一致性策略清晰、可验证

## 1. 对标结论（来自 Element Synapse 的“事故类薄弱点”）

面向生产部署，最容易放大成事故的薄弱点集中在：

- 扩展与一致性：worker/多进程需要共享 DB 与跨进程同步；内部 replication/控制面必须严格内网化与鉴权
- 联邦放大：join/EDU/presence/server-keys 等会在大房间与复杂拓扑下触发风暴，需要限流/缓存/隔离
- 同步分流：initial sync 与增量 sync 的资源特性差异大，需要隔离与背压策略
- 数据生命周期：位置/事件/媒体/审计等数据需有可配置的保留与持续清理，并且可观测

## 2. synapse-rust 当前差距（P0 风险）

### 2.1 控制面与内部接口安全（P0）

- `/_synapse/worker/v1/*` 已实现分层：管理类端点走 Admin JWT，worker 调用类端点走 replication shared-secret
- worker 控制面默认不注册：仅当 `worker.enabled=true` 时启用 `/_synapse/worker/v1/*` 路由
- `replication_http_auth_middleware` 已挂载到 worker 调用类端点路由
- `synapse_worker` 的 `/metrics` 默认仅监听 localhost，支持可选 token 鉴权；若绑定非 localhost，必须配置 token 才启动 metrics listener

### 2.2 联邦放大与资源竞争（P0/P1）

- 已为 server-keys 拉取增加全局并发上限、配置化 timeout、失败 backoff，避免 key 风暴拖垮主链路
- 仍需对 join/EDU（尤其 presence）处理做并发上限与隔离，避免在大房间/复杂联邦下出现资源饿死
- 需要明确“受控 429”策略：哪些端点允许 429、如何返回 retry_after_ms、如何在测试中锁定行为

### 2.3 同步与背压（P0/P1）

- Sliding Sync / Sync 已有实现与限流；已为 Sync 增加“initial vs incremental”分离限流门禁（配置 + 回归）
- 性能 smoke 已补充可解析输出（PERF_SMOKE_JSON + ok/429/other、p50/p95/p99）；CI 门禁化仍作为后续迭代

### 2.4 数据生命周期（P0/P1）

- Beacon/Location、媒体、审计、通知队列等必须具备可持续运行的清理/保留任务与可观测指标

## 3. 三阶段落地（P0 → P1 → P2）

### 3.1 P0：安全与门禁先行（必须进主干）

目标：默认配置下不暴露内部控制面；出现异常时“可回归、可定位”。

- 内部接口
  - 将 `/_synapse/worker/v1/*` 中“worker 调用类端点”接入 replication shared-secret 中间件（保留管理类端点走 Admin JWT）
  - shared-secret 使用独立 header，避免与用户 Authorization 冲突
  - 为控制面端点补充集成回归：开启/关闭中间件、缺失/错误 secret 的行为
- Worker metrics
  - `synapse_worker` metrics 监听默认绑定 localhost；可配置端口；支持可选 token 鉴权
- 文档与配置
  - 产出“内部接口暴露清单 + 反向代理建议 + 安全门禁”单页

验收门禁：

- 新增集成测试：worker 内部端点在 `worker.replication.http.enabled=true` 时缺少 secret 必须 401；带正确 secret 正常
- 关键控制面端点在默认配置下不可被公网误用（至少默认 bind 到 127.0.0.1 或需要 secret）

### 3.2 P1：联邦/同步的放大治理（生产 SLO 基线）

目标：避免 join/EDU/key 拉取风暴导致核心链路饥饿；同步链路可稳定降级（429）。

- 联邦
  - 为 server-keys 拉取、presence EDU 处理增加并发上限与 backoff（全局/按 origin）
  - 为 join 相关路径增加隔离舱：遇到风暴时优先保证 join 主链路
- 同步
  - 将 initial sync 与增量 sync 的资源隔离策略固化为配置与回归用例
  - 对 Sliding Sync 的 429/backoff 行为补齐契约测试（已有则加边界用例）
- 观测
  - 对上述限流点输出 metrics（ok/429/5xx、queue/backlog），并提供最小仪表盘说明

验收门禁：

- 联邦压力场景下 join 成功率不被 EDU/key 风暴拖垮（集成/模拟）
- 同步端点在过载时返回协议兼容的 429 + retry_after_ms（可回归）

### 3.3 P2：数据生命周期与长期可运维（GA）

目标：长期运行无累积风险；数据可治理；故障演练可闭环。

- 数据生命周期
  - 位置/媒体/审计/队列：保留策略 + 定时清理 + 可观测 + 手工运维入口
  - 24h soak：RSS 不得单调上升；数据库膨胀有阈值与报警
- 互操作
  - 建立客户端互操作矩阵（Element Web / Element X / FluffyChat / Hydrogen 等）
- 发布与支持
  - 生成“生产部署模板”（docker/compose、反向代理、worker/redis/postgres 参考）

## 4. 执行顺序（本次变更范围）

本次已完成 P0，并补齐了原先列入 backlog 的 P1/P2 生产门禁项；更长期的互操作矩阵、生产部署模板等仍保留在后续路线图中。

### 4.1 本轮已完成（原 Backlog 项）

- P1：join 隔离舱、EDU（presence）处理的并发上限/隔离 + 配套 metrics 与压测场景
- P1：将性能 smoke 门禁化到 CI
- P2：数据生命周期（位置/媒体/审计/队列）保留与持续清理任务 + 可观测 + 24h soak 门禁

## 5. 证据与映射（单一事实来源）

- 协议差异矩阵：docs/MSC_DIFFERENCE_MATRIX.md
- Sliding Sync / Beacons / Location 技术路线：docs/SLIDING_SYNC_BEACONS_LOCATION_EVALUATION_AND_ROADMAP.md
- 生产就绪审查与验收：本文件作为 P0/P1/P2 入口，相关验证证据归档到 docs/synapse-rust/VERIFICATION_* 系列文档
