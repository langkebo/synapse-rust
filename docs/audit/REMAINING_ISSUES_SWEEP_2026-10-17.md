# 遗留问题全面排查（2026-10-17）

> 目的：给"项目还剩什么问题"一个**当前可复核**的答案。本文只列**本轮实测**的结论，
> 每条带文件:行与可复现命令；已被证伪或已修的历史结论统一收进 §5，避免重复立项。
>
> **两段式**：§1–§7 是**排查快照**（基线 `00a94e95a`，只读代码与跑门禁，不动代码）；
> **§8 是执行记录**（同日按 §7 的顺序把 R-1…R-6 与 L-1…L-3 全部闭环，附判据与变异自证）。
> 先读 §8 看现状，§1–§7 保留为"当时为什么这么判"的证据链。

## 0. 一句话现状（执行后）

`cargo nextest --test unit` **1884 tests / 1884 passed**；五条静态棘轮与基线一致；
`check_sqlx_cache_fresh.sh` 通过（解析器缺陷已修，不再误报）；路由契约保持绿。
R-1…R-6 全部闭环，L-1…L-3 已处置（L-1 连 schema 一起清空、L-2 删字段、L-3 加可执行提示）。
L-4…L-8 的结论不变（其中 L-7 仍需一次专门时段）。

## 1. 门禁结果总表（**修复前快照**，基线 `00a94e95a`）

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

---

## 8. 执行记录（2026-10-17，按 §7 的顺序逐项闭环）

> 本节只写**做完之后**的事实：改了什么、判据是什么、变异是否自证。每项都可单独复核。

### 8.1 门禁前后对照

| 门禁 | §1（修复前） | 现在 |
| --- | --- | --- |
| `nextest --test unit --features test-utils` | ❌ 8 failed / 1883 | ✅ **1884 / 1884 passed**（2 skipped） |
| `check_sqlx_dynamic_ratio.sh` | ❌ 生产 104 > 96；测试 770 > 769 | ✅ 生产 96、测试 768、静态 1373、builder 17 —— 四项全等于基线 |
| `check_trait_ratchet.py` | ❌ 71 > 68 | ✅ 71 = 71（带理由上调） |
| `check_sqlx_cache_fresh.sh` | ❌ 误报 1 处 | ✅ 1369 处静态字面量全命中 |
| `check_plan_numbers_fresh.sh` | ✅ | ✅（`.sqlx` 1341、两处基线同步） |
| `check_file_size_ratchet.py` / `check_fmt_ratchet.sh` / `check_clippy_allow_ratchet.py` | ✅ | ✅ |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | ✅（缓存命中） | ✅（真实重编译） |
| `scripts/contract/check_route_contract.sh` | ✅ | ✅（未改路由字面量） |
| `cargo deny check advisories` | ✅ | ✅（未改依赖） |

### 8.2 R-1 基线指纹漂移 —— 已修（1 行 + 记录）

- 实测：`migrations/00000000_unified_schema_v12.sql` 的 FNV-1a 64 与常量不符。
- 根因订正：`0063f09ff` 同时改了迁移与常量，但**写进常量的值本身算错了**（注释记的
  `bytes=237198` 也与实际 237691 不符）。
- 修法：按 R10 第①条独立复算。自检链完整 —— `36221e71b` 的 `236378` 字节 →
  `dda1c0d97cc60b47`，与那次改动之后、`0063f09ff` 之前的实际常量吻合，证明哈希实现未变。
- 五处 `include_str!`（storage / services / e2ee×2 / federation）全部指向同一文件，
  改一处常量即全绿。

### 8.3 R-6 `check_sqlx_cache_fresh.sh` 误报 —— 已修（解析器）

