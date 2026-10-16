# Worker 拓扑验证：现状审计与完善方案（2026-10-16）

> **问题来源**：`docs/synapse-rust-vs-synapse-comparison.md` 的 §2.3 / §5.1 / §12.4 三处写作
> 「Worker 拓扑验证仍在建设中（`topology_validator.rs`）」。
> **本轮结论**：该判定**方向对、粒度错**。`synapse-services/src/worker/topology_validator.rs`
> （956 行）**不是半成品**——它是一套**已接线到启动路径的静态配置校验器**；真正缺的不是"验证器"，
> 而是**运行时可观测性、强制力、以及管理面数据的真实性**。⇒ 正确措辞是
> 「**静态校验已成型；运行时对账、强制力与参考部署未做**」。
>
> **取证方式**：全部为本仓源码实测（标注 `文件:行`）；上游对照取
> `element-hq/synapse@release-v1.163` 的 `synapse/config/workers.py`（665 行）。
> 本文件是**审计与方案**，不含实现改动；实施按 §4 的批次推进。

---

## 1. 现状盘点：已经建成什么

### 1.1 静态配置校验（已成型，且已接线）

| 能力 | 实现位置 | 级别 | 接线点 |
|------|----------|------|--------|
| 主进程必须在启用集合内 | `topology_validator.rs:129` | error | `validate_worker_config` → `src/server/mod.rs:208` |
| `instance_map_key` 跨类型唯一 | `:134-147` | error | 同上 |
| 同前缀归属冲突 | `:149-183` | **warning** | 同上 |
| 实例名无法反推 worker 类型 | `:367-374` | warning | 同上 |
| `instance_map` 非空但 `replication.enabled=false` | `:376-380` | error | 同上 |
| replication secret 存在性 | `:385-387` | error | 同上 |
| replication secret 强度（占位符/短值） | `synapse-common/src/config/worker.rs:206-257` | error（**release 才严格**） | `src/server/mod.rs:197`（独立 fail-fast） |
| stream writer 引用未知实例 | `:396-408` | error | 同上 |
| stream writer 归属的 worker 类型不允许 | `:410-421` | error | 同上 |
| 从实例名反推 worker 类型 | `:188-197`、`:246-261` | — | — |
| MSC4140 家族精确路径集（6 条） | `:48-68` | — | 有**双向**守卫：`tests/unit/worker_delayed_events_ownership_tests.rs`（既查漂移也查家族覆盖） |
| 3 个路由归属探针（sync/media/federation） | `:266-315` | — | 生产 3 处路由用 `current_instance_worker_type` 打标：`synapse-web/src/routes/sync.rs:25`、`routes/media/preview.rs:14`、`routes/federation/mod.rs:285` |

### 1.2 运行时面（已接线）

| 能力 | 实现位置 | 接线证据 |
|------|----------|----------|
| worker 注册 / 心跳 / 注销 | `worker/manager.rs:157` / `:228` / `:291` | 28 条 `/_synapse/worker/v1/*` 路由，`synapse-web/src/routes/worker.rs:700-737` |
| 健康检查（心跳超时 → Unhealthy）+ 周期探测 | `worker/health.rs:271` | `synapse-services/src/wiring/admin.rs:254-268`（含 shutdown 联动） |
| 任务 / 命令队列、复制位点、事件流 | `manager.rs:321` / `:497` / `:471` / `:406` | 同上路由 |
| worker 事件汇（写 `worker_events`） | `worker/event_sink.rs:41` | `container.rs:469`（`config.worker.enabled` 时构造） |
| 独立 worker 进程（Redis 消费 + 心跳 + `/metrics`） | `src/bin/synapse_worker.rs` | 独立 bin |
| 「全局维护只在一个实例上跑」 | `topology_validator.rs:327-346` | `src/server/mod.rs:1127`、`wiring/admin.rs:238` |

### 1.3 上游对照：本仓在**建模**上其实超前

上游的"拓扑校验"是**配置解析期的 `ConfigError`**（`workers.py:222-505`，进程根本起不来）。它
**没有** worker 注册表、**没有**健康检查、**没有**拓扑只读端点、**没有**"worker 类型 × 职责域 ×
路由前缀 × 复制流 × 能力位"的表驱动模型。本仓的 `WorkerType`（10 类型，`synapse-storage/src/worker/models.rs:9-148`）
与上述运行时面都是上游没有的东西。

