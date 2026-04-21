# Sliding Sync / Beacons / Location：技术评估与三阶段落地方案（PoC → Beta → GA）

本文以 Element Synapse（Python/Twisted + Rust 组件）项目的实现与演进方式为参照（尤其是其对 Sliding Sync 的系统性测试与持续优化节奏），结合本仓库 synapse-rust 的现状，给出三项能力的技术评估、差异清单与可执行的三阶段落地方案。

## 0. 目标与范围

覆盖三项能力：

1) **现有实现完整度**：API 端点、数据库 Schema、事件流处理逻辑  
2) **与最新 Matrix 规范兼容性差异**：Sliding Sync（MSC3575）与 Beacons/Location（MSC3488/MSC3489/MSC3672）  
3) **测试覆盖**：单元测试、集成测试、契约测试  
4) **性能基准**：≥10k 并发 Sliding Sync 延迟；Beacon 位置更新频率（1Hz）；Location 共享资源消耗  
5) **安全审计**：权限模型、隐私合规、加密传输、滥用/DoS 防护

交付目标（GA）：

- Sliding Sync：**100% MSC3575 合规**，端到端延迟 **≤200ms（p95@10k 并发稳定态）**  
- Beacons：**实时 1Hz 更新**（受控房间规模/配额下），并具备可回归的端侧电量评估方法，目标 **≤2%/h**  
- Location：支持端到端加密（E2EE）场景下的“可用且安全”（服务端不解密内容），7×24h 可用性 **≥99.9%**

## 1. 现状评估（synapse-rust）

### 1.1 Sliding Sync

**路由/端点**

- 路由：[`src/web/routes/sliding_sync.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/sliding_sync.rs)  
- 顶层装配：[`src/web/routes/assembly.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/assembly.rs)

当前滑动同步在本仓库已具备服务端实现与路由集成（包含 `/_matrix/client/v3/sync` 与 unstable namespace 入口），并通过服务层/存储层落库实现 pos、lists 等能力：

- 服务：[`src/services/sliding_sync_service.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/src/services/sliding_sync_service.rs)  
- 存储：[`src/storage/sliding_sync.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/sliding_sync.rs)

**数据库**

- Unified schema 中已包含 sliding sync 相关表/序列：[`migrations/00000000_unified_schema_v6.sql`](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/00000000_unified_schema_v6.sql)

**测试**

- 单测（结构/占位）：[`tests/unit/sliding_sync_api_tests.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/tests/unit/sliding_sync_api_tests.rs)  
- 集成/契约（路由 + 服务 + DB）：[`tests/integration/api_sliding_sync_contract_tests.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_sliding_sync_contract_tests.rs)

**主要缺口**

- 已补齐 PoC 级契约测试：鉴权、pos 递进/失效、lists/ranges/rooms 返回语义、room_subscriptions 行为、与传统 `GET /sync` 共存性。  
- 已补齐 typing/receipts/account_data、限流/backoff 与多 worker（跨实例 pos）一致性专项回归；threads 等扩展能力仍需后续补齐。

### 1.2 Beacons / Location（地理位置）

**服务/存储**

- 服务：[`src/services/beacon_service.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/src/services/beacon_service.rs)  
- 存储：[`src/storage/beacon.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/beacon.rs)

**数据库**

- consolidated 对齐迁移已包含 `beacon_info` / `beacon_locations`：[`migrations/20260404000001_consolidated_schema_alignment.sql`](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/20260404000001_consolidated_schema_alignment.sql)

**路由/事件接入**

- Beacon/Location 通过“房间事件写入链路”接入（无需额外专用端点）：  
  - `m.beacon_info*`（state）写入：`PUT /_matrix/client/v3/rooms/{room_id}/state/m.beacon_info/{state_key}`（含 state_key=sender 约束）  
  - `m.beacon*`（message-like）写入：`PUT /_matrix/client/v3/rooms/{room_id}/send/m.beacon/{txn_id}`  
  - 写入路径会将事件落到 `events`，并把解析后的字段落到 `beacon_info` / `beacon_locations`
- `BeaconService` 已进入依赖注入图（`ServiceContainer` 持有），并在 room 写事件逻辑中被调用。

**测试**