- 根因：`sqlx_query_census.read_string_literal` 未实现 Rust 的行尾 `\` 续行语义
  （原样保留换行 + 缩进），于是任何用续行写的 SQL 字面量都算出与缓存不符的摘要。
- 修法：遇到 `\` + `\n`/`\r\n` 时删除换行并吞掉续行的**全部前导空白**（与 Rust 一致）。
- **变异自证**：把该分支删掉（回到修复前）⇒ 新用例
  `sqlx_cache_fresh_honours_rust_line_continuation` **0 passed / 1 failed**，失败信息为
  "续行字面量按 Rust 语义折叠后应命中缓存"；还原 ⇒ 1 passed，且整仓 1369 处静态字面量全命中。
- 用例本身是**双向**的：正向用折叠后的摘要登记缓存（必须通过），反向用「未折叠」的摘要
  登记（必须失败）—— 否则重写解析器时会静默回归。

### 8.4 R-2 生产区字面量动态 SQL 8 处 —— 已全部静态化

| 文件 | 处数 | 改法 |
| --- | --- | --- |
| `synapse-storage/src/burn_after_read.rs` | 3 | `get_processed_burns` / `purge_processed_burns` / `purge_burn_log` → `query_as!` / `query!` |
| `synapse-storage/src/user_deactivation.rs` | 3 | INSERT / SELECT / DELETE → `query!` / `query_scalar!` / `query!` |
| `synapse-storage/src/membership/mod.rs` | 2 | `get_users_no_longer_sharing_rooms_since` / `ban_member` → `query_scalar!` / `query!` |

- 这 8 处都是**固定 SQL**（不存在"运行期决定文本"的情形），所以走编译期校验而不是
  `&sql`（后者只是把计数从 `literal` 挪到 `param`，把债务藏起来）。
- `.sqlx` 由 `scripts/ci/sqlx_prepare.sh`（唯一入口）对**新迁移的库**重生成：1335 → **1341**。
  净 +6 的账：新增 8 条、两处 `ORDER BY` 变更使 2 条旧条目失效、`prepare` 另清掉 2 条
  **历史陈旧条目**（旧 `ban_member`（无 `updated_ts`）与已删的 MSC4242 孤儿查询）——
  这也顺带说明"缓存里的死条目"此前无人发现，因为 `check_cache_fresh` 只查"源码→缓存"单向。
- `membership/mod.rs` 剩下的 4 处 `runtime` 是既有的合法动态（keyset 分页拼装），未动。

### 8.5 R-3 `ORDER BY delete_ts` 决胜键 —— 三处补齐

`burn_after_read.rs` 的 `get_pending_burns` / `get_processed_burns` / `get_expired_burns`
全部补 `id ASC`（`delete_ts` 是毫秒值，同毫秒批量入队时"取前 N 条"不可复现）。
按"修好要收紧基线"跑了 `check_ts_order_tiebreak.py --update`：`burn_after_read.rs 2 → 0`，
基线只剩一处删除（`69 处 / 29 个文件`）。

### 8.6 R-4 三处 `Ok(empty_json())` —— 是**行号漂移**，不是缺条目

- 原判"未进 allowlist"不准确：`scripts/shell_routes_allowlist.txt` 里本来就有
  `app_service.rs:646/665/684`，但 `a3664aca7` 在文件上方插了 3 行，站点漂到
  `649/668/687`。该文件头部已写明"行号漂移要在同一提交里跟改，这是设计好的信号"。
- 处置：跟改三行并写明漂移来源（同时保留"spec 规定空响应"的语义）。

### 8.7 R-5 `pub trait` 68 → 71 —— 认可这批 seam，带上调理由

`account_deactivation_service.rs` 的 `RoomParting` / `PendingDeactivations` / `UserDataPurger`：
每个都有**生产实现**（`RoomService` / `UserDeactivationStorage` / `StorageUserDataPurger`）与
**测试替身**（`FakeRooms` / `FakePending` / `FakePurger`），且是**不同方向**的依赖
（退房 / 排队 / 删数据）⇒ 不合并。理由块写进 `scripts/ci/trait_count_baseline`，
`STORE_API` 保持 33（存储抽象面未放宽）。

### 8.8 L-6 `m.profile_fields` 声明/实现不一致 —— 已消除（改按路由面推导）

- **原判需要订正**：稳定 `GET/PUT/DELETE /_matrix/client/v3/profile/{user_id}/{key_name}`
  **早就注册了**（`assembly.rs` 的 `// Stable MSC4133: Extended profile fields`）。
  真正的问题是能力位**硬编码 `true`**，与路由面无关 —— 一旦某分支没挂路由，能力位仍说"有"。
