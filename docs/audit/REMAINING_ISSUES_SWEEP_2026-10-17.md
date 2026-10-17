# 遗留问题全面排查（2026-10-17，基线 `00a94e95a`）

> 目的：给"项目还剩什么问题"一个**当前可复核**的答案。本文只列**本轮实测**的结论，
> 每条带文件:行与可复现命令；已被证伪或已修的历史结论统一收进 §5，避免重复立项。
>
> 基线：`00a94e95a`（工作区干净，0 个未提交改动）。
> 方法：读码 + 跑门禁 + 失败项逐条定位到提交/行号；不采信文档里的"仍存在/已修复"字样。

## 1. 门禁结果总表

| 门禁 | 结果 | 备注 |
| --- | --- | --- |
| `cargo clippy --workspace --all-targets --features test-utils --all-features -- -D warnings` | ✅ 0 | 但该次是缓存命中（1.4 s）；以 unit 车道的完整编译为准 |
| `cargo nextest run --test unit --features test-utils` | ❌ **8 failed / 1883** | 见 R-1…R-5（3 个根因） |
| `scripts/ci/check_file_size_ratchet.py` | ✅ 210 over 600 = baseline | 阈值已 500→600，与 `.clippy.toml` 一致 |
| `scripts/ci/check_fmt_ratchet.sh` | ✅ 0 / baseline 0 | |
| `scripts/ci/check_clippy_allow_ratchet.py` | ✅ 163 = baseline | |
| `scripts/ci/check_trait_ratchet.py` | ❌ **71 > 68** | R-5 |
| `scripts/ci/check_sqlx_dynamic_ratio.sh` | ❌ 生产 96→**104**，测试 769→**770** | R-2 |
| `scripts/ci/check_plan_numbers_fresh.sh` | ✅ 5 键一致（`.sqlx` 1335 = 文档 1335） | |
| `scripts/ci/check_sqlx_cache_fresh.sh` | ❌ 1 处"缓存陈旧" | **误报**，R-6 |
| `scripts/contract/check_route_contract.sh` | ✅ 92 guards + 路由面 up-to-date + SDK⊆ledger | |
| `cargo deny check advisories` | ✅ `advisories ok` | ⚠️ 依赖本地 advisory 库新鲜度 |

> 结论：**契约链、格式、体积、文档数字、依赖安全是绿的**；红的是 SQLx 两条棘轮、
> trait 棘轮、以及 unit 车道里 8 条守卫用例。

## 2. 红色项（会挡住 CI / 提交）

### R-1 基线指纹漂移：迁移改了，常量没同步（P0，1 行修复）

- 用例：`tests/unit/test_isolation_unification_tests.rs:1007` `baseline_fingerprint_is_the_single_v12_source`
- 实测：`left(实际)=65b6701ed356b3d1`，`right(常量)=7a9773b755bea292`
- 根因：`0063f09ff feat(schema): 新增 event_receipts_stream 日志表与 user_pending_deactivations 队列`
  同时改了 `migrations/00000000_unified_schema_v12.sql` 与该常量，但常量值与改后文件的哈希不一致
  ⇒ 模板名前缀与实际内容脱钩，测试会**静默铸出第二个模板**（这正是该用例存在的理由）。
- 修法：按当前文件重算指纹并写入 `tests/unit/test_isolation_unification_tests.rs:244`
  （测试自己会把实际值打在断言里），同时确认 `synapse-storage` / `synapse-federation` /
  `synapse-services` 里 `include_str!` 的副本仍逐字节一致。

### R-2 生产区字面量动态 SQL +8（P1，两条棘轮同时红）

`tests/unit/sqlx_dynamic_literal_guard_tests.rs:733` 与 `check_sqlx_dynamic_ratio.sh` 指向同一批：

| 文件 | 新增处 | 行号 |
| --- | --- | --- |
| `synapse-storage/src/burn_after_read.rs` | 3 | 306 / 329 / 345 |
| `synapse-storage/src/user_deactivation.rs` | 3 | 36 / 56 / 70 |
| `synapse-storage/src/membership/mod.rs` | 2 | 382 / 448 |