- 已具备端到端集成测试（写事件→入库断言）：[`tests/integration/api_beacon_location_tests.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_beacon_location_tests.rs)

**主要缺口**

- 已加入基础滥用防护：对同一 `beacon_info` 的位置上报进行 1Hz 速率限制（返回 429 + `retry_after_ms`）。  
- 已补按用户+房间/按房间限额与热点房间 token-bucket 背压（含 429 + retry_after_ms 回归）；容量模型与压测门禁仍需持续完善。  
- 缺少与 Sliding Sync 的联动验证（Sliding Sync 下是否能正确携带/过滤/增量下发相关事件）  
- 需要把“数据生命周期”门禁化：过期清理/保留策略应持续运行并可观测（避免位置数据无限增长）

## 2. 参照基线（Element Synapse）

Element Synapse 在 Sliding Sync 上的一个重要实践是：**用大规模、细粒度的服务端集成测试把协议语义“锁住”**，并持续围绕增量同步做性能优化与回归测试（其仓库中存在完整的 sliding_sync 测试目录与用例集，且针对增量同步性能有专项优化迭代）。

这意味着：要把 Sliding Sync / Beacons / Location 推到生产就绪，必须把“协议差异矩阵 + 集成测试 + 性能门禁 + 安全门禁”作为同等优先级的交付物，而不是只实现功能路径。

## 3. 规范兼容性差异面（必须形成差异矩阵并纳入 CI）

差异矩阵文档（单一事实来源）：[`docs/MSC_DIFFERENCE_MATRIX.md`](file:///Users/ljf/Desktop/hu/synapse-rust/docs/MSC_DIFFERENCE_MATRIX.md)

### 3.1 Sliding Sync（MSC3575）

关键差异面（GA 前逐条验收）：

- 端点稳定化策略（unstable → stable 的迁移/兼容窗口）  
- lists/ranges 的增量语义与 pos 递进一致性（断线重连/catch-up 行为）  
- extensions 的兼容（typing/receipts/account_data/threads 等）与多 worker 一致性  
- 错误码与 backoff（限流/负载保护时的协议行为）

### 3.2 Beacons / Location（MSC3488/MSC3489/MSC3672）

关键差异面（若仅作为普通事件承载则默认不合规）：

- `m.beacon_info`（State）与 `m.beacon`（MessageLike）事件的内容结构/字段别名（stable/unstable）  
- Beacon 生命周期（start/stop/timeout、过期清理）与成员可见性（membership/history_visibility）  
- 高频更新 1Hz 的服务端约束：限流、配额、背压、写放大治理（避免把位置更新当普通 timeline 全量扩散）  
- Location 在 E2EE 房间中的服务端行为：不得破坏密文、不得因内容不可见而错误做策略判断

## 4. 测试与基准：门禁化设计

### 4.1 必须新增的测试类型

- **集成/契约测试（主干门禁）**  
  - Sliding Sync：鉴权、pos、lists/ranges、rooms 订阅、增量/初始/重连  
  - Beacon/Location：事件写入→解析落库→回读；权限/越权；过期清理；速率限制
- **互操作矩阵（CI 周期任务）**  
  - 客户端组合（示例）：Element Web、Element X（iOS/Android）、FluffyChat、Hydrogen  
  - 场景：Sliding Sync 初始/增量、Beacon 1Hz、Location（含 E2EE 房间）
- **性能压测（nightly + release gate）**  
  - Sliding Sync：10k 并发 p95 ≤ 200ms（GA），p99 ≤ 350ms  
  - Beacon：1Hz 更新在目标房间规模下可持续运行（DB/CPU/带宽上界可量化）  
  - 24h soak：内存泄漏 0 容忍（RSS 不得单调上升，必须给出定量阈值）

### 4.2 指标定义（建议统一成 CI 可解析报告）

- 延迟：p50/p95/p99 + 错误率（5xx/4xx 分开统计）  
- 资源：CPU、RSS、FD、连接数、DB QPS、慢查询数、缓存命中率  
- 正确性：协议差异矩阵“Must”项 0 失败  
- 安全：未授权访问 0；敏感数据日志 0；限流/封禁策略可回归

## 5. 三阶段落地方案（PoC → Beta → GA）

### 5.1 阶段 1：PoC（链路闭环 + 差异矩阵）

**a) 代码开发任务**

- Sliding Sync  
  - 补齐集成测试最小闭环：路由鉴权、pos 递进一致性、lists/ranges 基本语义  
  - 输出“MSC3575 差异矩阵（Must/Should/May）”并固化成回归用例  
- Beacons / Location  
  - 将 `BeaconService` 纳入依赖注入图（ServiceContainer）  
  - 在事件写入链路中实现：  
    - `m.beacon_info*`（state）事件解析 → `beacon_info` 落库  
    - `m.beacon`（message）事件解析 → `beacon_locations` 落库  
  - 最小权限/安全：仅允许成员写入；`m.beacon_info` 必须由其 state_key 对应用户写入

**b) 集成测试任务**

- 新增：`api_beacon_*` 端到端测试：写入 `m.beacon_info` 与 `m.beacon` → DB 断言  
- Sliding Sync：新增 1~2 个契约测试覆盖初始/增量与 pos  
- PoC 压测脚本入口（先 1k 并发）：输出延迟与资源报告（可回归）  
  - 手动入口（load smoke）：[`tests/performance/manual_smoke_tests.rs`](file:///Users/ljf/Desktop/hu/synapse-rust/tests/performance/manual_smoke_tests.rs)（`#[ignore]`，含 Sliding Sync 与 Beacon 热点房间背压 smoke；通过 `cargo test --features performance-tests --test performance_manual` 编译/执行）

**c) 交付物与阈值**