⇒ 把本仓写成「仍在建设中」是**低估**；但同时也确实存在"**静态有、运行时空**"的真实落差（§2）。

---

## 2. 缺口清单（W-1 … W-9）

### W-1（P0）拓扑非法**不阻断启动**

- **现象**：校验结果只进日志。`src/server/mod.rs:207-216`：`validation.log()` 后，`!valid` 只
  `warn!("…will continue to start…")`。
- **后果**：`stream_writers.events` 指向不存在的实例、`instance_map` 非空但 replication 关闭、
  secret 缺失（非 release）等**都会照样起服务**，错误被推迟到运行期以 404 / 无响应 / 事件不落库的形式暴露。
- **上游**：同类错误是 `ConfigError`（`workers.py:378-435`、`:395-430`）⇒ 进程直接退出。
- **判据**：`worker.topology.strict=true` 时非法配置必须 `Err`（拒绝启动）；默认 `false` 时行为与今天
  逐字节相同（负控）。

### W-2（P0）**没有任何运行时对账**（declared vs actual）

- **现象**：`validate_worker_config(&WorkerConfig)` 只吃静态配置（全仓唯一入参形态，见
  `synapse-web/src/routes/worker.rs:392`、`src/server/mod.rs:208`）。声明的 `instance_map` /
  `stream_writers` **从未**与真实注册并心跳的 worker（`WorkerManager::get_active`，`manager.rs:219`）比对。
- **后果**：三类事故不可见 ——
  1. 声明了 `event_persister` 但那个进程**从未起来**；
  2. 某个 worker **注册了**但类型不在配置里（配置漂移）；
  3. 声明的实例注册过、**心跳早已停**（stale）。
- **判据**：给定「配置 + 注册表快照」，产出 `missing` / `unexpected` / `stale` 三个集合；
  每类各 1 正控 + 1 负控。

### W-3（P0）`GET /_synapse/worker/v1/topology` 返回**硬编码文档**

- **现象**：`synapse-web/src/routes/worker.rs:658-660` → `WorkerTopologySummary::baseline()`
  （`synapse-storage/src/worker/models.rs:793`），函数签名里的 `State` 是 `_state`（未被使用）。
- **后果**：运维从这个端点看到的是"本仓支持哪些 worker 类型与预设"，**不是"我的集群现在长什么样"**。
  它与 `topology/validate` 一样完全不知道运行时注册表。
- **同批待补**：`baseline()` 里的 `deployment_presets`（`monolith` / `split_minimal` …，
  `models.rs:810-880`）只是**数据**：既不能用它校验用户配置"是否等于某个预设"，也不能由它**生成**
  配置 ⇒ 预设目前没有消费者。
- **判据**：端点返回体新增 `live` 段（注册中 worker + 健康 + 最后心跳年龄），**且既有
  `workers` / `deployment_presets` 字段原样保留**（向后兼容，测试不破）。

### W-4（P1）**覆盖性与基数规则缺失**

- **现象**：本仓的流写入约束是「非空（`:390-393`）+ 归属类型白名单（`:236-244`）」。
  `allowed_stream_writer_types` 对 `typing/to_device/account_data/receipts/presence/push_rules/device_lists`
  **只允许 master**，`events` 允许 `master`/`event_persister`。
- **与上游的差异（要说清是"更紧"还是"漏"）**：
  - **更紧**：上游允许 `device_lists`、`thread_subscriptions`、`quarantined_media_changes` 有专用
    writer（`workers.py:425-435`），本仓不允许 ⇒ 换来的是"无法表达专用 writer"（见 W-6）。
  - **漏**：① `events` 的**重复项**（`events: [event_persister, event_persister]`）不被检出；
    ② 上游对 5 条流有「**恰好 1 个**」的基数规则（`typing/to_device/account_data/presence/push_rules`，
    `workers.py:378-408`），本仓靠"只允许 master"间接实现——**一旦将来放开类型白名单，这条语义约束就静默消失**，
    没有独立判据兜底；
    ③ 任何流都**没有**"属主必须活着"的检查（与 W-2 同源）。