- 来源提交：`aa074981f`（burn-after-read 清理，4 处）、`d9bc4cb88`（注销异步 parting，2 处）、
  `86e2cc2a2`（MSC4262 LEFT_ROOM，3 处，其中 2 处落在 membership）。
- 为什么必须处理：`dynamic_production` 是**单调下降**棘轮（真实债务），不是"调基线了事"的那类。
- 修法优先级：① 能静态化的改 `query!/query_as!/query_scalar!`（这三个模块都是固定 SQL，
  只有 `Option` 过滤可能是动态拼装）；② 确属"运行期决定 SQL 文本"的，改传 `&sql`（`query(&sql)`），
  这样走 `param` 计数而不是 `literal` 计数，语义上也标明了"动态"；③ 仅在合并/移动导致计数不变时
  才下调 `scripts/ci/sqlx_literal_production_baseline`。

### R-3 `ORDER BY delete_ts` 无决胜键（P1，正确性）

- 用例：`tests/unit/ts_order_tiebreak_tests.rs:43`
- 实测：`synapse-storage/src/burn_after_read.rs` 单键站点 **2 → 3**
  （`:285 ORDER BY delete_ts ASC`、`:317 ORDER BY p.delete_ts ASC`、`:365 ORDER BY delete_ts ASC`）
- 影响：`delete_ts` 是毫秒级写入值，同一毫秒内批量入队的行**顺序不确定** ⇒ 分批清理时
  "取前 N 条"的结果不可复现（测试偶发、排查困难）。
- 修法：每个站点补第二键（`, id ASC` 即可，这三处都是清扫/取批，`id` 单调且与入队顺序一致）。

### R-4 三处合法空响应未进 allowlist（P3，配置）

- 用例：`tests/unit/placeholder_scan_tests.rs:123`
- 实测缺失条目：`app_service.rs:649 / 668 / 687`
- 判定：**不是功能缺口**。三处分别是 `PUT /transactions/{txn}`（推送确认）、
  `GET /users/{userId}`、`GET /rooms/{alias}`（命名空间查询），Matrix AS API 规定的响应就是 `{}`。
- 修法：在 `scripts/contract/extract_unresolved_allowlist.txt` 登记三条并写明"spec 规定空响应"。

### R-5 `pub trait` 68 → 71（P2，需要决策）

- 新增三个：`RoomParting`（`account_deactivation_service.rs:130`）、
  `PendingDeactivations`（`:172`）、`UserDataPurger`（`:246`）——同一批注销重构引入的 seam。
- 两条路：① 认可这批 seam，**上调** `scripts/ci/trait_count_baseline` 的 `TOTAL` 并写明理由
  （该文件的既有体例就是"只降不升，升必须解释"）；② 合并 `RoomParting` + `UserDataPurger`
  为一个 `RoomPartingAndPurge`（两者都由同一批 storage 实现）。
  建议 ①：它们是**不同方向**的依赖（退房 vs 删数据），合并会把两个变化原因绑在一起。

### R-6 `check_sqlx_cache_fresh.sh` 误报（P1，会挡住所有人的提交）