- 差异矩阵文档（MSC3575/3488/3489/3672）  
- PoC 集成测试通过（CI 主干）  
- 性能 PoC：1k 并发稳定态 Sliding Sync p95 ≤ 500ms、错误率 < 0.1%

### 5.2 阶段 2：Beta（10k 并发 + 语义完整 + 可运维）

**a) 代码开发任务**

- Sliding Sync  
  - 协议语义补齐与性能优化：缓存/背压、多 worker 一致性、观测性指标完善  
- Beacons  
  - 生命周期与过期处理：定时清理、超时逻辑与可配置保留策略  
  - 高频更新治理：按用户/设备/房间的速率限制与配额；写放大与扇出控制策略  
- Location  
  - 明确 E2EE 约束：服务端不解密；所有策略基于元数据/速率而非内容字段

**b) 集成测试任务**

- 10k 并发压测（nightly）+ 24h soak（周期任务）  
- 互操作矩阵扩充到 ≥5 组合（并固化为可跑脚本/环境）

**c) 交付物与阈值**

- Sliding Sync：MSC3575 合规 ≥ 95%，p95 ≤ 250ms（10k 并发稳定态）  
- Beacon：1Hz 在目标房间规模下稳定运行；限流/配额/背压可回归  
- 观测性：Grafana/Prometheus（或等价）面板与报警规则

### 5.3 阶段 3：GA（100% 合规 + SLO + 发布）

**a) 代码开发任务**

- Sliding Sync：稳定端点/兼容策略、最终性能收敛（≤200ms@p95）  
- Beacons/Location：合规补齐（权限/隐私/数据生命周期/删除导出）、HA 运维方案与容量模型  
- 安全：威胁建模、渗透测试闭环、日志脱敏与审计机制完成

**b) 集成测试任务**

- release gate：10k 并发性能门禁 + 7×24h soak + 故障演练  
- 0 内存泄漏：RSS 不得在 24h 内单调上升（需给出阈值与判定脚本）

**c) 交付物与阈值**

- Sliding Sync：MSC3575 100% 合规；p95 ≤ 200ms、p99 ≤ 350ms（10k 并发稳定态）  
- Beacon：1Hz 达标；端侧电量评估方法与报告（≤2%/h）  
- Location：E2EE 场景可用；可用性 ≥99.9%（以 7×24h 压测/演练作为 GA 前证据）

## 6. 风险与缓解

- **协议演进风险（MSC 变化）**：以差异矩阵为单一事实来源；每两周滚动评审更新并同步用例  
- **高频位置更新导致 DoS**：强制速率限制/配额/背压；热点房间策略（分级配额）  
- **隐私合规风险**：默认最小化存储、明确保留/删除策略、日志零敏感内容；审计记录必须脱敏  
- **E2EE 约束**：服务端不得依赖内容字段做安全判断；所有策略走元数据/频率/房间权限

## 7. 周粒度时间表（模板：12 周）

- W1–W2：PoC（事件桥接 + DI 注入 + 最小集成测试 + 差异矩阵草案）  
- W3–W4：PoC 收敛（Sliding Sync 契约测试扩充 + 1k 并发基线）  
- W5–W8：Beta（10k 压测、缓存/背压、Beacon 生命周期与 1Hz 治理、观测性）  
- W9–W10：Beta 收敛（互操作矩阵、故障演练、合规策略）  
- W11–W12：GA（100% 合规验收、≤200ms、7×24h、发布文档）

## 8. 责任人矩阵（RACI 模板）

- PM/协议负责人：R（差异矩阵与验收定义），A（阶段准入）  
- 后端负责人：A（架构与最终质量），R（Sliding Sync/Beacon/Location 实现）  
- 性能/DB 工程师：R（索引/迁移/写放大治理、压测基线、性能剖面）  
- QA/测试负责人：A（CI 门禁与互操作矩阵），R（用例实现与回归）  
- 安全负责人：A（威胁建模与审计结论），R（隐私/风控/渗透测试闭环）  
- SRE：R（监控告警、容量、HA/演练、SLO 报告），C（架构评审）

## 9. 通过/失败量化阈值（CI Gate）

- Sliding Sync  
  - Must 合规项 0 失败  
  - Beta：p95 ≤ 250ms@10k 并发稳定态；GA：p95 ≤ 200ms、p99 ≤ 350ms  
  - 错误率 < 0.1%，且 5xx 为 0（允许受控的 429）
- Beacon  
  - 1Hz 更新在目标房间规模下稳定；限流/配额/背压用例必须全部通过  
  - 任意越权/非成员写入 0 容忍  
- Location  
  - E2EE 不破坏密文/路由；7×24h 可用性 ≥99.9%  
- 通用  
  - 敏感数据（位置坐标/URI）写日志 0 容忍  
  - 24h soak RSS 不得单调上升（阈值由脚本判定）