- 修法：新增 `profile_fields_capability()`，按
  `manifest_has_route("GET", "/_matrix/client/v3/profile/{user_id}/{key_name}")` 推导；
  同时订正 `capability_governance.rs` 里那条过时的 `GAP:` 注记。
- 回归：`test_msc4133_profile_fields_tracks_the_stable_route_only` —— 正向（稳定路由在服务
  ⇒ true）、反向（只有不稳定命名空间 ⇒ false，且响应体一致）。常量实现无法同时满足两条。

### 8.9 L-1 MSC4242 残留 —— 代码 + schema 一起清空

- 删 `create_state_event_with_dag`（`event/create.rs`，**零调用方**）与
  `get_prev_state_events` + `prev_state_events_from_json`（`event/dag.rs`，零调用方），
  连同三个 `db_tests`（持久化 / 回滚 / malformed-json）与五处已失效的引用注释。
- 从基线移除 `events.prev_state_events` 列 + 两个 partial index（GIN 与
  `(room_id, event_id)`）—— 没有写入方也没有读取方，留着只让 `events` 每次插入多维护两个索引。
- 后果：基线 `237691 → 236554` 字节，模板指纹 `65b6701ed356b3d1 → 16a162d3c01f87ce`；
  静态查询 −4（3 条宏在 `create_state_event_with_dag`、1 条在 `get_prev_state_events`），
  测试区动态夹具 −3 ⇒ 两处基线按实测量化下调（见 §8.1）。

### 8.10 L-2 `worker.worker_app` —— 删除字段

全仓零读取点（除默认值与一条断言它是 `None` 的测试），示例配置也从不使用。留着它的唯一
效果是"配置里写了却不起作用"：运维按上游习惯写 `worker_app: synapse.app.synchrotron`
会以为改了角色，实际角色仍由 `instance_name` 推导。本仓的角色来源保持**单一**
（`worker.instance_name` ↔ `WorkerType::instance_map_keys()` 前缀匹配，
`WORKER_INSTANCE_NAME` 走同一解析器）。serde 默认忽略未知字段，旧配置不会加载失败。

### 8.11 L-3 `stream_writers` 纯声明面 —— 从"沉默"改成"可执行提示"

- 结构体文档明确写"本版本是纯声明面，没有写入路径消费者"。
- `validate_worker_config` 在配置**偏离默认值**（每路仅 `master`）时追加一条 warning，
  并放在 `!enabled` 早退**之前** —— worker 关着但配了分流同样要提示。
- 回归：`topology_flags_stream_writers_as_declarative_only` 三向断言（默认不得有提示、
  偏离必须有、`enabled=false` 也必须有）。

### 8.12 仍未做（结论不变，不属于本轮范围）

| ID | 状态 | 为什么不在本轮 |
| --- | --- | --- |
| **L-4** `preview_url` 桩 | 保持 | 真抓取要同时做 DNS 钉扎 + SSRF 白名单，是独立特性而不是"修缺陷" |
| **L-5** sticky 无逐 destination 补发 | 保持 | 等上游 MSC 定稿；当前形态已在对比报告登记为残余 |
| **L-7** 全量集成套件 | 保持 | 需一次专门时段（~2 h 串行）；本轮只跑受影响的子集 |
| **L-8** `#[ignore]` 20+ 处 | 保持 | 每处都有理由且被 `e2e_honesty_tests` 强制；建议给"需真实 homeserver"那批加 `just e2e` 入口 |
| **A5** live 联邦互操作 | 保持 | 需双主机名 + 互信 CA |