- 现象：`synapse-storage/src/event/redaction.rs:142:query!` 被判"`.sqlx/` 找不到对应条目"。
- 取证：`.sqlx/query-4b366b1d….json` 的 `query` 字段正是该语句，且**文件名 == sha256(query)**（自洽）；
  而检查器算出的字符串是 `... redacted_by = $3 \n             WHERE ...` —— 它**把 `\` 续行和
  下一行的缩进原样保留**，没有实现 Rust 的"行尾 `\` 删除换行与前导空白"语义
  （`scripts/ci/sqlx_query_census.py` 的 `read_string_literal`，调用点在 `check_cache_fresh`）。
- 佐证：unit 车道用 `SQLX_OFFLINE=true` **完整编译并通过 1875 条用例** ⇒ 缓存本身可用。
- 影响：任何用 `\` 续行写 SQL 字面量的文件都会让 preflight 红，而错误信息会把人引向
  "重新 prepare"（重跑 `sqlx_prepare.sh` 也不会解决，因为问题在解析器）。
- 修法（二选一）：① 修 `read_string_literal`：遇到 `\` + `\n` 时丢弃换行并吞掉续行的前导空白
  （与 Rust 一致），并补一条针对续行字面量的自测；② 把该字面量改成单行（治标，下一处续行仍会踩）。
  建议 ①。

## 3. 代码 / 配置层遗留（非门禁红，但确是缺口）

| ID | 问题 | 证据 | 建议 |
| --- | --- | --- | --- |
| **L-1** | MSC4242 存储面**两个零调用方函数** + 一列无写入者 | `create_state_event_with_dag`（`synapse-storage/src/event/create.rs:280`）与 `get_prev_state_events`（`event/dag.rs:265`）在生产 0 调用；`prev_state_events` 列无写入方（`create_event_with_graph` 留 NULL） | 与已删的 `find_events_referencing_missing_state` 同处置：删或加 `msc4242-unstable` feature 门；现在是"看起来像半个功能"的残留 |
| **L-2** | `worker.worker_app` **配置项从未被读取** | `synapse-common/src/config/worker.rs:19` 定义；全仓除测试无读点 | 要么接线（决定 worker 形态），要么删字段并在示例配置里注明"保留未实现" |
| **L-3** | `stream_writers` 是**纯声明面** | 只有 `worker_topology.rs` 的解析/校验与测试消费，写路径无消费者 | 明确写成"仅声明，尚无消费者"，或在 `GET /_synapse/worker/v1/topology` 的 `valid` 里降级提示 |
| **L-4** | `preview_url` 仍是**桩** | `synapse-services/src/media_service.rs:974` 只回 `{"url"}`（不含任何抓取） | 保持诚实桩即可；若要做真抓取，需同时做 DNS 钉扎与 SSRF 白名单（不是小改动） |
| **L-5** | MSC4354 sticky **无逐 destination 增量补发** | 有 `EduType::StickyEvent`（`synapse-federation/src/edu.rs:38`）与启动重放，但无 per-destination 位点 | 等上游 MSC 定稿；当前形态已在对比报告登记为残余 |
| **L-6** | 稳定 `/{keyName}` profile 路由**未注册**，但 `/versions` 已声明 `m.profile_fields` | `capability_governance.rs:154` 自带 `GAP:` 注释；派生表里只有 `unstable/uk.tcpip.msc4133/...` | 二选一：注册稳定路由（契约链要同步），或在能力里改为 `false`。**声明与实现相反**是最该避免的形态 |
| **L-7** | **全量集成套件从未跑完** | 历次只跑高风险子集（59 条 + 手工 18 条 + 装配 3 条）；文档自认 ~2 h | 拆成"可并行的小 lane"再跑一次全量；否则"没跑过"会一直是无法量化的风险 |
| **L-8** | 20+ 处 `#[ignore]`（有理由，守卫强制） | `tests/e2e/user_flow_tests.rs` 7、`performance/*` 4、`pagination_gate_tests` 3 等；`e2e_honesty_tests` 强制每个 ignore 都写理由 | 保持现状（有理由即可）；建议给"需要真实 homeserver"的那批加一个可选的 `just e2e` 入口 |

## 4. 与"上一轮复核"的差异（口径修正）

上一轮（`c044d0489`）列了 5 项，本轮**其中 3 项已由并行会话修掉**：

| 项 | 当前状态 | 证据 |
| --- | --- | --- |
| UG-11 注销异步可重试退房 | ✅ 已实现 | `d9bc4cb88` + `user_pending_deactivations` 表 + `PendingDeactivations`/`RoomParting` seam |
| M-04 `max_session_age_days` 生效 | ✅ 已实现 | `6bfa2d562`（`AtomicI64` + 四处注入） |
| A4 首写播种 | ✅ 已实现（本轮前一批） | `f804c3917` |
| A5 live 联邦互操作 | ❌ 仍未做 | 需双主机名 + 互信 CA |
| MSC4502 | ✅ 端到端完整（早前已证伪） | handler → service `limit+1` → 存储游标 |

