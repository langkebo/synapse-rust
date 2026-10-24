# synapse-rust 未解决问题清单（living audit）

> 起点：2026-10-22 全维度审查（8 维度 + 冗余专项，原 104 条）
> 本版：**2026-10-24 第七轮**；基线 `01345e38f`
> **本文档只写"还没解决"的东西。** 已关闭条目的完整正文在
> [`archive/COMPREHENSIVE_AUDIT_2026-10-22_closed-detail.md`](./archive/COMPREHENSIVE_AUDIT_2026-10-22_closed-detail.md)，
> 编号与一句话索引见[附录 D](#附录-d已关闭条目索引)。代码里若还写着"见 CODE-03"，从附录 D 进。
> 严重度：**P0** 数据丢失/不可用/认证绕过 ｜ **P1** 破坏规范语义或系统性安全 ｜
> **P2** 明确缺陷、有清晰修法 ｜ **P3** 规范漂移、文档失真、死代码
> 计数口径：所有数字以 `git ls-files` 的受版本控制文件为域（排除 `target/`、排除 `.worktrees/`）

## 0. 总览

| 维度 | P0 | P1 | P2 | P3 | 合计 |
|---|---|---|---|---|---|
| 架构设计 | 0 | 0 | 2 | 1 | 3 |
| 代码质量 | 0 | 0 | 1 | 1 | 2 |
| 功能完整性 | 0 | 0 | 0 | 0 | 0 |
| 性能（含数据层） | 0 | 0 | 1 | 2 | 3 |
| 安全性 | 0 | 0 | 0 | 5 | 5 |
| 可维护性 | 0 | 0 | 1 | 0 | 1 |
| 测试覆盖 | 0 | 0 | 0 | 2 | 2 |
| 文档完整性 | 0 | 0 | 0 | 0 | 0 |
| 专项：冗余与过度开发 | 0 | 0 | 0 | 0 | 0 |
| **合计** | **0** | **0** | **5** | **11** | **16** |

> 计数口径（2026-10-25 重算）：**P0 = 0，P1 = 0，P2 = 5，P3 = 11，合计 16**。
> 来源 = §0.1 的 3 条 open `NEW-*`（NEW-09 / NEW-10 / NEW-11）+ §0.2.2 的 2 条 + §0.2.3 的 11 条。
> `NEW-08` 已于 2026-10-25 关闭（`device_sync.rs` 整模块删除，判据与变异证据见附录 D），不再计入。
> 其中 **14 条是"登记项"**（明确不做或只等外部条件），真正还没动手的只有
> NEW-09（**18** 个端点族仍无 HTTP 往返用例；两波已补 6 族：push / event_report / reactions /
> friend / telemetry / server_notification）与 NEW-11（state 组装层未共享）。

**结论摘要**：**P0 = 0，P1 = 0**（原 9 条 P1 全部有实现 + 能失败的门禁，逐条见附录 D）。
⚠️ 第八轮曾把一条误报成 P1（NEW-06「设备吊销通知静默丢失」），**已撤回**：真实广播路径在
`routes/device.rs::broadcast_device_list_update`（签名事务），死的是 `device_sync` 里那条
零调用者的重复实现，已删除；更正过程记在[附录 D](#附录-d已关闭条目索引)。
剩下的 31 条里，**没有一条会造成数据丢失或越权**；16 条 P2 中真正"用户可见"的只有两类：

1. **能力台账与实现不一致**（FUNC-05 的联邦能力未登记、MAINT-04 的手工台账已出现两行错账）
   —— 表现是"客户端照文档调用却得到另一套行为"；
2. **测试面盲区**（TEST-02 只断言状态码、TEST-05 90 处 `sleep`、TEST-06 5 个 feature 门控整文件）
   —— 表现是"改坏了也绿"。
   其余 P2 是**长期结构债**（ARCH-02 双同步实现、ARCH-04 `events` 未分区、ARCH-05 7 个 >2000 行文件）
   —— 不修也不会立刻出错，但每加一个功能都在加利息。

**下一批该做什么**（按"能不能写出可失败的验证"筛，见 §0.2.1）：
`TEST-05` → `NEW-05`（逐文件判定）。其余要么需要决策
（ARCH-02/04/05），要么只等外部条件（PERF-10 的实例级 PG 配置、PERF-01 的投递语义）。

## 0.1 第七轮新发现（2026-10-24，只列仍开着的）

> 这是第七轮（2026-10-24）新发现里**仍开着**的部分。**已关闭的 8 条不在此重复**，判据与证据见
> [附录 D](#附录-d已关闭条目索引)：除下面列出的 5 条外，还有 **NEW-04**（`scripts/...` 悬挂引用）、
> **NEW-05**（「自证测试」面）。
> 其余已关闭：
> NEW-01（联邦缩略图 `animated` 未透传）、NEW-02（规则文档入库）、DOC-03（MSC3245 错标）、
> **NEW-06（设备吊销通知「静默丢失」—— 误报，已撤回并删除死代码）**、
> **NEW-07（联邦入站设备列表 EDU 对客户端不可见 —— 真缺陷，已修）**、
> **NEW-08（`device_sync.rs` 零调用死模块 —— 已整模块删除）。**

| 编号 | 级别 | 一句话 | 证据 |
|---|---|---|---|
| **NEW-11** | P3 | `sync` 与滑动同步的 **state 组装层**（`lazy_load` / `required_state` / MSC4222 `state_after`）仍各写一遍 —— ARCH-02 只共享了 **timeline** 层 | 要先写「同一房间状态下两条路径的**可见状态集合**一致」的等价断言（懒加载成员集 + `include_redundant_members` + `state_after` 三组语义），没有断言就不抽 —— 与 ARCH-02 同一条原则 |
| **NEW-10** | P3 | 4 个"已带 `break` 条件的轮询循环"仍用裸 `sleep(50ms)` 做间隔（`room_service_tests_migrated` ×3、`api_msc3912_redaction_cascade_tests` ×1） | 折叠成 `tests/common/wait_until(deadline)` 可少 4 个站点并统一超时失败信息；但每个都要改完立刻跑一次真库用例（改坏的表现是"偶发超时"，静态检查看不出来）⇒ 独立一笔 |
| **NEW-09** | P3 | 只锁形状的测试文件里曾有 **24 个端点族没有 HTTP 往返用例**（`shape_only_test_matrix` 里写 `NONE`）—— **两波共补 6 族**（第一波 `push` / `event_report` / `reactions`，第二波 `friend` / `telemetry` / `server_notification`），**剩 18 族** | 逐族「`setup_fresh_test_app` + 注册用户/建房 + 断言 status/body/errcode」，每族一笔提交、每笔都把登记行从 `NONE` 换成真覆盖文件（`check_shape_only_tests.py` 的 C2 会验证该文件**确实**发请求）。判据：两波共 24 条用例全绿（每族 4 条）；守卫实测 `有真覆盖 11 / 缺口 18`（累计新增 6 个 integration 文件，均按「每族一笔提交」落地）。剩下 21 族里较易：`room_summary` / `thread` / `space` / `friend` / `refresh_token` / `registration_token`；较难（要外部角色或签名）：`app_service` / `module` / `worker` / `federation` / `federation_cache` / `admin_extra` |

## 0.2 未完成任务清单

> 这份清单是**唯一权威**。一条算"已完成"当且仅当它出现在某次提交里并给出可复现证据；
> 不在本清单、也不在本轮提交里的编号，只可能出现在附录 D（已关闭索引）。

### 0.2.1 可先红后绿

> 2026-10-24 已清空：最后一条 `TEST-05` 已按「逐处分类 + 注解门禁」收口（见附录 D）。
> 剩下的 19 条全部落在 §0.2.2（需要决策/等价断言）与 §0.2.3（登记项）。

### 0.2.2 需要决策或外部条件

| 编号 | 级别 | 一句话 | 卡在哪 |
|---|---|---|---|
| **ARCH-04** | P2 | `events` 未分区、`room_memberships` 无归档 | 需要真实数据量判断分区收益；本机无生产级数据，属**等外部条件** |
| **ARCH-05** | P2 | 7 个生产文件 >2000 行 | 拿不出可失败验证（拆完行为不变），按 §10.0 降级为**登记项**，不排期 |

### 0.2.3 登记项（已知、明确不做或只等时机）

| 编号 | 级别 | 一句话 | 为什么不做 |
|---|---|---|---|
| **CODE-10** | P2 | 生产 **837 处** `let _ =` / `unwrap_or_default()`（238 文件） | 大部分是合法用法；设下降配额会诱导写 `unwrap()` 来"减少计数"（§10.10） |
| **CODE-11** | P3 | `#[allow(...)]`：棘轮口径（`python3 scripts/ci/check_clippy_allow_ratchet.py --print`，2026-10-24 W8 现算）**160 site / 159 处无 `// reason:`（95 文件）**，基线 159/95；原记「271 处 / 117 文件」是裸 `#[allow` grep 的宽口径，与棘轮不可比 | 棘轮已在工作（只许下降）；剩"补理由"是纯体力活，随触达文件顺手补（W8 触达的 4 个文件/2 个脚本未新增任何无理由 allow，计数 160 → 160、无理由 160 → **159**） |
| **PERF-01** | P2 | `to_device_messages` 对**普通设备**无按龄清理（脱水设备路径有 `DELETE`） | 需要先定"客户端未确认算不算已投递"，是产品语义 |
| **PERF-10** | P3 | PostgreSQL **实例级**未设 `statement_timeout`（应用侧每连接已设 30s） | 只影响直连 psql 的场景，属部署配置，不在仓库内 |
| **PERF-11** | P3 | N+1 查询 3 处（风险可控） | 当前数据量下无实测劣化，缺 benchmark 支撑 |
| **PERF-12 / MAINT-07** | P3 | `INDEXES.md` 人工表无法从 DDL 反推 | 重生成会让"索引治理"退化成"索引清单"；计数已由 `check_doc_metrics.py` 现算 |
| **MAINT-01** | P2 | 测试替身/工具二进制与生产 crate 未做特性隔离 | 已覆盖 `user_store_fake`、`federation_test_keys`；其余属"每个 crate 一次评估"，收益低于改动面 |
| **SEC-09** | P3 | CAS 回调的 `redirect_after` 未做同源校验 | 需要 CAS 部署形态（回调域名）才能定白名单 |
| **SEC-11** | P3 | 静态密钥封装存在全零密钥死代码 | 无运行时路径可达；删它需先确认没有部署依赖 |
| **SEC-12** | P3 | legacy r0/v1/r1 媒体下载端点匿名可读 | **是 Matrix legacy 语义**（新版才要求 token）；收紧会破坏老客户端 |
| **SEC-13** | P3 | 联邦鉴权 DEBUG 日志打印签名原文 | DEBUG 默认关闭；改它要同时改运维的排障手册 |
| **SEC-15** | P3 | 鉴权覆盖率约 94.4%，client/media 面存在未覆盖路由 | 覆盖率本身未重测；补测要先有"哪些路由该公开"的白名单口径 |

## 0.3 复核历史（压缩）

| 轮次 | 日期 | 基线 | 这轮干了什么 | 判定变化 |
|---|---|---|---|---|
| 初轮 | 10-22 | `d3d41618b` | 逐文件静态审查，出 104 条 | — |
| 二轮 | 10-22 | `bf15f173a` | 对 P1 逐条实测 | **4 条实质错误**被更正（PERF-01 主结论不成立、FUNC-01 一半错、FUNC-02 与 RD-19 自相矛盾、PERF-10 口径错），3 条是"把已实现当缺失" |
| 三轮 | 10-22 | `88554436b` | 只重跑数字 + 扩散扫描 | CODE-02 影响面 5 → **19**；新增 CODE-12/FUNC-06/PERF-13/PERF-14 |
| 四轮 | 10-23 | `d86d825fd` | 核 W0–W5 批次是否真落地 | 全部成立；**3 条数字被改错**（`#[ignore]` 11 被"更正"成 16/7 是对的）、`check_no_default_admin_credential.sh` 自匹配自身恒红 |
| 五轮 | 10-23 | `63f030154` | W6 收尾 + 文档数字脚本化 | 新增 `check_doc_metrics.py`；DOC-08 前提不成立 |
| 六轮 | 10-24 | `629110931` | 只重算会漂移的数字 | TEST-04 8 → 4、TEST-05 27/14 → 84/40、CODE-01 维持 🟡 |
| **八轮** | 10-24 | `d96a2144a` | 联邦设备列表链路（NEW-07 修复 + NEW-06 撤回 + 24 条 missing_docs 归零 + SEC-14 裁决） | P1 1 → **0**、missing_docs debt 24 → **0**、SEC-14 判为**误建议**（照做会断联邦）；新增 NEW-08 |
| **七轮** | **10-24** | **`01345e38f`** | **把"已解决"整体移出文档 + 新增 4 条 + 收口 TEST-04/MAINT-05/CODE-09/FUNC-04；按建议执行 3 项决策（ARCH-06 / NEW-02 / DOC-03）** | TEST-04 收口（0 个死门禁）、MAINT-05 5 → **8 处**、FUNC-04 两行改 `TRUE`、新增 NEW-01…04 |

**方法教训（都是自己踩出来的，写在这里省下一轮）**：

1. **`grep` 的 BRE 扩展在本机 shim 下静默返回空** ⇒ 检索一律 `grep -E` 或专用工具；
2. **统计宏/调用点不要用带 `**` 的 git pathspec**（`synapse-*/src/**/*.rs` 会漏掉 `synapse-*/src/*.rs`）；
3. **说"不存在/算不出来"之前先读生成它的代码**（第 3 轮把 `66/75` 判成"没有定义"，其实是另一套口径）；
4. **门禁要剥注释与字符串**：`--run-ignored only` 与本轮元门禁的"探针脚本被判可达"是同一个坑；
5. **`cargo tree -d` 的口径必须写清 `--workspace`**：不加时 80 个 crate 名 / 31 组，加了才与文档的
   47 / 32 / 15 可比（我第一遍就是不加，得出一度"38 → 31"的错误结论）。

## 0.4 数字真值（一律脚本现算，不手抄）

| 度量 | 真值 | 命令 / 来源 |
|---|---|---|
| 测试文件 | **375**（`.rs` **287**；unit **129** / integration **147** / e2e **3** / perf **5**） | `python3 scripts/ci/check_doc_metrics.py --print` |
| 基线索引定义 | **343** | 同上 |
| 活路由 / 派生表行 / `INDEXES.md` 人工表 | **979** / **981** / **134** | 同上（`LIVE_ROUTES` / `DERIVED_TABLE_ROWS` / `INDEXES_DOC_ROWS`） |
| clippy allow | **271 处 / 117 文件**；其中无理由 **160 / 96**（棘轮基线） | 机械扫描 + `CLIPPY_ALLOW_*` |
| `let _ =` / `unwrap_or_default()` / `if let Ok(` | 生产 **837 / 238 文件**；全仓 **1580 / 337 文件** | 机械扫描 |
| `tokio::time::sleep` | **90 处 / 43 文件** | 机械扫描 |
| `>2000` 行 `.rs` | 生产 **7**；测试基建 **2**（`test_isolation.rs` 3305、`test-utils/lib.rs` 2973）；生成 **1**（`derived_route_table_always.inc.rs` 5512）；测试 **3** | `wc -l` |
| 依赖分裂 | crate 名 **47** / 多版本 **32** / 同版本多来源 **15** | `cargo tree -d --workspace --locked` 按 crate 名聚合 |
| `check_*` 门禁脚本 | **47 个，全部可达或已登记（豁免 0）** | `python3 scripts/ci/check_gate_reachability.py` |
| 基线 SQL `updated_ts` 列 | **84 列**：`NOT NULL` **43** / 可空 **42**（`grep updated_ts` 的 86 里含 1 行注释与 1 行 `ALTER`） | `python3 scripts/ci/check_schema_timestamp_convention.py --print` |

> 漂移是正常的（代码在动）。**不要把本表当常量**：下一轮先跑上面这几条命令再引用。

---

## 一、架构设计

### ARCH-02（P2）`sync` 与 `sliding_sync` 无共享语义基座 —— ✅ 已关闭（2026-10-24，先断言后抽取）

- **位置**：`synapse-services/src/sync_service/mod.rs`、`synapse-services/src/sliding_sync_service/mod.rs`
- **事实**：全仓成对交叉比对**未发现复制粘贴的函数体** —— 两条路径各有独立实现。所以这条**不是**
  "重复代码"，而是**同一套语义（懒加载成员、时间线裁剪、`limited`、账户数据）没有共享基座**，
  每改一处语义就要在两个服务里各写一遍。
- **影响**：已经发生过语义漂移（limited timeline 重放成员变更只修了其中一条路径）。

**按原修法的顺序做：先断言、再抽。**

1. **等价断言** `tests/integration/sync_path_equivalence_tests.rs`（3 条，直接调两个 service，
   共用**一份真实房间状态**）：① 完整时间线的事件集合与顺序一致；② 裁剪后两条路径保留的是
   **同一条最新尾巴**；③ 被裁剪时两条路径都置 `limited=true`。
   夹具**不手工写** `sliding_sync_rooms` —— 滑动侧房间行由生产路径
   `materialize_room_from_activity` 从 `room_memberships` + `events` 现算，否则断言会退化成
   "两边读同一份手写数据"，测不出漂移。
2. **断言第一次运行就抓到一条真缺陷**：`EventStorage::get_room_events_batch_inner` 用
   `ROW_NUMBER() OVER (… ORDER BY stream_ordering ASC)` 配 `rn <= limit`，取的是房间**最旧**
   的 N 条 ⇒ `/sync` 的初始同步返回**最早**的 N 条并标 `limited=true`，而 `limited` 只表示
   "还有更早的"，客户端回翻只会更早 ⇒ **长房间的客户端永远看不到最新消息**。
   实测（5 条消息的房间、上限 2）：`/sync` 给 `[…575, …577]`（最旧两条），滑动同步给
   `[…683, …692]`（最新两条）。
   修法：只在**无 `since` 下界**（初始同步）时改成 `ORDER BY stream_ordering DESC`；增量同步
   保持 ASC（水位线之后**最早** N 条，按批追赶）—— 那是既有 catch-up 语义，不动。
3. **timeline 层已共享**：新增 `sync_helpers::timeline_window(events, limit, TimelineOrder)`
   → `TimelineWindow { events（升序）, limited }`，「取哪 N 条 / 输出顺序 / `limited`」只剩这一份
   实现；`/sync` 的 `apply_timeline_limit` 与滑动同步的 `build_timeline` 都改成调它。两条路径的
   真实差异只剩**入参顺序**，用 `TimelineOrder::{NewestFirst, OldestFirst}` 显式化。

**判据（实测）**：抽取前 `truncation_keeps_the_same_tail` 红、另两条绿；修复 + 抽取后
**3 passed**；同期 `sync_service_tests_migrated`(13) + `sliding_sync_service_tests_migrated`(31) +
`sync_handlers_coverage_tests`(15) 全绿；`-p synapse-services --lib --all-features` 的 timeline
相关 **32 条**单测全绿。

**仍未共享（另登记 NEW-11）**：**state 组装层**（`lazy_load` / `required_state` / MSC4222
`state_after`）两条路径仍各自实现。按同一条原则"没有断言就不要抽"，它得先写**可见状态集合一致**
的等价断言（涉及懒加载成员集、`include_redundant_members`、`state_after` 三组语义），属独立一笔。
所以本条关闭的是"**时间线语义**单点化 + 有断言守护"，不是"整个组装层已合并"。

### ARCH-03（P2）"未实现"存在两套错误语义

- **位置**：`synapse-common/src/error.rs:314`（`not_implemented` → 404 `M_UNRECOGNIZED`）
  与 `:449`（`not_supported` → 501 `M_UNKNOWN`）
- **事实**：两者**并存且都有使用者**（`voice.rs` 用前者，`account_compat.rs` / `room/events.rs` 用后者）。
  语义区分是刻意的：*端点不存在*（404）vs *端点存在、字段/扩展不支持*（501）。
- **仍开着的部分**：客户端拿不到"该降级还是该换端点"的统一信号；`M_UNKNOWN` 与 `M_UNRECOGNIZED`
  在 501/404 之间混用，没有对外的错误码契约。
- **本轮已修的小错（新发现）**：`:449` 的 docstring 把 `not_implemented` 说成 "404 `M_UNIMPLEMENTED`"
  —— 该 errcode 在本仓**不存在**，wire string 是 `M_UNRECOGNIZED`。
- **修法**：定稿一份"未实现语义表"（哪个用 404、哪个用 501），写进 `ROUTE_CONTRACT.md`，
  然后全仓对齐 12 个调用点。

### ARCH-04（P2）`events` 未分区、`room_memberships` 无归档

- **位置**：`migrations/00000000_unified_schema_v12.sql`（`events` 建表块，注释里保留了未实现的分区说明）；
  `synapse-services/src/admin/retention.rs`（保留策略按单条 `DELETE`）
- **影响**：消息表随运行时间线性膨胀，索引与 autovacuum 成本持续上升；单条 `DELETE` 会产生长事务
  与大量死元组，大房间上可能拖垮 WAL 与复制延迟。
- **现状**：W4 只做了**分批 prune**（`PERF-06`，已关闭）；分区/归档未动。
- **修法**：按 `origin_server_ts` 声明式分区（或至少按月归档表）。

### ARCH-05（P2）7 个生产文件 >2000 行

| 文件 | 行数 |
|---|---|
| `synapse-storage/src/room/mod.rs` | 2531 |
| `synapse-storage/src/membership/mod.rs` | 2502 |
| `synapse-storage/src/refresh_token/mod.rs` | 2478 |
| `synapse-common/src/error.rs` | 2442 |
| `synapse-storage/src/device/mod.rs` | 2382 |
| `synapse-common/src/config/mod.rs` | 2324 |
| `synapse-federation/src/client.rs` | 2086 |

- **口径边界**：以下**不计入**拆分目标 —— 生成物 `derived_route_table_always.inc.rs`（5512，改生成器而非产物）、
  测试基建 `synapse-common/src/test_isolation.rs`（3305）与 `synapse-test-utils/src/lib.rs`（2973）、
  测试文件 `tests/integration/room_service_tests_migrated.rs`（5255）等 3 个。
- **为什么登记而不排期**：拿不出"先红后绿"的验证 —— 拆分的行为结果应完全不变，无法证明"修好了"。

---

## 二、代码质量

### CODE-01b（P2）`hash_token` 缺 secret 时 panic；8 个 `src/bin` 入口完全不校验

- **位置**：`synapse-common/src/crypto.rs`（`hash_token` 的 `unreachable!`，已带
  `#[expect(clippy::unreachable, reason = …)]` 登记）；`src/bin/` 下 8 个可执行入口
- **现状**：CODE-01 的门禁部分已收口 —— `--lib --bins` 口径的 `unreachable!()` 从 4 处降到 **1 处**
  （就是这里的 fail-closed），门禁 = clippy `-D clippy::unreachable` × 三处副本（`ci.yml` /
  `.githooks/pre-push` / `preflight.sh`），由
  `tests/unit/ci_test_scope_tests.rs::production_code_denies_unreachable_macro` 守着。
- **仍开着的部分**：`hash_token` 的签名没改成 `Result`。主服务在 `src/server/mod.rs` 有
  `validate_token_hash_secret` 兜底，但**8 个 `src/bin` 入口没有任何校验** —— release 构建下
  少一个 env 就是启动即 panic。
- **修法**：`hash_token` → `Result`，连带 `verify_token_hash`（bool → `Result<bool>`）与 8 个入口的
  启动路径。这是可独立审查的一笔改动，不该混在"补门禁"里。

### CODE-06（P2）超长函数：口径待定，不改判

- **原记录**：9 个函数 >150 行（最长 461 行）。
- **本轮机械重算**：生产文件里 `fn` 体 >150 行的有 **52** 条 —— 但其中包含 `#[cfg(test)] mod` 里的
  测试函数与生成文件，**与原口径不可比**。按"先定义口径再统计"的纪律，本轮**不改判**。
- **下一轮要做的事**：先定义三条边界（是否含 `#[cfg(test)]`、是否含生成物、是否按行数还是圈复杂度），
  然后再报数。**不要**把 52 写进缺陷清单 —— 那是口径混出来的假增长。

### CODE-08（P2）已注册端点恒 403，用无理由 `#[allow(dead_code)]` 掩盖

- **位置**：`synapse-web/src/routes/e2ee/devices.rs:108` 一带（`GET /_matrix/client/v3/rooms/{room_id}/keys/distribution`
  恒 403，自述 "server-internal endpoint"）
- **现状**：该端点已由 `check_unsupported_endpoints.py` 的 16 条 by-design 清单覆盖（双向比对，
  恒失败 handler 必须登记）。**剩下的问题是那个 `#[allow(dead_code)]` 没有 `reason`**，
  读代码的人分不清"预留"与"忘了删"。
- **修法**：补 `reason = "by-design unsupported: …"` 指向契约条目。

### CODE-10（P2）生产 837 处 `let _ =` / `unwrap_or_default()`，无收敛策略

- **口径**：生产 **837 处 / 238 文件**；全仓 **1580 / 337 文件**（§0.4 有命令）。
- **已做**：W0 只加了"危险性扫描"（缓存/存储删除禁止 `let _`）+ 3 个 helper 收敛 19 处缓存失效。
- **明确不做**：总数下降配额（§10.10：配额会诱导写 `unwrap()` 来"减少计数"）。
- **下一轮可做的**：按"错误被吞后有没有用户可见后果"分类，只处理前两类；不要动 `unwrap_or_default()`
  在 `Option` 语义下的合法用法。

### CODE-11（P3）`#[allow(...)]` 271 处 / 117 文件，其中 160 处没有 `// reason:`

- **现状**：`check_clippy_allow_ratchet.py` + `clippy_allow_baseline` 已在工作（只许下降，
  新条目必须带理由），所以**"无理由 allow 增长"已经不可能**。
- **仍开着的部分**：存量 160 处没理由。**修法**：随触达文件顺手补 `reason`，不要为一个纯体力活单开批次。

---

## 三、功能完整性

### FUNC-05（P3）联邦能力项有意不注册，未全部登记为设计决策

- **已登记**：thirdparty 4 条 + 联邦 key 上传 5 条（RD-15/16）。
- **未登记**：`/_synapse/admin/v1/backups` 等"无路由"项，读者会误判为遗漏。
- **修法**：随能力台账重做（MAINT-04）一起进 `ROUTE_CONTRACT.md` 的生成段。

---

## 四、性能表现（含数据层）

### PERF-01（P2）`to_device_messages` 对普通设备无按龄清理

- **事实**：`/sync` 走 `get_messages_since(user, device, since, limit)`（增量 + 有界），**不是**全量读取
  —— 初轮的"P1 全量读取"已被第二轮证伪。真正的问题是该表**对普通设备没有任何 retention**：
  全仓唯一的 `DELETE FROM to_device_messages` 在脱水设备路径（`dehydrated_device.rs`）。
- **影响**：长期不活跃设备的消息永久累积；配合 RD-27 的"名字相反的死 API"，排查时会查错方向。
- **修法**：加按龄 prune（复用 `PERF-06` 的 `ctid` 分批模式）；**先定语义**——"客户端未确认算不算已投递"。

### PERF-10（P3）PostgreSQL 实例级未设 `statement_timeout`

- **事实**：应用为每个连接 `SET statement_timeout='30s'`，并配了 `idle_in_transaction_timeout_secs: 60`；
  缺的只是**实例级**（`postgresql.conf`），因此直连 psql 的运维查询不受保护。
- **修法**：部署侧加 `statement_timeout`；仓库内只能改 `docker/config/postgres.conf` 并在文档注明
  "应用侧已兜底，这里防的是直连"。

### PERF-11（P3）N+1 查询 3 处（风险可控）

- **位置**：原记录 3 处（列表查询里按行再查一次）。**缺少 benchmark 支撑**，当前数据量下无实测劣化。
- **修法**：等 `bench` 车道覆盖到这几条路径再改；不要在无测量依据时换 `JOIN`（会改变内存形状）。

### PERF-12 / MAINT-07（P3）`INDEXES.md` 与 SQL 的计数漂移（整份生成明确不做）

- **已做**：计数与"人工表行数"进了 `doc-metrics` 块，由 `check_doc_metrics.py` 现算校验（343 / 134）。
- **不做**：整份重生成。人工表里的"为什么需要这个索引"**无法从 DDL 反推**，生成 343 行会让文档
  从"索引治理"退化成"索引清单"（§10.10）。

---

## 五、安全性

> 原 15 条安全项里 8 条已关闭（SEC-01…SEC-08、SEC-10）。剩下 6 条全是 P3，且没有一条是"越权"：
> 四条是**运维面**（实例配置、日志、部署形态），两条是**legacy 语义**（矩阵老客户端要求匿名可读）。

### SEC-09（P3）CAS 回调的 `redirect_after` 未做同源校验

- **位置**：`synapse-web/src/routes/cas.rs`（`/login?service={redirect}`）
- **影响**：开放重定向 —— 攻击者可构造看起来指向本域的 CAS 登录链接，登录后跳去外站。
- **修法**：`redirect_after` 只接受同源路径；非空且非同源 ⇒ 400。

### SEC-11（P3）静态密钥封装存在全零密钥死代码

- **位置**：`synapse-common/src/key_encryption.rs`（`let mut okm = [0u8; 32]`）一带
- **影响**：死路径，但"全零密钥"出现在密钥封装里会被读成"可能用了全零密钥"。
- **修法**：删除或改成显式 `unreachable`（需要 CODE-01 的 `#[expect]` 形式登记）。

### SEC-12（P3）legacy r0/v1/r1 媒体下载端点匿名可读

- **位置**：`synapse-web/src/routes/media/`（`create_media_r0_router` / `r1` / `v1` 的下载 handler
  用 `OptionalAuthenticatedUser`）
- **判据**：这是 **Matrix legacy 语义**（v1.11 之前媒体仓库允许匿名读），收紧会破坏老客户端；
  新版 `v3` 路径已要求鉴权。
- **修法**：登记为设计决策 + 在部署文档里说明"如需关闭匿名读，需按客户端版本分布决定"。

### SEC-13（P3）联邦鉴权 DEBUG 日志打印签名原文

- **位置**：`synapse-web/src/middleware/federation_auth.rs`
- **影响**：DEBUG 默认关闭；一旦为排障打开，日志里就有可重放的签名原文（配合 SEC-14 的窗口问题更危险）。
- **修法**：改成打印签名哈希的前 8 位。

### SEC-14（P3）联邦请求缺 `ts` 仅告警不拒绝

- **位置**：`synapse-web/src/middleware/federation_auth.rs`（`signing_ts` 缺失走 warn 分支）
- **影响**：无 `ts` 的请求无法参与重放窗口判定，等价于"跳过新鲜度检查"。
- **修法**：缺失即拒绝（401），并补一条"缺 ts ⇒ 401"的用例（**先写用例证明当前会放行**）。

### SEC-15（P3）鉴权覆盖率约 94.4%，client/media 面存在未覆盖路由

- **事实**：覆盖率本身**未重测**（原值是静态统计）。
- **修法**：先重测，再按"哪些路由本来就该公开"建白名单；没有白名单口径就没法判断 5.6% 里哪些是真缺口。

---

## 六、可维护性

### MAINT-01（P2）测试替身、工具二进制与生产 crate 未做特性隔离

- **已做**：`user_store_fake`（RD-07）、`federation_test_keys`（RD-08）已加 `#[cfg(any(test, feature = "test-utils"))]`。
- **仍开着**：其余测试工具（`test_mocks`、`synapse-test-utils` 的部分模块）仍随默认构建进入生产依赖图。
- **修法**：逐 crate 评估——不是一次性机械替换，每个 crate 的 `test-utils` feature 传播路径不同。

### MAINT-04（P2）能力台账手工维护，已出现多处失真

- **证据**：FUNC-04 的两行（`animated`、`M_APPSERVICE_LOGIN_UNSUPPORTED`）**两轮都被判为"与实现相反"**，
  本轮已改对并加了守卫（`capability_ledger_absence_claims_match_the_code`：代码里有该标识符 ⇒
  台账不许说 `MISSING` / `0 命中`）。
- **根因仍在**：整张表靠手抄。生成器要能反推"为什么没实现"，做不到；**折中**是把"证据必须可复现"
  变成判据（本轮做的），而不是生成整张表。

### MAINT-07（P3）索引台账与迁移说明未同步

- 见 PERF-12：计数已脚本化，整份重生成明确不做。

---

## 七、测试覆盖

### TEST-02（P2）断言强度不足：逐文件状态（2026-10-24 重写）

原条目写"6 个测试文件只断言状态码"。逐个打开后发现**三种不同的病**，混在同一条里：

| 文件 | 原状 | 现在 |
|---|---|---|
| `api_admin_target_guard_tests.rs` | 8 个用例只断言 403 | ✅ **已补**：`assert_forbidden` 断言 403 + `M_FORBIDDEN` + 非空 `error`；成功分支断言 `id_server_unbind_result=success` |
| `api_worker_replication_auth_tests.rs` | 3 个用例只断言 200/401/404 | ✅ **已补**：401 断言 `M_UNAUTHORIZED` + error；心跳 200 断言 `{status:"ok"}`；404 断言带 `M_` errcode；admin 列表断言**真的含刚注册的 worker_id**（空数组也能 200） |
| `api_federation_key_fetch_limits_tests.rs` | 4 处只断言 404/200（另有 metrics 断言，所以不是纯状态码） | ✅ **已补**：404 断言 `M_NOT_FOUND` + error；成功断言 `{server_keys:[...]}` 非空 |
| `api_security_headers_tests.rs` | 状态码 + 安全头断言 | ✅ **不适用**：它的被测对象**就是响应头**（`nosniff` / `referrer-policy` / CSP），body 不是契约面 |
| `api_route_ledger_tests.rs` | 路由台账 / 快照断言（46 处断言） | ✅ **不适用**：它测的是 ledger 与快照，不发 HTTP |
| `msc4108_rendezvous_route_tests.rs` | **自证**：`let status = StatusCode::OK; assert_eq!(status, StatusCode::OK)`，0 个 HTTP 调用 | ✅ **已修（W8）**：12 个自证用例删除，unit 文件只留路由台账 + 真函数调用；真实往返（200/304/400/404/412/413/204 + ETag 变化）搬到 `tests/integration/msc4108_rendezvous_route_tests.rs`（12 条，真 router + 真库） |

**判据（本轮实测）**：把 `assert_forbidden` 的期望 errcode 改成 `M_UNKNOWN` ⇒
`FAIL … 403 必须带 M_FORBIDDEN … 实际 body = {"errcode":"M_FORBIDDEN","error":"Only super_admin …"}`；
把心跳响应期望改成 `"done"` ⇒ `FAIL … 心跳成功响应必须是 {"status":"ok"}`。两次都还原字节一致。

**原"仍开着的部分"已关闭（W8）**：`msc4108` 那 535 行里 12/20 个用例是自证，它需要的不是"补 body
断言"而是**接到真 router 上** —— 已做：unit 文件只留路由台账 + 真函数，真实往返搬到
`tests/integration/msc4108_rendezvous_route_tests.rs`（12 条，真 router + 真库）。
**变异实测**：把 `create_session_without_content_type_is_rejected` 的期望 errcode 改成 `M_UNKNOWN`
⇒ `left: "M_MISSING_PARAM" / right: "M_UNKNOWN"`，红；还原后绿。同类文件的规模见 NEW-05。

### NEW-05（P2）「自证测试」面 —— ✅ 已关闭（W8，逐文件判定完成）

**形态**（标本是 `tests/unit/msc4108_rendezvous_route_tests.rs` 的旧版）：

```rust
#[test]
fn test_send_captcha_request() {
    let captcha = json!({ "captcha_type": "image", "target": "@user:@localhost" });
    assert!(captcha.get("captcha_type").is_some());   // 断言自己刚放进去的字段
}
```

它不构造 router、不发请求、不碰 DB ⇒ **改坏生产 handler 也不会红**，而文件名是
`*_api_tests.rs` / `*_route_tests.rs` —— 看板与审查者都以为这条链路被覆盖了。

**判定口径（定稿，写进 `scripts/ci/shape_only_test_matrix` 头部以免下一轮重猜）**：

- ① 接真（真 router + 真库，或真调生产函数）⇒ 文件名可以继续叫 `*_route_tests`；
- ② 只能测形状 ⇒ 文件名必须带 `_shape_only_tests`，文件头 20 行内写明「只锁形状」，
  并在 `scripts/ci/shape_only_test_matrix` 登记「真覆盖文件」或 `NONE`（= 已登记的覆盖缺口）。

**做了什么（2026-10-24）**：

| 动作 | 数量 | 明细 |
|---|---|---|
| 改名 + 声明 + 登记（②） | **29** | 16 个来自"≥3 处本地字面量"的启发式候选（`captcha`/`e2ee`/`federation_cache`/`formatting`/`media`/`media_quota`/`reactions`/`refresh_token`/`registration_token`/`retention`/`server_notification`/`sliding_sync`/`admin`/`key_backup`/`rate_limit` + `integration/coverage`），**13 个是 C3 规则新发现的**（启发式漏掉的同模板文件：`admin_extra`/`app_service`/`background_update`/`event_report`/`federation`/`friend`/`module`/`push`/`room_summary`/`space`/`telemetry`/`thread`/`worker`）——它们的 `.get(..).is_some()` 形态不足 3 处，所以旧口径数不到 |
| 保留原名（①/有效） | **15** | `*_{api,route}_tests.rs` 里仍在册的那些都**真的碰生产代码**（路由台账 `declared_ledger_all()`、DTO 反序列化与 validator、`resolve_rotation_interval_ms` 等），由门禁 C3 逐个核过 |
| 真覆盖存在 | **5** | `e2ee`→`api_e2ee_advanced_tests`、`media`→`api_media_routes_tests`、`retention`→`api_room_retention_tests`、`key_backup`→`api_key_backup_route_table_tests`、`rate_limit`→`api_rate_limit_contract_tests`（门禁会验证这些文件**真的**发请求/碰 DB，填了不真的也算红） |
| 覆盖缺口（登记为 `NONE`） | **24** | 这些端点族**至今没有任何 HTTP 往返用例** —— 见下面的新登记项 |

**新门禁** `scripts/ci/check_shape_only_tests.py`（接入 `preflight.sh`，每次提交都跑）：

- C1 登记：`*_shape_only_tests.rs` 必须在表里 + 文件头有声明（只有名字不算说清楚）；
- C2 真覆盖：表里填的覆盖文件必须存在且含真实依赖标记（`oneshot`/`Request::builder`/
  `create_router`/`setup_fresh_test_app`/`sqlx::query`）；
- C3 名字承诺：`*_{api,route}_tests.rs` 必须「调生产符号」或「发请求」，否则红；
- C4 过期条目：文件删了 / 声明被摘 / 改名没同步 ⇒ 红。

**判据（实测）**：首次运行 C3 直接抓出 **13 个**启发式漏掉的文件（`admin_extra` 等），
改名前 `结论：14 项违规`（含 `msc4108` 的误报 —— C3 最初只认 `synapse_*::`，已补"发请求也算"）；
改名+登记后 `OK —— 29 个只锁形状、5 个有真覆盖、24 个缺口已登记`。
**变异**：把 `captcha_shape_only_tests.rs` 的文件头声明删掉 ⇒ C1 红；把任一 `*_shape_only_tests.rs`
改回 `*_api_tests.rs` ⇒ C3 红（两者都实测）。

**残留（不在本条内，另登记为 NEW-09）**：24 个 `NONE` 是**真覆盖缺口**，不是"测过了"。
本条只负责把"假的覆盖"变成"显式的缺口"，补真覆盖是另一件事（每个都要 `setup_fresh_test_app`
- 注册用户 + 逐端点断言，属独立批次）。

---

### TEST-05（P2）测试域 `sleep` 站点 —— ✅ 已关闭（2026-10-24，判据改判）

> ⚠️ 本节在 2026-10-24 的 NEW-05 改写里被误删过一次（那次 splice 的结束锚点越过了本节），
> 现在按当时的内容 + 本轮分类重建；`check_plan_doc_sections.py` 的基线只校验章节数，拦不住这种删法。

分布（`python3 scripts/ci/check_sleep_ratchet.py --print` 现算）：**PROD 26 / TEST_INLINE 34 /
TEST_FILE 47 = 107**；测试域合计 81。

**`tests/**`（35 处，成员 A 的范围）已全部带 `// sleep-reason: <类别>`**，分五类：

| 类别 | 处数 | 代表站点 |
|---|---|---|
| 故障注入（模拟慢上游 / 故意超预算） | 5 | `api_federation_key_fetch_limits_tests` ×2、`api_federation_join_key_fetch_priority_tests`、`manual_smoke_tests` 与 `server_router_assembly_tests` 的 30s third-party rule |
| 负载模拟 / 节流 | 8 | `tests/performance/api_load_tests.rs`：sleep 就是被模拟的请求延迟与压测节流 |
| 产品语义的时间窗 | 4 | 恢复窗口 5.2s（`room_service_tests_migrated` ×2、`api_telemetry_alerts_tests`）、缓存 TTL 到期（`cache_tests`） |
| 时间戳铺开 | 5 | `friend_room_storage_tests_migrated` ×3、`threepid_storage_tests_migrated`、`space_children_service_tests` |
| 条件轮询的间隔 | 13 | `room_service_tests_migrated` ×3（外层 `timeout` + `break`）、`api_msc3912_redaction_cascade_tests`、`tests/integration/mod.rs`（连接池退避）、`tests/e2e/user_flow_tests.rs` ×7（等外部 homeserver 就绪） |

另把两处**误命中**的断言文案改掉（`not the rule's 30s sleep (took …)` 里的 `sleep (` 会被棘轮当成
站点）⇒ 改成 `30s delay`。属口径噪声，不是行为改动。

**新判据（可失败）**：`check_sleep_ratchet.py` 要求测试域每一处 `sleep` 在同一行或前两条注释内有
`sleep-reason:`，缺注解即红。变异实测：新增一处裸 `sleep` ⇒ `FAIL 测试域有 1 处 … 没有
// sleep-reason: 注解 … cache_tests.rs:470`；删掉既有注解 ⇒ 同款红。

**2026-10-24 20:25 收口（不分 A/B 之后把 src 侧也做了）**：`synapse-*/src/**` 里剩下的
**46 处**（`TEST_INLINE` 34 + `src/**/*tests.rs` 12）逐处加了同样的 `// sleep-reason:`：

| 类别 | 处数 | 代表 |
|---|---|---|
| 等 TTL / 锁 / 会话过期 | 15 | `synapse-cache/src/tests.rs` ×7（1s TTL）、`circuit_breaker` ×5（open→half-open 150ms）、`background_update` ×3、`call_session`、`login_lockout` |
| 时间戳铺开 / 时间源精度 | 7 | `presence`、`saml/db_tests`、`megolm/storage`、`olm/session`、`database_initializer`、`test_isolation`、`time.rs` |
| 等后台任务就绪（waiter 注册 / janitor / 驱逐器） | 12 | `event_notifier` ×6、`test_schema_guard` ×2、`media/quarantine_tests`、`sync_service/tests`、`password_hash_pool`、`test_isolation` |
| 故障注入 | 5 | `module_service` 30s、`security.rs` 90s/40s、`src/server/mod.rs` 4.2s 恢复窗口 |
| 契约/指标窗口 | 7 | `federation_signature_cache` ×2、`invite_blocklist` ×3、`synapse-cache/src/tests.rs` 任务节流、`src/server/mod.rs` |

**判据（最终）**：`check_sleep_ratchet.py` 要求**测试域 81 处全部**带 `sleep-reason:`，
基线键 `TEST_FILE_SRC_UNANNOTATED` 从 12 收紧到 **0**；新增一处未注解的 `sleep` 即红
（两处变异实测：插裸 sleep、删既有注解）。**计数仍是 26/34/47=107** —— 本轮把判据从
"计数下降"改成"**每一处都有类别、且新增必须报类别**"，因为逐处复核后确认：81 处里
**没有一处是"随便睡一下等它好"**，全部对应故障注入 / TTL / 时间窗 / 时间戳铺开 / 已被
`break` 条件包住的轮询。真正可省的只有 4 个轮询循环的间隔（折叠成
`tests/common/wait_until(deadline)` 可少 4 个站点、并统一超时失败信息），已登记为
`NEW-10`（独立一笔，改一个跑一个真库）。

### TEST-06（P2）5 个整文件以 feature 门控 —— ✅ 已关闭（W8，见附录 D）

判据与实现见[附录 D](#附录-d已关闭条目索引) 的 TEST-06 行（`check_test_feature_gates.py` +
`scripts/ci/feature_gated_test_matrix`）。

## 八、文档完整性

### DOC-04（P3）三份文档共 9 组行号漂移

- **修法**：把"引用行号"改成"引用符号名"（`fn xxx` / `const YYY`），行号每一次改动都会漂。

---

## 九、专项：冗余与过度开发

> W6 五批（`e4ac05ce0` / `b51aafb00` / `f7bf4eec0` / `d9398fed1` + RD-14 接入）已关闭
> **RD-01…RD-26**（含 26 个条目、3 处"审计原文结论作废"的偏差记录）。剩余 1 条：

### RD-27（P2）`to_device` 的 4 个存储方法零调用，其中一个名字与语义相反

- **位置**：`synapse-e2ee/src/to_device/storage.rs`（`get_messages`、`get_and_delete_messages`、
  `delete_messages`、`delete_messages_up_to`）
- **事实**：全仓零调用；`/sync` 实际走 `get_messages_since`（带 `LIMIT`）。
  **`get_and_delete_messages` 的名字是反的** —— 生效路径**不删任何行**，PERF-01 的初判就是被它误导的。
- **修法**：删除 4 个方法（`pub` 方法是破坏性变更，先确认无外部消费者）；若确实要"投递后清理"，
  在 `get_messages_since` 的调用方显式实现。

---

## 十、后续方案

### 10.1 批次状态

| 批次 | 主题 | 状态 |
|---|---|---|
| W0 | P1 一致性收口（CODE-02/03） | ✅ `8b8505ff5` + `076309e7c`（事务化） |
| W1 | P1 配额与事务（FUNC-06/03、PERF-02） | ✅ `cc364bd6f` |
| W2 | P1 授权与信任根（SEC-01、CODE-05、TEST-01） | ✅ `85180061c` |
| W3 | P1 结构决策（ARCH-01 Thread） | ✅ `3789583ef` |
| W4 | P2 资源与性能（PERF-03…09/13/14、CODE-04/07） | ✅ 三层：`2358c0f5a` / `0d18fede0` / `3cf5c006a` |
| W5 | P2 安全一致性（SEC-02…08） | ✅ `ca6980c51` |
| W6 | P3 冗余与文档（RD-01…26） | ✅ 四批 + RD-14 接入 |
| W7 | 门禁可达性（TEST-04）+ 三处文档失真（MAINT-05/CODE-09/FUNC-04） | ✅ 见提交 `01345e38f` |

### 10.2 下一批（W8）建议

**顺序**：`TEST-05` → `NEW-05`（逐文件判定）→ `FUNC-05`。清单里已无 P0/P1。

理由：`TEST-05` 剩余 81 处测试域 `sleep` 是体力活但收益最长尾；`NEW-05` 要先逐文件判定"接真 router 还是改名"才有判据；`FUNC-05` 只差把已登记的能力项接进生成段（MAINT-04 的前置）。
**需要先确认规范是否允许缺省** —— 硬拒可能与老版本 homeserver 断联邦，所以它排在需要判断的位置。

### 10.3 明确不做（防止过度开发）

| 不做 | 理由 |
|---|---|
| 为 CODE-10 设总数下降配额 | 会诱导写 `unwrap()` 来"减少计数" |
| 整份重生成 `INDEXES.md` | 会让"索引治理"退化成"索引清单" |
| 拆分测试文件（3 个 >2000 行） | 收益（并行编译）远低于冲突成本 |
| 下架 voice 三端点 / `_synapse/admin/v1/backups` | 只登记不下架（by-design） |
| 强行合并 `sync` 与 `sliding_sync` | 那是重写而非重构 |
| 用朴素判据做"文档路径必须存在" | 实测 30 个悬挂里有 28 个是相对路径/历史叙述，会假红（NEW-04） |

### 10.4 验收口径（怎么算"这一轮收口了"）

一条条目算完成，必须同时满足：

1. **有提交**（不是"工作区改好了"）；
2. **有能失败的证据**：新增/修改的用例在修复前红、修复后绿，或门禁在变异下红（**变异自证**）；
3. **有回填**：本文件的对应条目移入附录 D（编号 + 一句话 + 提交号），且 `§0` 计数同步；
4. **没引入新的三角债**：`check_doc_metrics.py` / `check_gate_reachability.py` / 全量 `--lib` 测试仍绿。

---

## 附录 A：已核实为"非缺陷"的项（避免重复排查）

| 项 | 结论 | 依据 |
|---|---|---|
| 220 张表的**主键** | 全部有 PK | 逐块解析基线 SQL：`CREATE TABLE` 220 个，无 PK 的 **0** 个 |
| `ApiError` 是否外泄内部错误 | **不会**：`Internal` 一律替换成 "An internal error occurred" | `synapse-common/src/error.rs::message()` |
| `not_implemented` 的 HTTP 状态 | 404 + `M_UNRECOGNIZED`（Matrix 无 501 的 errcode） | `error/code.rs::Unimplemented` → `M_UNRECOGNIZED`，`http_status()` = 404 |
| `to_device` 的 `/sync` 读取 | **不是全量**：`get_messages_since` 带 `LIMIT` | `synapse-e2ee/src/to_device/storage.rs:365-390` |
| voice 三端点的"恒失败" | **by-design**（MSC3245 规定语音处理在客户端） | `voice.rs` 的 handler 均返回 `not_implemented` 带 MSC 说明 |
| `_matrix/media/r0` 匿名下载 | **by-design legacy 语义**，不是越权（见 SEC-12） | legacy 客户端兼容 |
| `#[cfg(test)]` 里的 `unreachable!()` | 正当写法，**不该**被门禁收进来 | CODE-01 的门禁作用域刻意是 `--lib --bins` |

## 附录 B：本次审查未覆盖

1. **benchmark / 压测**：全轮未跑，PERF-* 全部是静态推断（除已由 `schema-health-check.yml` 的
   `EXPLAIN` 断言覆盖的三条索引查询）。
2. **端到端 E2EE 互操作**：`E2EE_INTEROP=1` 未在本轮跑。
3. **`cargo tree -d` 的"处置进度"**：本版只重算了组数（32），未逐族复核 tracker 的处置建议。
4. **真实数据量下的分区收益**（ARCH-04）：需要生产量级数据，本机没有。

## 附录 C：与历史清单的关系

| 文档 | 关系 |
|---|---|
| `archive/COMPREHENSIVE_AUDIT_2026-10-22_closed-detail.md` | 本文件的前身，**含全部已关闭条目的完整正文**（2143 行）。要查"当时为什么这么判"进这里 |
| `COMPREHENSIVE_AUDIT_2026-10-22.md`（本文件） | 唯一权威的**未解决**清单 |
| `UNRESOLVED_ISSUES_SUMMARY.md` | 头部已标 `STATUS: HISTORICAL`，指向本文件 |
| `docs/synapse-rust/ROUTE_CONTRACT.md` | 路由契约的唯一事实来源（本文件不重复它的计数） |
| `docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md` | 依赖分裂的唯一权威口径（数字待按 NEW-03 更新） |
| `docs/audit/SHIT_MOUNTAIN_AUDIT_2026-10-23.md` | 另一视角（"下一次做正确改动的成本"），与本文件不互相替代 |

## 附录 D：已关闭条目索引

> 只留"编号 + 一句话 + 关闭依据"，正文在归档文件里。**代码注释里引用的编号从这里进。**

| 编号 | 一句话 | 关闭依据 |
|---|---|---|
| ARCH-02 | `sync` 与 `sliding_sync` 无共享语义基座（时间线裁剪/顺序/`limited` 各写一遍） | **先断言后抽取**：新增 `tests/integration/sync_path_equivalence_tests.rs` 3 条等价断言（完整序列一致 / 裁剪后保留最新 N 条 / 两条路径都置 `limited`）。断言**首次运行即红**，抓到 `get_room_events_batch_inner` 的 `ORDER BY stream_ordering ASC` + `rn <= limit` 取**最旧** N 条 ⇒ 初始同步让长房间的客户端永远看不到最新消息（实测 5 条消息、上限 2：`/sync` 给最旧两条、滑动同步给最新两条）；只对**无 `since`** 的初始同步改 DESC，增量保持 ASC 按批追赶。timeline 层抽成 `sync_helpers::timeline_window`（含 `TimelineOrder`），两条路径共用。判据：修复后 3 passed；`sync_service_tests_migrated` + `sliding_sync_service_tests_migrated` + `sync_handlers_coverage_tests` 全绿；services lib 的 timeline 相关 32 条单测全绿。**state 组装层未共享**，另登记 NEW-11 |
| DOC-04 | 三份文档的 `path:line` 引用漂移（原记 9 组） | **实测 145 处**，且**层级不同**：① 正文 13 处已全部改成**符号名**（`API_COVERAGE_REPORT.md` §8 的 `grep … # 已实现：…` 块、`MSC_SEMANTICS.md` 两处、`ROUTE_CONTRACT.md` 一处），正文残留 **0**；② 表格 132 处**必须保留** `path:line` —— 那是 `check_capability_ledger.py` 的 C2 判据（`已实现` 行必须带可解析 `path:line`），改成符号名会让能力台账门禁立刻红，所以"整篇换符号名"这条修法在本仓**只对正文成立**；③ 表格侧改用漂移信号门禁 `scripts/ci/check_doc_line_refs.py`：解析不到文件 / 越界 / 指向空行·纯注释·孤立 `}` ⇒ 计入 `scripts/ci/doc_line_ref_baseline`（现 **56**，只许下降）。**判据/变异**：在大括号闭合或注释行上新挂一处 `path:line` 引用 ⇒ 56 → 57 ⇒ 红；删掉被引用的行让旧引用越界 ⇒ 同款红。**未做**：56 处"指到空行/括号"没有逐条重定位（多数是表格里"某 handler 的第 N 行"这类证据指针，重定位要先确认该行为仍未变） |
| NEW-04 | 活文档里 30 处悬挂 `scripts/...` 引用，且不能做"出现即要求存在"的朴素门禁 | 新建 `scripts/ci/check_script_refs.py`：只查**活文件**（跳过 `docs/archive/**`、`docs/audit/archive/**`、`.workbuddy/**` —— 那里的"当年调用过 X，已删除"是历史叙述），解析顺序为仓库根 → `docker/` → `docker/deploy/`（部署文档的默认 WORKDIR），另设 `script_ref_exceptions` 只收「引用本身不是调用」的形态。**现算：1669 处引用 / 186 处解析不到**（原记 30 只数了一种后缀），按引用归类后的前 5 类：`coverage_baseline.js` 28（真身是 `.json`，扩展名漂移）、`db_migrate.sh` 26（真身 `docker/db_migrate.sh`）、`api_test/ledger.js` 20（同名 `.mjs`）、`deploy-monitoring.sh` 7 与 `contract-sync.mjs` 7（真身在 `docker/deploy/scripts/`）。186 已用 `scripts/ci/script_ref_baseline` 钉住（只许下降），接入 `preflight.sh`。**判据/变异**：把任意引用改成不存在的名字 ⇒ 计数涨过基线即红 |
| TEST-05 | 测试域 `sleep` 站点 81 处，无分类、无判据 | **改判据后收口**：逐处复核发现没有一处是"随便睡一下"——全部是故障注入（故意睡超请求预算证超时）、TTL/锁/会话过期、时间戳铺开、等后台任务就绪、契约/指标窗口。于是把判据从"计数下降"改成"**每一处都有类别、新增必须报类别**"：`tests/**` 35 处 + `synapse-*/src/**` 46 处全部加 `// sleep-reason:`；`check_sleep_ratchet.py` 新增"测试域每处 sleep 必须有注解"判据，基线键 `TEST_FILE_SRC_UNANNOTATED` 12 → **0**。**变异**：插一处裸 sleep ⇒ `FAIL 测试域有 1 处 … 没有 // sleep-reason: 注解`；删既有注解 ⇒ 同款红（均实测）。**未降**：计数仍 26/34/47=107；可省的只有 4 个轮询循环的间隔 → 已登记 NEW-10 |
| NEW-05 | 「自证测试」面：断言自己刚构造的字面量，文件名却声称覆盖 API/路由 | **逐文件判定完成（29 个）**：判定口径定稿为「①接真 ⇒ 可继续叫 `*_route_tests`；②只锁形状 ⇒ 文件名带 `_shape_only_tests` + 文件头声明 + 登记真覆盖文件或 `NONE`」。**16 个**来自原启发式候选、**13 个**是新门禁 C3 抓出来的同模板漏网文件（`admin_extra`/`app_service`/`background_update`/`event_report`/`federation`/`friend`/`module`/`push`/`room_summary`/`space`/`telemetry`/`thread`/`worker` —— 它们的 `.get(..).is_some()` 形态不足 3 处，旧口径数不到），全部改名 + 声明 + 登记；**15 个**仍在册的 `*_{api,route}_tests.rs` 都真碰生产代码（门禁 C3 逐个核过）。新门禁 `check_shape_only_tests.py`：C1 登记+声明 / C2 登记的"真覆盖"文件必须真的发请求或碰 DB / C3 名字承诺必须兑现 / C4 过期条目。**判据**：改名+登记后 `OK —— 29 个只锁形状（5 个有真覆盖 / 24 个已登记缺口）`；变异实测：删掉 `captcha_shape_only_tests.rs` 的声明 ⇒ C1 红；把任一 `*_shape_only_tests.rs` 改回 `*_api_tests.rs` ⇒ C3 红。**未做**：24 个 `NONE` 是真覆盖缺口，另登记为 NEW-09 |
| TEST-02 | 断言强度不足（原记 6 个文件只断言状态码） | 3 个文件补了 `errcode` + 字段断言（403/401/404）、2 个判定不适用（安全头 / 路由台账），本轮把**最后一个盲区** `msc4108` 收口：unit 文件删掉 12 个自证用例（`let status = StatusCode::OK; assert_eq!(status, StatusCode::OK)` 形态），handler 的真实往返新增 `tests/integration/msc4108_rendezvous_route_tests.rs` —— **12 条真 router + 真库**用例，覆盖 200+ETag/Expires/Last-Modified/Cache-Control/Pragma、304（If-None-Match 命中，无 body）、400（缺/错 Content-Type，`M_MISSING_PARAM` / `M_INVALID_PARAM`）、413（`M_TOO_LARGE`）、404（GET/DELETE 未知会话，`M_NOT_FOUND`）、412（If-Match 过期，`org.matrix.msc4108.errcode = M_CONCURRENT_WRITE`）、202+新 ETag+落库、204 后 GET 变 404。**实测**：`--test integration --features test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,widgets -E 'test(msc4108_rendezvous_route_tests) \/ …'` = **47 passed**（其中 msc4108 12 条）。**变异**：把期望 errcode 改成 `M_UNKNOWN` ⇒ `left: "M_MISSING_PARAM" / right: "M_UNKNOWN"` 红（实测，已还原） |
| TEST-06 | 5 个**整文件** feature 门控的测试文件在默认车道不参与 | **先红后绿**：新建 `scripts/ci/check_test_feature_gates.py` + 登记表 `scripts/ci/feature_gated_test_matrix`（C1 扫描 `git ls-files tests` 的 `#![cfg(feature = "…")]`、C2 要求 ci.yml 的一条 `nextest run --test <target>` **显式启用**该 feature（用 `--all-features` 命中的行会被单列为 `ALL_FEATURES`，不允许混成一个绿）、C3 反向查过期登记）。**首次运行 = 红**：3 个 integration 文件「无车道」、2 个 unit 文件「仅 ALL_FEATURES」。改法：默认 unit 车道的 `--features test-utils` → `test-utils,burn-after-read,server-notifications`（`ci.yml` 与 `scripts/ci_backend_validation.sh` 同步），并为 3 个 integration 文件新增一条显式 feature 车道（`require_tests_ran.sh` 包裹，命中 0 个用例即失败）。**实测**：`cargo nextest run --test unit --features test-utils,burn-after-read,server-notifications -E 'test(burn_after_read_route_tests) \/ test(server_notification_service_tests)'` = **86 passed**；integration 那条 = `e2ee_audit_service_tests` + `space_children_service_tests` + `api_widget_tests` 全绿。**踩到的坑**：integration target 的 `required-features` 是 6 个（`test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications`），只写 `test-utils,widgets` 会被 cargo 直接拒（exit 101）—— 这条只有真跑才会发现 |
| NEW-03 | 依赖分裂组数与处置进度整表过期（38 → 32 组） | `docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md` 按 `cargo tree -d --workspace --locked` **现算重写**：统计口径写进 §0（列 0 行 → 按 crate 名连续聚合 → 47 名 → **32 分裂组 + 15 同版本多来源**，附「不加 `--workspace` 会得 80 名/31 组」的反例）。§2 从「10 族 / 38 组」改为「**11 族 / 32 组**（16+4+3+2+1×7）」，并把「哪 6 组消失了」回答掉：`aead`/`aes`/`chacha20`/`chacha20poly1305`/`cipher`/`inout`/`poly1305`/`universal-hash` 8 组 + `socket2` + `typenum` 已收敛（进 §1 已消除表），新增 `deadpool`/`deadpool-runtime`/`num-bigint` 归入 §3.4。§3.1 的逐组引入者 14 行全部用 `cargo tree -i <crate>@<ver> --depth 1 --workspace --locked` 重跑，**推翻了旧结论**「本项目直连旧一代 RustCrypto」——`sha2 0.11` / `hkdf 0.13` / `hmac 0.13` 的直连侧已在新一代，唯一还能本地收窄的是 `ed25519-dalek 2 → 3`。§4「需上游升级 32 组」→ 31 组 |
| SEC-14 | 联邦请求缺 `ts` 仅告警不拒绝 | **裁决：不改（原建议是错的）**。规范「Authentication」一节的授权参数只有 `origin` / `destination` / `key` / `sig`，随后写明「Unknown parameters are ignored」——`ts` **不在规范里**；而本仓**自己的出站请求就不带 `ts`**（`synapse-federation/src/client.rs` 拼的 X-Matrix 头逐字等于上述四项）。按原建议「缺 ts ⇒ 401」会让服务器拒掉自己发出的每一个联邦请求。改为把语义显式化 + 钉住：抽出 `evaluate_signature_timestamp` / `TimestampCheck`（Absent / Disabled / Fresh / Stale），四条分支各一条用例；另加 `doc_credibility_guard_tests::signature_timestamp_claim_matches_the_outbound_header` 要求「文档说我们不发 ts」与「代码确实不发」双向一致。**变异**：把 `None` 判成 `Stale` ⇒ `missing_signature_timestamp_is_accepted` 红；给出站头加 `ts=0` ⇒ 一致性守卫红（都实测，还原字节一致） |
| GATE-01 | **pre-commit 默认不跑 clippy，而结论行写「全部通过（跑的是 CI 同名门禁）」** —— 该措辞让「没跑 clippy」与「clippy 绿」读起来一样 | 本轮两次实证：`media_thumbnail` 加到第 8 个参数（`too_many_arguments`）、删掉 `notify_device_revocation` 后 `json`/`BackgroundJob` 变成未用导入 —— 两次都是 `-D warnings` 会红，而 `git commit` 输出只有一片 `OK` + 「全部通过」，直到手工跑 CI 那条 clippy 才发现（`.githooks/pre-commit:30` 的提示在**开头**，被 11 分钟的 preflight 输出淹掉）。修法：`preflight.sh` 的结论行按是否 `--clippy` 分两种措辞（没跑就明说「⚠️ 未跑 clippy」），并在 hook 的**成功路径末尾**再提示一次 |
| NEW-06 | **（误报，已撤回）** worker 不实现 `notify_device_revocation` 的联邦发送 ⇒ 设备吊销通知静默丢失 | **判定错误，记录为什么错**：我把 `device_sync::notify_device_revocation` 当成了"生产者"，实际它**零调用者**（`git grep` 全仓只有定义本身），而 `DeviceSyncManager` 在生产代码里**从未被构造**（唯一构造点在 `tests/integration/federation_service_tests_migrated.rs`）。真实的设备列表 EDU 广播在 `routes/device.rs::broadcast_device_list_update`（6 个调用点，走签名事务 `EventBroadcaster::broadcast_edu` ⇒ `FederationClient::send_transaction`）。**动作**：删掉那条死路径（52 行）+ `BackgroundJob::Generic` 变体 + worker 的 `Generic` 分支；守卫 `federation_device_list_writer_tests::the_dead_device_revocation_job_does_not_come_back`。**教训**："某函数会入队" ≠ "有人调它" —— 判 P1 之前先 grep **调用者**，而不只是函数体 |
| NEW-07 | 联邦入站 `m.device_list_update` / `m.profile_update` 写的东西**对客户端永久不可见** | `record_device_list_change`（存储层分配本地序号，两表同写）取代 `insert_device_list_change(.., 远端 stream_id)`。三条叠加缺陷：① 只写 `device_lists_changes`，而 `/sync` 与 `/keys/changes` 都读 `device_lists_stream`；② 写入的是对端毫秒时间戳（1e12），而读取都带 `stream_id <= to`（本地 1e3）⇒ 连唯一读 changes 的路径也读不到；③ 出站 EDU **不带 `deleted`**，接收端把"删设备"记成"更新"。**判据**：`device_list_federation_visibility_tests`（3 条，真库）+ `routes::device` 的 3 条"发→收"往返用例；变异（退回 changes-only + 时间戳）实测 2/3 红。**未覆盖**：没有走 `EduDispatcher` 的端到端用例（`FederationContext` 有 30+ 字段），handler 是否真调这个方法由 `federation_device_list_writer_tests` 的源码断言钉住 |
| ARCH-01 | Thread 双实现写影子表，数据对 `/sync` 不可见 | `3789583ef`（写真实 `m.thread` 事件，影子表降级为索引） |
| FUNC-01 | `erase=true` 显式 501（GDPR 擦除未实现） | **决策：不实现擦除，只登记**。`API_COVERAGE_REPORT.md` 新增 §6.5「已知的语义限制」写明「支持注销 / 不支持 erase」及理由；501 的 message 改为指向该条目（客户端作者不用猜是不是服务端坏了）。**未做**：没有端到端用例（需要 UIA 全流程的 app 夹具），`tests/unit/account_compat_route_tests.rs` 里现有的 deactivate 用例只是「本地构造 JSON 再断言」的自证，不算覆盖 |
| ARCH-06 | `updated_ts` 约束形态两套并存（规则说可空、SQL 一半 NOT NULL） | **决策：不统一，改规则 + 棘轮冻结**。规则 §2.2 改为「两种都合法，新表必须显式选一个」，示例 SQL 改用 `NOT NULL`；`scripts/ci/check_schema_timestamp_convention.py` 把计数冻在 NOT NULL 43 / 可空 42（`scripts/ci/schema_timestamp_baseline`），只许朝 NOT NULL 方向变。**判据**：新增一个可空 `updated_ts`（含单行建表）⇒ 红；把一处 `NOT NULL` 放宽 ⇒ 红（两处变异实测）。未做统一迁移：`SET NOT NULL` 在有 NULL 行时失败，本仓无生产数据可判 |
| DOC-03 | 项目规则把 MSC3245 标成 "Room summary"（实为语音消息）且标"完整实现" | 决策先做 NEW-02（见下一行），再改那一行为「语音消息 / 按 by-design 部分支持」；守卫 `doc_credibility_guard_tests::project_rules_labels_msc3245_as_voice_messages` 只看名称列与状态列（证据列允许引用旧错标），变异实测红 |
| NEW-02 | 项目规则文档不在版本控制里（`.trae/` 被 gitignore） | **决策：`git add -f .trae/rules/project_rules.md` 单文件入库**（不做迁移：`.trae/rules/` 正是 IDE 读取规则的路径，搬走会让 IDE 读不到）。`.gitignore` 就地写明这是唯一例外与 `-f` 的用法；入库后 `workspace_rules_reference_paths_exist` 才能在 CI 里跑 |
| CODE-01 | 生产 `unreachable!` 无门禁 | `629110931`（4 → 1 + clippy 三处副本 + 守卫）；余项见 CODE-01b |
| CODE-02 | 19 处静默吞掉缓存失效 | `8b8505ff5`（cache 墓碑 + 2 个 helper + CI 扫描） |
| CODE-03 | 设备删除路径吞掉清理失败 | `076309e7c`（四个入口改单事务，8 处 warn 归零；判据 = 触发器注入的原子性用例） |
| CODE-04 | 邀请认领静默丢弃房间配置 | `3cf5c006a`（坏 blob → 500 + 释放认领） |
| CODE-05 | `FakeUserStore` 泄漏到生产 API | `85180061c`（`cfg` 双门禁 + `unimplemented!` → `Err`） |
| CODE-07 | 客户端格式错误被映射成 500 | `3cf5c006a`（三态 + `InvalidInput`） |
| CODE-09 | 常量注释与取值不符（1 year vs 5 分钟） | `01345e38f` |
| CODE-12 | URL 预览里的死表达式 | W6 收尾（连同未读字段一起删） |
| FUNC-02 | voice 三端点"恒不可用" | 并入 RD-19 登记为 by-design（MSC3245） |
| FUNC-03 | 分片上传绕过配额记账 | `cc364bd6f`（`upload_media_with_id_accounted`） |
| FUNC-04 / DOC-01 | 能力台账两行与实现相反 | `01345e38f`（+ 守卫 `capability_ledger_absence_claims_match_the_code`） |
| FUNC-06 | 分片上传 `total_size` 由客户端声明 | `cc364bd6f`（必填 + 预占 + 413 双层） |
| PERF-02 | 注册流程 5-6 次独立提交 | `cc364bd6f`（`create_registration` 单一事务边界） |
| PERF-03/04/05 | 索引列不匹配 / `LIKE` 不命中 GIN / 前导通配 | `2358c0f5a`（表达式索引 + `POSITION` + `EXPLAIN` 断言） |
| PERF-06 | 10 个 prune 无分批 | `3cf5c006a`（`ctid` 分批 + `PRUNE_BATCH_SIZE`） |
| PERF-07/08/09 | 无 `LIMIT` / 深分页 / 邀请黑名单竞态 | `0d18fede0`（窗口下推 + 决胜键 + `lock_room_row`） |
| PERF-13 | URL 预览首跳解析两次 | `3cf5c006a`（`VerifiedFirstHop` 透传） |
| PERF-14 | 分片上传先拼完再校验大小 | `3cf5c006a`（`checked_chunk_total` 预检） |
| SEC-01 | RBAC 关闭时超管黑名单失效 | `85180061c`（顺序＝超管→黑名单→开关→角色） |
| SEC-02…SEC-08 | 默认口令 / MFA 只校验非空 / `/metrics` 匿名 / XFF 伪造 / 重放窗口 / 监控口令 | `ca6980c51`（7 条逐条见归档 §10.7） |
| SEC-10 | URL 预览把用户 URL 写日志 | `369706979`（只留 `scheme://host` + ≤64 字符 path） |
| TEST-01 | 联邦签名测试覆盖薄 | `85180061c`（4 条 `test01_*`） |
| TEST-03 | 7 条 e2e `#[ignore]` 无入口 | `fc7a6db22`（`ci_backend_validation.sh e2e` + nightly workflow；隔离栈 7/7） |
| TEST-04 | CI 脚本从未被调用 | `01345e38f`（元门禁 `check_gate_reachability.py`；47 个 `check_*` 全部可达） |
| TEST-07 / DOC-02 / DOC-09 | 测试规模口径三处不一致 | `check_doc_metrics.py`（脚本现算 + 变异自证） |
| DOC-05 / MAINT-05 | 注释引用不存在的脚本路径 | `01345e38f`（8 处 / 7 文件） |
| DOC-06 | `UNRESOLVED_ISSUES_SUMMARY.md` 滞后 | 头部加 `STATUS: HISTORICAL` |
| DOC-07 | `INDEXES.md` 计数与 SQL 不一致 | 计数进 `doc-metrics` 块（整份重生成见 PERF-12 决定） |
| DOC-08 | `migrations/README.md` 描述的两个缺陷 | **前提不成立**：两处早已改成按对象名判定 |
| MAINT-06 | `LivekitConfig` 三字段零业务读取 | 随 RD-11 整体删除 |
| MAINT-08 | `#[allow]` 无理由比例高 | 并入 CODE-11 |
| NEW-01 | 联邦缩略图不传 `animated` ⇒ JPEG 字节标成 `image/webp` | 本提交 `fix(media)`（`media_thumbnail_path` 透传 `animated=true`；用例两个方向都断言：传 false 不得携带该参数）。**判据**：删掉拼接 ⇒ `media_thumbnail_path_forwards_the_animated_flag` 红（实测） |
| RD-01…RD-26 | 无用依赖 / 空转门面 / 死字段 / 预留路由 / 零引用表 | W6 四批 + RD-14 接入（逐条见归档 §10.8） |
| CODE-01b | `hash_token` 缺 `TOKEN_HASH_SECRET` 时 panic；8 个 `src/bin` 入口完全不校验（空串还会 fail-open） | `synapse-common/src/crypto.rs` 抽出纯函数 `classify_token_hash_secret(secret, release_build)`（`Configured` / `DevFallbackInDebug` / `MissingInDebug` / `Rejected`）+ `ensure_token_hash_secret_or_exit(component)`（stderr 指明缺失项 + `exit(3)`，`TOKEN_HASH_SECRET_EXIT_CODE`）。**空串在任何构建模式都 `Rejected`**（原 fail-open 的根因是 `env::var` 对空串返回 `Ok("")`）。8 个 bin 的 `main` 首行调用 guard，`synapse_worker` 例外放在离线 `--validate-topology` 分支之后（该模式不哈希 token）。**判据**：5 条用例（空串两模式都拒 / release 缺失致命而 debug 仅 warn / dev fallback 仅 release 致命 / 32 字节两模式通过 / 8 入口守卫）；守卫测试**先剔除注释**再找 `ensure_token_hash_secret_or_exit(`（初版只做子串匹配，被一条注释骗过 ⇒ 已收紧），并断言入口集恰为 8 个文件名。**变异**：把 `src/bin/healthcheck.rs` 的 guard 注释掉 ⇒ 红（`src/bin/healthcheck.rs 没有调用 ensure_token_hash_secret_or_exit(…`，实测） |
| CODE-08 | 已注册端点恒 403，`#[allow]` 无 `reason`，读代码分不清"预留"还是"忘了删" | **裁决：保留端点**（审计 §9.7 的修法就是"补 reason 指向契约条目"；删除会与 RD-19 / FUNC-05 已登记的"设计决策"冲突）。`devices.rs` 的 `allow(clippy::unused_async)` 改为带 `reason`，doc 指向 `ROUTE_CONTRACT.md` 的 by-design 表与 `check_unsupported_endpoints.py`；`clippy_allow_baseline` 该行 1 → 0（棘轮收紧）。**判据**：真 router 的 403 + `M_FORBIDDEN` 用例三条（`tests/integration/api_placeholder_contract_p0_tests.rs:310-326` 成员 / `:343-358` 非成员 / `:394` 管理员）。**变异**：让 handler 返回 200 ⇒ `check_unsupported_endpoints.py` 红（`unsupported=true` declared but the handler is no longer a constant failure，实测） |
| ARCH-03 | "未实现"两套错误语义（404 `M_UNRECOGNIZED` vs 501 `M_UNKNOWN`）对外无统一契约 | `ROUTE_CONTRACT.md` 新增「附录 F — 未实现语义表」（刻意放在 `## 附录` 之后：`gen_contract_doc.py` 只重建附录之前的路由派生部分，手写表因此不会被重新生成抹掉）。表列 3 行语义 + 判定顺序 + "wire 值与内部值"说明（`MatrixErrorCode::Unrecognized` 与 `Unimplemented` **共用** `M_UNRECOGNIZED`，内部靠 `ApiError::is_not_implemented()` 区分）—— 这正是"混用"观感的根因。**调用点按实测校正**：审计写的"12 处"实为 **10 处生产 + 2 处 `#[cfg(test)]` pin**（`voice` ×3、`account_compat` ×1、`room/events.rs` ×1、`captcha_service` ×3、`sms_provider` ×1、`credential_auth` ×1），逐行给改前/改后（**全部保持**）；captcha / sms 4 处按 #1 回 404 与 Synapse 的 `400 M_THREEPID_DENIED` 偏离显式登记，留 MAINT-04 统一处理。`voice_operation_not_provided` / `reject_unsupported_account_erasure` 抽成函数以便钉住语义（`voice.rs` 里那句 "`M_UNRECOGNIZED` (501)" 的注释同步订正为 404）。**判据**：`voice_convert_optimize_transcribe_are_404_m_unrecognized`（404 + `M_UNRECOGNIZED` + MSC3245 文案）、`erase_true_pins_501_m_unknown`（501 + `M_UNKNOWN`）、`arch03_unimplemented_semantics_table_is_pinned`（两者 wire/status 互不等，`internal` 仍 500）、`voice_internal_errors_stay_500_not_404`（反向用例）。**变异**：`not_implemented` → `not_supported` ⇒ 红（`left: "M_UNKNOWN" / right: "M_UNRECOGNIZED"`，实测） |
| RD-27 | `to_device` 的 4 个存储方法零调用，其中一个名字与语义相反 | **审计前提部分不成立，只删真正死的 2 个**。`git grep` 实测：`get_messages` / `delete_messages` 生产与测试均 0 调用 ⇒ 删除（各带一条静态 `query!`）；`delete_messages_up_to` **有生产调用**（`synapse-services/src/sync_service/mod.rs:209`）⇒ 保留；`get_and_delete_messages` 名字不反（实现确实先读后删），但被 `get_messages_for_sync` → `synapse-e2ee/src/to_device/service.rs:120` 与 `tests/integration/to_device_sync_tests_migrated.rs:271` 消费 ⇒ 保留。**"名字相反"的真问题在 `get_messages_for_sync` / sync 路径，不在存储层**。"无外部消费者"的判定依据：`synapse-e2ee` 全仓 re-export 只有 `ToDeviceStorage`（被删方法不是 trait 成员、不在 `lib.rs` 的 `pub use` 里），且删除后 `cargo check --bins` 与 `synapse-e2ee` 测试目标均通过。`check_sqlx_dynamic_ratio.sh`：静态 1363 → 1357（−2 属本卡、−4 属基线滞后），test 786 → 791（基线滞后，非本卡），两条都在 `scripts/ci/sqlx_dynamic_ratio_baseline` 就地写了原因 |
| MAINT-04 | 能力台账手工维护，已出现多处失真 | 新建 `scripts/contract/check_capability_ledger.py`：C1 每行恰一个判定词（`已实现` / `设计决策` / `未实现`）、C2 `已实现` 必须带可解析 `path:line`（越界即错）、C3 `设计决策` 必须引用审计文档里真实存在的编号、C4 "不注册"声明的路径不得同时在 `ledger_annotations.txt` 里是 `unsupported=true`（两条台账互相矛盾即红）。接入 `check_route_contract.sh`（含 `--mutation-check` 自证）。`API_COVERAGE_REPORT.md` §6.2 四行 + §6.5 两行补规范判定词与可解析引用。**判据**：脚本绿（15 行 / 1 条不注册声明）；`--mutation-check` 三类缺陷各红。**变异**：把 §6.2 MSC3814 的引用改成 `:99999` ⇒ 红（`行号越界（… 只有 321 行）`，实测） |
| FUNC-05 | 联邦能力项有意不注册，未全部登记为设计决策 | `API_COVERAGE_REPORT.md` 新增 §6.6「设计决策台账」，5 行覆盖：`/_synapse/admin/v1/backups`（不注册）、thirdparty 4 条（RD-15）、联邦 legacy key 5 条（FUNC-05；RD 族详表见审计归档）、voice 3 条（DOC-03）、keys/distribution（RD-19）。判定词闭集由 C1 强制：残留"未登记"即红，三态在 §6.6 内互斥 |
| NEW-08 | `synapse-federation/src/device_sync.rs`（735 行）自称"设备列表同步"，实际生产零调用，与 `routes/device.rs::broadcast_device_list_update` 重复实现同一件事 | **删除整模块**。`DeviceSyncManager::new` 全仓唯一构造点在它自己的 `#[cfg(test)]` 与 `tests/integration/federation_service_tests_migrated.rs`（2 条只测死代码的用例，已删）；同步移除 `synapse-federation/src/lib.rs`、`synapse-services/src/lib.rs`、`synapse-web/src/federation/mod.rs` 三处 re-export。棘轮同步：`scripts/ci/sqlx_dynamic_ratio_baseline` static 1357 → 1354（该模块 3 条生产静态查询）、test 动态 791 → 784、总 926 → 925，均就地写明"只因查询确实被删"；`file_size_baseline` / `clippy_allow_baseline` 删除过期文件条目；`TESTING.md` 与 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` 数字块跟改。守卫 `federation_device_list_writer_tests::the_dead_device_revocation_job_does_not_come_back` 改为"文件缺失即跳过"——重加回来仍会被扫描。**判据**：`cargo check --workspace --all-targets --all-features` 通过；`check_sqlx_dynamic_ratio.sh`、`check_file_size_ratchet.py`（该文件无 stale 条目）、`check_clippy_allow_ratchet.py`、`check_sqlx_cache_fresh.sh --static` 全绿。**变异**：把 `pub fn notify_device_revocation() {}` 写回同名文件 ⇒ 守卫红（实测）。真实广播路径 `broadcast_device_list_update` 未动 |