- **判据**：把上游那 10 条基数规则逐条落成纯函数判据（≥1 / ==1 / 重复项），并保留类型白名单作为**独立**的第二道。

### W-5（P1）`load_balancer` **生产未启用**（文档声称"内置"）

- **现象**：`enable_load_balancer`（`manager.rs:136`）与 `with_load_balancer`（`:124`）在全仓**只有测试调用点**
  （`synapse-services/src/worker/load_balancer.rs` 自身单测 + `tests/integration/worker_task_recovery_tests.rs`）；
  `wiring/admin.rs:255-257` 只接了 `.with_health_checker(...)`。
- **后果**：生产 `select_worker_for_task`（`manager.rs:689-703`）的 `if let Some(lb)` 分支**永不进入**，
  恒走 `select_worker_fallback`（按类型 + 健康度）。对比报告 §5.1 的「内置 load_balancer 模块」
  **只对测试为真**。
- **判据**：默认关闭时选主结果与今天一致（负控）；显式开启 + 一个候选不健康 ⇒ 不选中它（正控）。

### W-6（P2）配置面缺 2 条流

`StreamWriters`（`synapse-common/src/config/worker.rs:93-140`）有 8 条流；上游有 **10** 条 ——
多 `thread_subscriptions` 与 `quarantined_media_changes`（`workers.py:363-376`、`:425-435`）。本仓
MSC4306（线程订阅，已实现线程相关读取面）与媒体隔离（`media/mod.rs`）**都已有对应功能**，
但"由哪个实例写"在配置上**无法表达**。

### W-7（P2）duty 模型不统一

上游把"谁跑 pusher / 谁通知 appservice / 谁更新 user directory / 谁负责发送联邦"统一做成**可校验的 duty**
（`workers.py:443-505`，含"legacy 选项与新选项矛盾即 `ConfigError`"）。本仓只有"全局维护"
一个 duty（`should_run_global_maintenance`，`topology_validator.rs:344`）＋ `stream_writers` ＋ 路由前缀，
其余职责**没有统一表达** ⇒ 既无法校验归属，也无法在 `/topology` 里回答"pusher 跑在哪"。

### W-8（P2）**零参考多进程部署**

- `docker/config/homeserver.yaml:214-224`：`worker.enabled: false`、单实例 `master`；
- `docker/docker-compose.yml`：只有 `synapse-rust` 一个应用服务（无 worker 服务）；
- `docker/nginx/`：目录里只有 `README.md` 与空的 `ssl/`，**没有任何 nginx conf** ⇒ 反向代理侧的
  worker 分流（哪个前缀打到哪个实例）在仓内**没有可参考写法**。
- **后果**：多进程拓扑**从未端到端跑过**。§5.1 写的"TCP Worker 拓扑（可配置）"与 §12.4 的
  "水平扩展方案成熟度待验证"其实指向同一件事：**机制在，证据不在**。

### W-9（P2）无拓扑指标 / 告警

全仓仅有 `src/bin/synapse_worker.rs:466-477` 三个队列指标（`synapse_worker_queue_length` /
`_consumer_lag` / `_consumer_pending`）。**没有**：拓扑是否有效、未覆盖职责域数量、注册实例数、
实例最后心跳年龄。健康检查结果只存在内存与日志里（`worker/health.rs:241-263`）。

---

## 3. "Worker 模块是否可用"的判定

| 部署形态 | 判定 | 依据 |
|----------|------|------|
| **单进程（默认）** | ✅ **可用且零风险** | `worker.enabled=false` 时全部归属判定回落 `Master`（`topology_validator.rs:305-323`），拓扑校验退化为 `validate_topology(&[Master])`（`:359-361`）；A2/A3/A4/B1/B2 等既有功能不经过 worker 面 |
| **多进程** | ⚠️ **机制可用、证据不足** | 注册/心跳/任务/命令/复制位点/健康检查/事件汇均有实现与用例；但 W-1（不阻断）+ W-2（无对账）+ W-5（LB 未启用）+ W-8（无参考部署/烟测）叠加 ⇒ 现状是"**能跑，但出问题不好查**" |