## 5. 已核实为"已修 / 证伪"，**不要**再立项

| 旧结论 | 现状 | 复核位置 |
| --- | --- | --- |
| 客户端撤回不级联（MSC3912） | ✅ 已级联 | `handlers/room/events.rs:1104` 调 `cascade_redact_related_events` |
| Content Scanner 零生产调用点 | ✅ 已有消费者 | `handlers/room/write_access.rs:111` 调 `scan_text_when_enabled`（媒体侧另有 `scan_when_enabled`） |
| `auth_issuer` 仍在册 | ✅ 已删 | `synapse-web/src/routes/` 零命中，另有守卫用例 `test_removed_auth_issuer_endpoint_is_not_served` |
| 停用用户写扩展 profile 字段 404 | ✅ 已修 | `extended_profile.rs:62-66` 用 `user_exists`（注释明写"不要换成 active_user_exists"） |
| Admin 媒体 quarantine 缺 `{server_name}` 形态 | ✅ 已注册 | `admin/media.rs:53-55`（含房间级 quarantine） |
| 缩略图不支持 `animated` | ✅ 已实现 | `media/download.rs:174,266` |
| `search_index` 遗留表 | ✅ 已从基线移除 | `migrations/00000000_unified_schema_v12.sql` 零命中（仅 `artifacts/` 本地旧副本，未跟踪） |
| ledger `query_params` 无消费方 | ✅ 有消费方 | `derived_route_table_worker.inc.rs` / `..._oidc.inc.rs` 均调用 `.with_query_params(..)` |
| MSC4502 / MSC4262 未收敛 | ✅ 均完整 | 见 §4 |
| ELEMENTSEC #6 / #11 | ✅ 已裁决（不受影响） | 对比报告 §19.10 |

## 6. 文档债（这是"遗留问题"反复出现的根因）

- `docs/audit/` 下**非归档文档 31 份**，其中 `PROJECT_REMAINING_ISSUES_2026-09-14.md`
  是 **2026-09-25 的快照**（含大量已修项），却仍被当作"遗留清单"引用 ⇒ 每轮都要重新证伪一遍。
- 同一条结论在 4 份文档里有 4 个版本（本次 §5 的 10 条，每条都能找到"仍存在"的旧文）。
- 建议：
  1. 把本文升为**唯一事实源**（`REMAINING_ISSUES_SWEEP_*.md`，按日期滚动），其余文档只写各自批次；
  2. `PROJECT_REMAINING_ISSUES_2026-09-14.md` 顶部加"已快照化，以最新 SWEEP 为准"并冻结；
  3. 门禁侧加一条"遗留清单类文档只能有一份处于 active"的检查（与 plan 分区棘轮同思路）。

## 7. 建议执行顺序

```
1) R-1 指纹（1 行，解锁 unit 车道）
2) R-6 解析器修复（解锁所有人的 preflight；比 R-2…R-5 更急，因为它让"红"变得不可信）
3) R-2 静态化 8 处（或改 &sql）→ R-3 三处补决胜键（同批，都在 burn_after_read）
4) R-4 三条 allowlist（1 分钟）
5) R-5 决策：上调 trait baseline 并写明理由
6) L-6 声明/实现二选一（唯一一条"对外承诺与能力不一致"）
7) L-1 清理 MSC4242 残留（与已删的孤儿查询同处置）
8) L-7 全量集成套件（需要一次专门的时段，不与其它批次混）
```

**验收判据**：上述 1–6 完成后，`nextest --test unit` 应 1883/1883 通过、
`check_sqlx_dynamic_ratio.sh` 与 `check_trait_ratchet.py` 与 baseline 一致，
`check_sqlx_cache_fresh.sh` 通过；`check_route_contract.sh` 保持绿。