**一句话**：Worker 模块**不是半成品**，是**"有躯干、缺神经"**——静态模型与运行时注册表都在，
缺的是把两者接起来（对账）、把结论变成强制（strict）、把结论变成可观测（端点 + 指标）。

---

## 4. 完善方案（批次 W-A … W-E，按性价比排序）

### W-A（建议先做）：让"验证"名副其实 —— **纯读，无行为变更**

1. **拟新增**纯函数模块 `synapse-services/src/worker/runtime_topology.rs`（只读现有 storage API，
   `WorkerManager::get_active` + `health.rs` 快照）：
   - `declared vs registered` 差集 → `missing` / `unexpected`
   - `uncovered_domains`（职责域无活属主）/ `over_owned_domains`（同域多属主；`events` 除外，它允许分片）
   - `stale_instances`（`last_heartbeat_ts` 超阈值；阈值与 `HealthCheckConfig` 同源，避免两套口径）
   - `stream_writer_violations`（上游 10 条基数规则，见 W-4）
2. `GET /_synapse/worker/v1/topology/validate` **新增** `runtime` 字段（保留既有字段）。
3. `GET /_synapse/worker/v1/topology` **新增** `live` 字段（既有 `workers` / `deployment_presets` 不动）。

- **判据**：四类判定各 1 正控 + 1 负控（差集 / 未覆盖 / stale / 基数）；基数规则**逐条对齐
  `workers.py:378-435`**（避免"我编的规则"）；端点各 1 条集成用例。
- **变异自证**：把差集判据改成恒空集合 ⇒ 对应用例必须变红。
- **为什么先做**：不新增 SQL（无 `.sqlx`、无基线/指纹变更）、不改启动路径（无 `src/server/mod.rs` 风险）、
  不改既有响应字段（无契约破坏）⇒ 可在共享工作区安全落地。

### W-B：强制力（严格模式 + 离线入口）——**独立批次**

1. `WorkerConfig` 增 `topology: { strict: bool（默认 false） }`；`strict` 时 `validate_worker_config`
   有 error ⇒ 返回 `Err` 拒绝启动（与 `worker_replication_secret_check` 在 `src/server/mod.rs:197` 同形）。
2. 默认 `false` ⇒ 存量部署行为零变化（与仓库"新开关默认不改变行为"的既有惯例一致）。
3. **离线入口**：给 `synapse_worker`（或根 bin）加 `--validate-topology`，退出码 0/1 ⇒ 配置**部署前**就能红，
   这是"拓扑验证"最实用的形态，也能直接进 preflight。
4. **判据**：`strict=false` 非法配置仍启动（负控）；`strict=true` 返回 `Err`（正控）；退出码用例。

### W-C：生产启用负载均衡（默认关）

`wiring/admin.rs` 增 `if config.worker.load_balancing.enabled { manager.enable_load_balancer(strategy) }`；
`WorkerConfig` 增 `load_balancing: { enabled（默认 false）, strategy }`。
- **默认关闭的理由**：今天 `select_worker_fallback` 已按健康度过滤（`manager.rs:723-743`），
  开启 LB 会**改变选主行为** ⇒ 必须显式开启。
- **判据**：默认关闭时选主结果与今天一致（负控）；开启 + 一个候选不健康 ⇒ 不选中它（正控）。

### W-D：参考多进程部署 + 端到端烟测（**水平扩展成熟度的唯一证据来源**）

1. 新增 `docker/docker-compose.workers.yml`（或 compose `profiles: [workers]`）：
   `master` + `event_persister` + `sync_worker` + `federation_reader` + `media_repository` + `background_worker`。
2. `docker/nginx/` 提供 upstream 示例：`/_matrix/client/*/sync` → `sync_worker`；
   `/_matrix/federation/*` → `federation_reader`；其余 client → `client_reader`。
3. 烟测脚本：起多进程 → `GET /_synapse/worker/v1/topology/validate` 断言 `valid:true` 且
   `runtime.missing` 为空 → 发 1 条消息 + 1 次 `/sync` → 断言事件落库、`/sync` 由 sync worker 返回。
- **判据**：脚本可重复执行；任一步失败退出非 0。

### W-E：拓扑可观测性

指标建议：`synapse_worker_topology_valid`、`synapse_worker_registered_total`、
`synapse_worker_uncovered_domains`、`synapse_worker_stale_instances`、
`synapse_worker_instance_last_heartbeat_age_seconds{instance}`；告警：`topology_valid == 0` 持续 5 分钟、
`uncovered_domains > 0`。
- ⚠️ **仓库既定坑**：面板/告警的指标名有**静态正则门禁**（`scripts/ci/check_dashboard_metrics.py`），
  且面板只能放进"真的抓了该 exporter"的栈（dev 栈没有 `postgres` job）。⇒ **先定指标名 → 注册 →
  同批改门禁白名单与面板**，不要分两次。

### 排期与其他批次的冲突面

W-A 只动 `synapse-services/src/worker/` 与 `synapse-web/src/routes/worker.rs`，与 §19.5 的 B3（AS 临时数据）、
C4（MSC4354 请求面）改动面**不重叠**；W-B 动 `src/server/mod.rs`；W-C/W-D 动 `wiring/admin.rs` 与 `docker/`。
⇒ 建议：**W-A 一批（可立即做）→ W-B 一批 → W-C/W-D/W-E 按需**。

---

## 5. 待核实（本轮未取证，勿当作结论）

| 编号 | 待核实项 | 核实方式 |
|------|----------|----------|
| V-W-1 | 多进程下 `worker_events` 的写入者与消费者是否唯一（是否会出现两个实例消费同一流位置） | 读 `event_sink.rs` + `worker_events` 表 DDL + 多进程烟测 |
| V-W-2 | 多实例命名（`client_reader-1` / `client_reader-2`）是否都被 `worker_type_for_instance_name` 正确归类 | 单测：`topology_validator.rs:188-197` 的前后缀匹配已处理 `-N`/`_N` 形态，需对角化用例确认 |
| V-W-3 | `WorkerTopologySummary` 是否被前端/外部工具消费（决定 W-A 能否直接扩字段） | 全仓检索消费方 + `docs/openapi/` |
| V-W-4 | 上游 `RoutableShardedWorkerHandlingConfig` 的"按房间路由到不同 events writer"本仓是否等价（本仓允许 `events` 多属主，但分片规则未取证） | 读 `workers.py:440-445` 与本仓 events 写入路径 |
| V-W-5 | `worker.enabled: true` 时，master 与 worker 对同一个「职责」是否会**双重执行**（上游注释明确写"No effort is made to ensure only a single instance of these tasks is running"，本仓是否更强） | 读 `should_run_global_maintenance` 的全部消费点 |

---

## 6. 文档同步（对 `docs/synapse-rust-vs-synapse-comparison.md`）

| 位置 | 现文案（问题） | 建议文案 |
|------|----------------|----------|
| §2.3 劣势行 | 「Worker 拓扑验证仍在建设中（`topology_validator.rs`）」——**低估**：验证器已成型且已接线 | 「拓扑**静态校验已成型**（`worker/topology_validator.rs`，启动时执行）但**只记日志不阻断**；**运行时对账**（声明 vs 注册）、严格模式与管理面实况未做」 |
| §5.1「Worker 发现」 | 「健康检查 + 拓扑验证」——没说清后者是静态的 | 「健康检查（运行时）+ 拓扑**静态**校验；运行时对账见 `docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md`」 |
| §5.1「负载均衡」 | 「内置 load_balancer 模块」——**生产未启用**（只有测试接线） | 「内置 load_balancer 模块（**默认未启用**：`enable_load_balancer` 目前只有测试调用点）」 |
| §12.4 风险提示 | 「Worker 拓扑验证仍在建设中，水平扩展方案成熟度待验证」 | 「Worker 拓扑：静态校验已成型，**运行时对账/强制力/参考多进程部署**未做 ⇒ 水平扩展成熟度待验证（见审计文档）」 |

⚠️ 改该文档必须过 `tests/unit/doc_credibility_guard_tests.rs`（反引号内仓库相对路径必须存在；
「N 条注册路由 / N 个模块」计数必须与 `docs/synapse-rust/ROUTE_CONTRACT.md` 一致；五个历史章节标题
后 6 行内保留 `§18.5` 指针）。§12.4 属五处之一 ⇒ **只改标题下方的条目，不动标题**。
