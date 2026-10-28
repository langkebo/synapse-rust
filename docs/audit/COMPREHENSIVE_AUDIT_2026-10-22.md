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
| 架构设计 | 0 | 0 | 1 | 0 | 1 |
| 代码质量 | 0 | 0 | 1 | 0 | 1 |
| 功能完整性 | 0 | 0 | 0 | 0 | 0 |
| 性能（含数据层） | 0 | 0 | 0 | 2 | 2 |
| 安全性 | 0 | 0 | 0 | 0 | 0 |
| 可维护性 | 0 | 0 | 0 | 0 | 0 |
| 测试覆盖 | 0 | 0 | 0 | 0 | 0 |
| 文档完整性 | 0 | 0 | 0 | 0 | 0 |
| 专项：冗余与过度开发 | 0 | 0 | 0 | 0 | 0 |
| **合计** | **0** | **0** | **2** | **2** | **4** |

> 计数口径（2026-10-27 逐条重算，**按表格行**计，合并行只算一行）：**P0 = 0，P1 = 0，P2 = 2，P3 = 2，合计 4**。
> 来源 = §0.2.2 的 1 条 + §0.2.3 的 3 条（§0.1 已于 2026-10-26 清空）。
> ⚠️ **本版更正一处旧账**：上一版写 `P3 = 13`，但把 §0.1 / §0.2.2 / §0.2.3 三处表格按行数出来是
> **14 行**（正则 `^\| \*\*ID\*\* \| P[0-9] \|` 扫一遍即可复核）—— 13 是漏计，不是有 1 条被关掉。
> 本版关闭 **NEW-19**（P2/功能完整性，`2e33c45a8`）、**NEW-10**（P3/测试覆盖，`d9fa05d26`）、
> **NEW-09**（P3/测试覆盖，`615b500d4`）与 **NEW-11**（P3，`050aefcb4` + 本笔）
> ⇒ P2 `6 → 5`、P3 `14 → 11`、合计 `20 → 16`。
> 本轮（2026-10-26）裁决并落地 **NEW-15**（删除未知任务统一 404）与 **NEW-16**（不限额时
> `remaining: null`）⇒ P3 `11 → 9`、合计 `16 → 14`；§0.1 自此清空，判据见附录 D。
> 同轮继续收口三条 P3 安全项：**SEC-09** 成立（CAS `redirect_after` 开放重定向，已修 +
> 11 组输入单测）、**SEC-11** 判为**误报**（`[0u8; 32]` 是 HKDF 输出缓冲，必被 `hk.expand`
> 覆盖，不是全零密钥）、**SEC-13** 成立但原描述有偏差（泄露的是**被签名的请求字节**，不是
> 签名串；已改成 `len` + 8 字符 SHA-256 指纹 + 源码棘轮）⇒ P3 `9 → 6`、合计 `14 → 11`。
> 同轮再收口 **SEC-15**（鉴权覆盖率从静态快照变成可重跑门禁：979 条路由 / 818 带提取器 /
> 161 条逐条登记，需凭证 92.2%、真正无凭证 76 条 = 7.8%，由
> `scripts/contract/check_route_auth.py` + `route_auth_public.txt` 双向守住）⇒ P3 `6 → 5`、合计 `11 → 10`。
> 同轮再收口 **PERF-10**（`postgres.conf` 补实例级 `statement_timeout=5min` /
> `idle_in_transaction_session_timeout=60s`，并由 `check_connection_budget.py` 断言
> 「显式存在 + 不比应用默认更严」）⇒ P3 `5 → 4`、合计 `10 → 9`。
> 同轮再收口 **SEC-12**（legacy r0/v1 匿名读**是裁决不是漏检**：`docker/deploy/README.md`
> 登记六条路径的实际鉴权口径，由 `doc_credibility_guard_tests` 把「文档声明 / r1 必须收紧 /
> r0·v1 必须仍匿名 / handler 仍 token 可选」四方钉在一起）⇒ P3 `4 → 3`、合计 `9 → 8`。
> 同轮再收口 **MAINT-01**（`synapse-common` 四个测试基建模块从"无条件编译"改为
> `#[cfg(any(test, feature = "test-utils"))]`，并由 7 份 manifest 在**测试侧**打开该特性；
> 守卫同时断言生产段**不得**打开）⇒ P2 `5 → 4`、合计 `8 → 7`。
> 同轮再收口 **PERF-01**（`to_device_messages` 对普通设备无按龄清理：新增
> `pruning::prune_old_to_device_messages` + `retention.to_device_retention_days`（默认 30，`0` 关闭）
> 并接进 prune 循环；语义裁决「`claim_messages` 取走即删 ⇒ 存活超窗 = 设备从未回来取」，
> 代价是离线超窗设备丢积压，与 `DEVICE_LIST_STREAM_RETENTION_DAYS` 同等取舍）
> ⇒ P2 `4 → 3`、合计 `7 → 6`。
> 同轮再收口 **ARCH-05**（7 个 >2000 行文件里，**绝大部分体积是内联测试模块**：如 `error.rs` 生产段
> 只到 1229 行、测试块 1251 行。按仓库已有约定（`event/db_tests.rs` / `thread/db_tests.rs` 那种
> 独立测试文件）把 9 个内联测试模块外移，7 个文件的**生产段**全部 ≤1557 行；
> `scripts/ci/file_size_baseline` 的对应 7 条**逐条收紧**而不是整表 `--update`）
> ⇒ 架构设计 P2 `2 → 1`、合计 `6 → 5`。
> ⚠️ 同一轮**新登记** **NEW-20**（P2）：`check_file_size_ratchet.py` **在 HEAD 上就是红的**
> —— 67 个文件涨过各自上限、另有 2 个新增 >600 行文件未入册（`--update` 会一次性批准全部
> 无关增长，故本笔只收紧自拆的 7 条，闸门仍红）⇒ P2 `2 → 3`、合计 `5 → 6`。
> **净结果：合计 6 → 6，但构成变了** —— ARCH-05 关闭、NEW-20 打开。
> 同轮再收口 **CODE-11**（`#[allow(...)]` 的 157 处无理由 / 94 文件全部补上 `// reason:`，棘轮基线
> 由 `157/94` 收紧到 **0/0**；同时修掉棘轮自身「空基线 = 失败」的自相矛盾 —— 零无理由正是它的目标态，
> 只有**缺文件**才 fail-closed）⇒ P3 `3 → 2`、合计 `6 → 5`。判据与变异见附录 D。
> 2026-10-27（W13）收口 **NEW-20**（文件行数棘轮在 HEAD 上红）：处理方式由**人拍板**为
> **整表 `--update` 一次性批准**（不逐条回溯、不拆文件）—— 67 条已入册增长与 2 个新增
> >600 行文件（`synapse-services/src/url_preview.rs` 607、`synapse-services/src/media/tests.rs` 646）
> **全部入册**，基线 220 → **222** 条；重设时把 19 条已缩小的 ceiling 一并收紧（−261 行），
> 不是单向放宽 ⇒ P2 `3 → 2`、合计 `5 → 4`。判据与变异见附录 D。
> 归因数据仍记在附录 D 备查（67 条增长 +4201 行、落在 33 个已提交批次，零条无人认领 ——
> 这解释了"为什么这次批准不是盲批"）；此前试过的一版「归因 + 拆分」(`848a736ed`) 按拍板撤回。
> ✅ NEW-19：`/send` 的线程回复现在同时进 `thread_relations` 与 `thread_replies`
> （时间戳回查刚落库的事件，不用本地时钟；重试同一 txn 不重复计数）。
> ✅ NEW-10：4 处手写轮询折成 `tests/common/mod.rs::wait_until`，测试域 `sleep` 计数
> `47 → 44`（`scripts/ci/sleep_baseline` 已随之收紧），超时文案带「等什么 + 等多久」。
> ✅ NEW-09：`captcha` 补齐**完整状态机**（发码→查状态→错码→对码→复用/超限），
> 关键是把「要预置模板」从注释里的借口变成测试里的 `seed_image_template`。
> ✅ NEW-11：两条路径的 state 等价断言 **3/3 就位**（全量集合 / 过滤方言 / `state_after` /
> 懒加载成员集），并顺手修掉 ARCH-02 三条从 NEW-17 起就红着的用例。
> 判据均见附录 D。
> ✅ **NEW-14 已收口**：C3 审计面 15 → 95 个文件（零误伤），14 个 `NONE` 族已逐族重跑 ⇒
> 4 族误判（改登记）、3 族幽灵、7 族真缺口（见 §0.1.1）。
> `NEW-08` 已于 2026-10-25 关闭（`device_sync.rs` 整模块删除，判据与变异证据见附录 D），不再计入。
> 其中 **4 条是"登记项"**（明确不做或只等外部条件）；§0.1 已于 2026-10-26 清空
> （最后两条 `NEW-15` / `NEW-16` 已裁决并落地），没有"我们已经知道怎么改但还没动手"的条目了。

**结论摘要**：**P0 = 0，P1 = 0**（原 9 条 P1 全部有实现 + 能失败的门禁，逐条见附录 D）。
⚠️ 第八轮曾把一条误报成 P1（NEW-06「设备吊销通知静默丢失」），**已撤回**：真实广播路径在
`routes/device.rs::broadcast_device_list_update`（签名事务），死的是 `device_sync` 里那条
零调用者的重复实现，已删除；更正过程记在[附录 D](#附录-d已关闭条目索引)。
剩下的 4 条里，**没有一条会造成数据丢失或越权**；2 条 P2 分别是 `events` 未分区（ARCH-04）、
517 行静默忽略（CODE-10）
—— 全是**长期结构债**：
不修也不会立刻出错，但每加一个功能都在加利息。2 条 P3 **全部是 §0.2.3 的登记项**
（N+1 只剩 1 处、已有 bench 组但无测量指认它是热点 / 索引清单人工不可反推），没有待裁决项。

**下一批该做什么**：**没有"我们知道怎么改但还没动手"的条目了**。剩下的 4 条分两类 ——
① 3 条**登记项**（§0.2.3，共 4 个 ID —— `PERF-12` / `MAINT-07` 同属一行；明确不做或只等外部条件）；
② ARCH-04（真实数据量）**卡在外部条件上**，不是排期问题。

## 0.1 第七轮新发现（2026-10-24，只列仍开着的）

> 这是第七轮（2026-10-24）新发现里**仍开着**的部分。**2026-10-26 已清空** —— 最后两条
> `NEW-15`（删除未知任务统一 404）与 `NEW-16`（不限额时 `remaining: null`）已裁决并落地，
> 判据与证据见[附录 D](#附录-d已关闭条目索引)。其余已关闭条目（`NEW-04` / `NEW-05` / `NEW-09` /
> `NEW-10` / `NEW-11` / `NEW-19` 等）同样只在附录 D 保留索引，完整正文在归档文件里。
> 本节保留为空表，是为了让"§0.1 已清空"这件事本身可核对，而不是删掉标题让人找不到。

| 编号 | 级别 | 一句话 | 证据 |
|---|---|---|---|
| — | — | 无开放项（2026-10-26 清空） | 见附录 D 的 `NEW-15` / `NEW-16` |

### 0.1.1 NEW-14 重跑结果：14 个 `NONE` 族的逐族判定（2026-10-25）

判据不是"看文件名"，而是三步：**①** 从 shape-only 文件头解析它声称的路由模块；**②** 从该模块
（含子目录）抽 `"/…"` 端点字面量；**③** 在**含真实依赖标记**（`oneshot` / `Request::builder` /
`setup_fresh_test_app` / `create_router` / `sqlx::query`）的测试文件里找这些端点。只有第 ③ 步
命中才算"有覆盖"，否则是真缺口。

| 族 | 声称模块 | 实测证据 | 判定 |
|---|---|---|---|
| `admin` | `routes/admin/`（目录） | `api_admin_rbac_wire_tests.rs`（5 用例）真打 `/_synapse/admin/v1/users` | 误判，已改登记 |
| `app_service` | `routes/app_service.rs` | `api_appservice_p1_tests.rs`（15 用例）用 `oneshot` 打 `/_matrix/app/v1/ping` 等 | 误判，已改登记 |
| `federation_cache` | 无此文件（端点在 `routes/admin/`） | `api_admin_federation_tests.rs`（13 用例 / 80 处标记）打 `/_synapse/admin/v1/federation/cache*` | 误判，已改登记 |
| `worker` | `routes/worker.rs`（MSC4140） | `api_delayed_event_schedule_tests.rs`（23 处标记）打 `delayed_events` | 误判，已改登记 |
| `admin_extra` | `routes/admin_extra.rs` | **全仓无 `admin_extra` 生产引用**（`synapse-web` 里 0 命中） | **幽灵族**：镜像了一个不存在的模块 |
| `coverage` | 无（无端点字面量） | 只有 `API_COVERAGE_REPORT.md` 这类文档引用，不是 HTTP 面 | **幽灵族** |
| `formatting` | `routes/formatting.rs`（**0 个 `.route(`**） | 该文件是 DTO/格式化助手，不注册任何路由 | **幽灵族** |
| `background_update` | `routes/background_update.rs`（19 条路由） | 测试里 0 命中 | 真缺口 |
| `captcha` | `routes/captcha.rs`（5 条路由） | 只有 `captcha_tests_migrated.rs`（service 级，0 个 `oneshot`） | 真缺口（HTTP 层） |
| `media_quota` | `routes/media/quota.rs` | 端点 `/quota/check`、`/quota/stats`、`/delete/{server}/{media_id}`，测试 0 命中 | 真缺口 |
| `module` | `routes/module.rs`（23 条路由） | 测试里 0 命中 | 真缺口 |
| `refresh_token` | `/refresh`（`routes/assembly.rs`） | 只有 `api_auth_routes_tests.rs:95` 的挂载烟测（断言 ≠404/405），**无轮换语义** | 真缺口 |
| `registration_token` | `routes/admin/token.rs` | 只有 `route_ledger_*.snapshot` 台账快照 | 真缺口 |
| `sliding_sync` | `routes/sliding_sync.rs`（4 条路由） | 只有 service/storage 级用例，无 HTTP 往返 | 真缺口（HTTP 层） |

**这解释了为什么"缺口数"一直在动**：`24` 这个数字最初是"文件名判定"的产物，其中至少 6 个族要么
早就有真覆盖（只是命名不在 C3 视野内），要么根本没有生产对应物。现在守卫的 `已登记缺口 10`
里仍有 **3 个是幽灵族**（矩阵格式只允许 `NONE` 或文件名，无法区分"没测"与"不存在"）——
真实待补的是 **7 族**。

## 0.2 未完成任务清单

> 这份清单是**唯一权威**。一条算"已完成"当且仅当它出现在某次提交里并给出可复现证据；
> 不在本清单、也不在本轮提交里的编号，只可能出现在附录 D（已关闭索引）。

### 0.2.1 可先红后绿

> 2026-10-24 已清空：最后一条 `TEST-05` 已按「逐处分类 + 注解门禁」收口（见附录 D）。
> 剩下的 4 条全部落在 §0.2.2（需要决策/等价断言）与 §0.2.3（登记项）。

### 0.2.2 需要决策或外部条件

| 编号 | 级别 | 一句话 | 卡在哪 |
|---|---|---|---|
| **ARCH-04** | P2 | `events` 未分区、`room_memberships` 无归档 | 需要真实数据量判断分区收益；本机无生产级数据，属**等外部条件** |

### 0.2.3 登记项（已知、明确不做或只等时机）

| 编号 | 级别 | 一句话 | 为什么不做 |
|---|---|---|---|
| **CODE-10** | P2 | 生产 **517 行** `let _ =` / `unwrap_or_default()`（193 文件） | 大部分是合法用法；设下降配额会诱导写 `unwrap()` 来"减少计数"（§10.10） |
| **PERF-11** | P3 | N+1 查询**剩 1 处**（原 3 处中 2 处已消除，风险可控） | 该路径已有 bench 组覆盖，但无测量指认它是热点 |
| **PERF-12 / MAINT-07** | P3 | `INDEXES.md` 人工表无法从 DDL 反推 | 重生成会让"索引治理"退化成"索引清单"；计数已由 `check_doc_metrics.py` 现算 |

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
| 测试文件 | **405**（`.rs` **317**；unit **137** / integration **169** / e2e **3** / perf **5**） | `python3 scripts/ci/check_doc_metrics.py --print` |
| 基线索引定义 | **344** | 同上 |
| 活路由 / 派生表行 / `INDEXES.md` 人工表 | **980** / **982** / **134** | 同上（`LIVE_ROUTES` / `DERIVED_TABLE_ROWS` / `INDEXES_DOC_ROWS`） |
| clippy allow | **160 处 / 111 文件**（扫描 **833** 个 `.rs`）；其中无理由 **0 / 0**（棘轮基线） | `python3 scripts/ci/check_clippy_allow_ratchet.py`（全量站点走其 `scan()`） |
| `let _ =` / `unwrap_or_default()` / `if let Ok(` | 生产 **517** 行 / **193** 文件；全仓 **1614** 行 / **357** 文件 | 全仓 = `git ls-files -z '*.rs' \| xargs -0 grep -cE 'let _ =\|if let Ok\(\|unwrap_or_default\(\)'` 求和；生产口径 = 8 个 crate 根（`PRODUCTION_ROOTS`）且剔 `tests/`、`*/tests.rs`、`*_tests.rs`、测试基建文件（`test*.rs`）与内联 `#[cfg(test)]` 块，由 `check_doc_metrics.py` 现算 |
| `tokio::time::sleep` | **104** 站点（PROD **26** / TEST_INLINE **34** / TEST_FILE **44**） | `python3 scripts/ci/check_sleep_ratchet.py` |
| `>2000` 行 `.rs` | 生产 **0**（W11 前 7，内联测试外移后最大 1557）；测试基建 **2**（`test_isolation.rs` **3375**、`test-utils/lib.rs` **2786**）；生成 **1**（`derived_route_table_always.inc.rs` **5512**）；测试 **3**（`room_service_tests_migrated.rs` **5248**、`event/db_tests.rs` **3516**、`sync_service/tests.rs` **2510**） | `git ls-files '*.rs' \| xargs wc -l \| awk '$1>2000'` |
| 依赖分裂 | crate 名 **47** / 多版本 **32** / 同版本多来源 **15** | `cargo tree -d --workspace --locked` 按 crate 名聚合 |
| `check_*` 门禁脚本 | **58 个，全部可达或已登记（豁免 0）** | `python3 scripts/ci/check_gate_reachability.py` |
| 基线 SQL `updated_ts` 列 | **85 列**：`NOT NULL` **43** / 可空 **42** | `python3 scripts/ci/check_schema_timestamp_convention.py --print` |
> **生产静默忽略先从 831 降到 826、再由 826 降到 517（2026-10-28），两次都是口径纠正，不是修代码。**
>
> **第一次（831 → 826）**：那 5 行 `let _ = drop_schema(...)` 本来就在
> `#[cfg(test)] mod db_schema_pool_tests` 里，但这行的剔除规则只看**路径**（`tests/`、
> `*_tests.rs`），而它们当时躺在 `lib.rs` —— 文件名不像测试文件，于是一直被算成「生产」。
> 把该测试模块拆到 `synapse-test-utils/src/db_schema_pool_tests.rs`（同时解决 `lib.rs`
> 触顶 2995 行棘轮）之后，它们按文件名归入测试口径。
>
> **第二次（826 → 517，文件 241 → 193）**：上一次的收尾句已经点出「这条规则的真实盲区是内联
> `#[cfg(test)]`」，本轮把它补上了。826 行 / 241 文件实际拆成四桶，只有第一桶是生产：
>
> | 桶 | 行 | 文件 | 举例 |
> |---|---|---|---|
> | 真实生产 | **517** | **193** | `room/lifecycle/create.rs` 的 `let _ = tx.rollback().await;`（合法：回滚失败已无补救动作） |
> | 生产文件内联 `#[cfg(test)]` 块 | 184 | 48 | `retention_service.rs::cleanup_test_room`、`call_session.rs::cleanup_test_data` |
> | 生产根内测试基建文件（`test*.rs`） | 61 | 6 | `synapse-common/src/test_isolation.rs` |
> | 非生产根（`benches/` 30、`synapse-test-utils/` 13、`scripts/` 9、`vendor/` 5） | 57 | 7 | bench 夹具的 `let _ =` |
>
> 全仓 1614 行 / 357 文件（文件数 +1：2026-10-28 把两条 prune 回归测试从
> `test_isolation.rs` 拆到 `synapse-common/src/test_isolation/tests/prune_tests.rs`，
> 同时让该文件回落到行数棘轮上限之内）。**任何「测试写在生产文件里」的仓库
> 都会把测试代码记成生产债务**，所以 `check_doc_metrics.py` 现在按花括号配对屏蔽
> `#[cfg(test)]` / `#[cfg(all(test, …))]` 整块，而 `#[cfg(any(test, feature = "test-utils"))]`
> 仍算生产 —— 带该 feature 构建时它确实进产物。
>
> 行数 **1612 → 1614（+2）**：新增的守卫用例
> `sqlx_dynamic_literal_guard_tests::cfg_test_gated_path_submodule_is_excluded_from_production`
> 里有两行**夹具字符串** `let _ = sqlx::query("…")`（与同文件既有夹具同一写法）。
> 这一项是**行正则**，字符串里的也照计 ⇒ 属**已知过宽**的口径；它不改变任何真实债务，
> 但会随夹具数量漂移，所以每次动夹具都要同批更新本块。
>
> **517 / 193（2026-10-28 晚）**：工作区里有另一个会话未提交的 `.rs` 改动
> （`synapse-e2ee/src/{cross_signing,device_keys}/service.rs`、
> `synapse-federation/src/event_broadcaster.rs`），它同时让生产桶少了 7 行、1 个文件；
> 本轮只新增了一个测试文件（`tests/unit/coverage_drift_attribution_tests.rs`，落在 `tests/`
> 口径，不影响生产桶）。该会话提交后需再对一次读数。

> 漂移是正常的（代码在动）。**不要把本表当常量**：下一轮先跑上面这几条命令再引用。
> 除「依赖分裂」「`check_*` 门禁脚本数」「`>2000` 行明细」三行外，其余数字都由
> `scripts/ci/check_doc_metrics.py` 的内联锚点守住 —— 改代码会让数字对不上、改文案会让
> 锚点失配，两种都会红。这三行需要人工复核（前两个要跑 `cargo tree` / 元门禁，成本高于
> 其余扫描；明细行只做记录，真正的守卫是「生产 0」这一条）。

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

### ARCH-03（P2）"未实现"存在两套错误语义 —— ✅ 已关闭（见附录 D）

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

- **位置**：
  - `migrations/00000000_unified_schema_v12.sql`：`events` 是**普通表**（L325
    `CREATE TABLE IF NOT EXISTS events (`，无 `PARTITION BY`）；v11 增量段 L4679 只留了一行待办注释
    「events 分区（需要独立处理：先 ADD PARTITION CONSTRAINT 后分批迁移）」—— 分区**从未落地**。
  - `synapse-services/src/retention_service.rs`（`run_cleanup` → `storage.delete_local_messages_before`）
    与 `synapse-storage/src/retention.rs`（实际删除语句）。
- **影响**：消息表随运行时间线性膨胀，索引与 autovacuum 成本持续上升；`room_memberships` 只按
  `ON DELETE CASCADE` 跟随房间删除，没有独立归档，历史房间的成员轨迹不可追溯。
- **现状**：`PERF-06` 已关闭，删除**已分批**（`synapse-storage/src/retention.rs` 走
  `crate::pruning::prune_in_batches(PRUNE_BATCH_SIZE, …)`，按 `ctid IN (SELECT … LIMIT …)` 循环），
  所以原记录里的「单条 `DELETE` 长事务 / 大房间拖垮 WAL」**已不成立**；剩下的只是
  **表规模线性增长 + 无归档**。分区/归档未动。
- **修法**：按 `origin_server_ts` 声明式分区（或至少按月归档表）。
- **为什么还开着**：分区收益取决于真实数据量与写入分布，本机无生产级数据；做它等于重写统一基线
  SQL（`EXPECTED_BASELINE_FINGERPRINT` 是契约），属**等外部条件**（§0.2.2）。
- **更正**：本节原写位置 `synapse-services/src/admin/retention.rs` —— **该路径不存在**（2026-10-28 更正）。

### ARCH-05（P2）7 个生产文件 >2000 行 —— ✅ 已关闭（本笔）

| 文件 | 原行数 | 拆后 | 生产段 |
|---|---|---|---|
| `synapse-storage/src/room/mod.rs` | 2531 | 1557 | 1557（`db_tests` 外移 967 行） |
| `synapse-storage/src/membership/mod.rs` | 2502 | 1187 | 1187（`tests` 188 + `db_tests` 1133 外移） |
| `synapse-storage/src/refresh_token/mod.rs` | 2478 | 1258 | 1258（`tests` 1249 外移） |
| `synapse-common/src/error.rs` | 2442 | 1230 | 1230（`tests` 1246 外移） |
| `synapse-storage/src/device/mod.rs` | 2382 | 1373 | 1373（`tests` 356 + `db_tests` 620 外移） |
| `synapse-common/src/config/mod.rs` | 2324 | 1408 | 1408（`tests` 916 外移） |
| `synapse-federation/src/client.rs` | 2086 | 1448 | 1448（`tests` 674 外移） |

- **口径边界**：以下**不计入**拆分目标 —— 生成物 `derived_route_table_always.inc.rs`（5512，改生成器而非产物）、
  测试基建 `synapse-common/src/test_isolation.rs`（3305）与 `synapse-test-utils/src/lib.rs`（2973）、
  测试文件 `tests/integration/room_service_tests_migrated.rs`（5255）等 3 个。
- **原判定被推翻**：本节原写"拿不出可失败验证（拆完行为不变）"。实测**不需要证明行为不变**——
  7 个文件里 5 个超 2000 行的主因就是**内联测试模块**（`error.rs` 生产段只到 1229 行，
  后面 1251 行全是 `mod tests`），把测试块按仓库**既有约定**（`event/db_tests.rs` /
  `thread/db_tests.rs` / `room_summary/db_tests.rs` 都是独立测试文件）外移即可，
  模块路径仍是 `<父模块>::<name>`、`use super::*` 语义不变 ⇒ 零行为变更。
- **落地**：9 个内联测试模块外移到同级文件（`room/db_tests.rs`、`membership/{tests,db_tests}.rs`、
  `refresh_token/tests.rs`、`error/tests.rs`、`device/{tests,db_tests}.rs`、`config/tests.rs`、
  `client/tests.rs`），父文件只留 `#[cfg(test)] mod X;`（`#[allow(...)]` 等属性留在声明上，
  lint 属性对模块内容同样生效）。
- **判据（可失败）**：`scripts/ci/file_size_baseline` 的 7 条**逐条收紧**到新行数，
  再把 `room/mod.rs` 人为追加 20 行 ⇒ 闸门把它列为 `room/mod.rs: 1557 -> 1577`；
  删掉追加后该条目从失败列表消失（实测两次，见提交）。编译判据：
  `cargo check -p synapse-common --all-targets`、`-p synapse-federation --all-targets`、
  `-p synapse-storage --features test-utils --tests` 全绿（零警告）；
  跑通 `error::tests` 78 passed、`config::tests` 67 passed、`client::tests` 34 passed、
  `room::db_tests` 44 passed、`membership::` 39、`refresh_token::` 41、`device::` 54。
- **连带（已登记 NEW-20）**：外移出的 9 个测试文件里有 7 个 >600 行，按仓库现状（`event/db_tests.rs`
  3471 行同样在册）进入基线；同时发现 `check_file_size_ratchet.py` **在 HEAD 上本来就红**
  （67 个文件涨过上限 + 2 个新增 >600 行文件），本笔**没有**整表 `--update`。

---

## 二、代码质量

### CODE-01b（P2）`hash_token` 缺 secret 时 panic；8 个 `src/bin` 入口完全不校验 —— ✅ 已关闭（见附录 D）

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

> **不计入 §0 计数**：本节保留的是"待定口径"，**尚未确认是缺陷**。要计入就得先定义口径，
> 否则 52 这个混口径数字会变成假增长（见下）。

- **原记录**：9 个函数 >150 行（最长 461 行）。
- **本轮机械重算**：生产文件里 `fn` 体 >150 行的有 **52** 条 —— 但其中包含 `#[cfg(test)] mod` 里的
  测试函数与生成文件，**与原口径不可比**。按"先定义口径再统计"的纪律，本轮**不改判**。
- **下一轮要做的事**：先定义三条边界（是否含 `#[cfg(test)]`、是否含生成物、是否按行数还是圈复杂度），
  然后再报数。**不要**把 52 写进缺陷清单 —— 那是口径混出来的假增长。

### CODE-08（P2）已注册端点恒 403，用无理由 `#[allow(dead_code)]` 掩盖 —— ✅ 已关闭（见附录 D）

- **位置**：`synapse-web/src/routes/e2ee/devices.rs:108` 一带（`GET /_matrix/client/v3/rooms/{room_id}/keys/distribution`
  恒 403，自述 "server-internal endpoint"）
- **现状**：该端点已由 `check_unsupported_endpoints.py` 的 16 条 by-design 清单覆盖（双向比对，
  恒失败 handler 必须登记）。**剩下的问题是那个 `#[allow(dead_code)]` 没有 `reason`**，
  读代码的人分不清"预留"与"忘了删"。
- **修法**：补 `reason = "by-design unsupported: …"` 指向契约条目。

### CODE-10（P2）生产 517 行 `let _ =` / `unwrap_or_default()`，无收敛策略

- **口径**：生产 **517 行 / 193 文件**；全仓 **1610 行 / 356 文件**（命令与门禁见 §0.4 ——
  `grep -c` 计的是**行**，一行里两处只算一次）。生产口径 = 8 个 crate 根且剔 `tests/`、
  `*_tests.rs`、测试基建文件（`test*.rs`）与内联 `#[cfg(test)]` 块。
- **826 → 517 是口径纠正，不是修了 306 行**：原数字把生产文件里的内联测试模块（184 行 / 48 文件，
  如 `retention_service.rs::cleanup_test_room`）、生产根内的测试基建文件（61 行 / 6 文件）与
  非生产根（57 行 / 7 文件：`benches/` 30、`synapse-test-utils/` 13、`scripts/` 9、`vendor/` 5）
  都算成了生产债务。分解表见 §0.4 的注。
- **已做**：W0 只加了"危险性扫描"（缓存/存储删除禁止 `let _`）+ 3 个 helper 收敛 19 处缓存失效。
- **明确不做**：总数下降配额（§10.10：配额会诱导写 `unwrap()` 来"减少计数"）。
- **下一轮可做的**：按"错误被吞后有没有用户可见后果"分类，只处理前两类；不要动 `unwrap_or_default()`
  在 `Option` 语义下的合法用法。
- **2026-10-28 复核：517 行里"DB 错误 ≡ 未找到"只有 22 处**。生产桶中 `if let Ok(` 共
  **145 行 / 62 文件**（占 517 的 28%），按被吞掉的错误来源分桶：fs/serde/parse **59**、
  cache **26**、other **24**、疑似 DB **22**、其余 `await` **14**。前两桶是合法降级
  （缓存失效、`Option` 解析、磁盘回退），不是债务。22 处 DB 吞错按**用户可见后果**分档：
  - **静默降级（2 处，后果最大）**：`/keys/query` 在 `get_cross_signing_keys_batch` 失败时以
    **200 返回空 cross-signing keys**（`synapse-e2ee/src/device_keys/service.rs:204`）—— 客户端会
    判定对端未上传 CS key；`get_joined_members` 失败时联邦接收方为空、事件**静默不外发**
    （`synapse-federation/src/event_broadcaster.rs:465`，上层只有 `tracing::warn!`）。
  - **fail-closed 但诊断误导（5 处）**：`cross_signing/service.rs` 的 225 / 240 / 444 / 621 / 820
    把 DB 错误包装成 400「no ed25519 device key found / master key not found」，
    排查会去查密钥缺失而不是数据库。
  - **局部视图退化（1 处）**：presence 成员集退化成只剩自己
    （`synapse-services/src/sliding_sync_service/extensions.rs:208`）。
  - **有兜底 / 设计如此（不算债务）**：`friend_room_service/mod.rs` 的 103 / 139 / 153 把 DB
    错误当成"没有好友房"，靠 `room_memberships` 的 UNIQUE 兜底；`cross_signing/service.rs:735`
    有 fallback 分支；`media_service.rs:1021` 是设计上的磁盘回退。
  - **裁决**：本轮按 §10.2「无用户影响证据不做」**只登记不修**。前两档若要修，代价是把
    `if let Ok(..)` 换成 `?`（200/400 变 500），必须同步改对应测试 ⇒ 下一轮独立改动。

### CODE-11（P3）棘轮口径 **159 site / 0 处无 `// reason:`（0 文件）**，基线 **0/0** —— ✅ 已关闭（W12，见附录 D）

- **现状**：`check_clippy_allow_ratchet.py` + `clippy_allow_baseline` 已在工作（只许下降，
  新条目必须带理由），所以**"无理由 allow 增长"已经不可能**。
- **存量**：存量 **0 处** 无理由（0 文件）—— 关闭前是 **157 处 / 94 文件**（129 个属性位，其中 28 个
  同时抑制 2–3 个 lint）。本笔按 lint 类逐个补 `// reason:`，不是模板字面量：`too_many_arguments`
  写明"参数与 SQL 列序 / 依赖集一一对应"，`unwrap_used` 写明"测试夹具失败即应 panic"，
  `unused_async` 写明"axum handler 的签名约束"，`type_complexity` 写明"`Arc<RwLock<…>>`
  就是 fake 状态本身的形状"等。
- **顺带修掉棘轮的自相矛盾**：`main()` 原先把**空基线**判成 `exit 2`，而"零无理由"正是棘轮的
  目标态 —— 等于禁止自己成功。改为只有**基线文件缺失**才 fail-closed（扫描器为空的保护仍在：
  `files == 0 or not sites`）；`doc_credibility_guard_tests` 的 `assert!(files > 0)` 同步换成说明注释。
- **数字仍不手抄**：`doc_credibility_guard_tests::clippy_allow_counts_in_the_audit_match_the_ratchet_baseline`
  从 `scripts/ci/clippy_allow_baseline` 现算「无理由处数 / 文件数」，与本标题及 §0.2.3 那行逐字比对；
  基线降了而这里没跟 ⇒ 红。

---

## 三、功能完整性

### FUNC-05（P3）联邦能力项有意不注册，未全部登记为设计决策 —— ✅ 已关闭（见附录 D）

- **已登记**：thirdparty 4 条 + 联邦 key 上传 5 条（RD-15/16）。
- **未登记**：`/_synapse/admin/v1/backups` 等"无路由"项，读者会误判为遗漏。
- **修法**：随能力台账重做（MAINT-04）一起进 `ROUTE_CONTRACT.md` 的生成段。

---

## 四、性能表现（含数据层）

### PERF-01（P2）`to_device_messages` 对普通设备无按龄清理 —— ✅ 已关闭（本笔）

- **事实**：`/sync` 走 `get_messages_since(user, device, since, limit)`（增量 + 有界），**不是**全量读取
  —— 初轮的"P1 全量读取"已被第二轮证伪。真正的问题是该表**对普通设备没有任何 retention**：
  全仓唯一的 `DELETE FROM to_device_messages` 在脱水设备路径（`dehydrated_device.rs`）。
- **影响**：长期不活跃设备的消息永久累积；配合 RD-27 的"名字相反的死 API"，排查时会查错方向。
- **语义裁决（原卡点被实现推翻）**：`claim_messages` 是 `WITH deleted AS (DELETE … RETURNING)`、
  `delete_messages_up_to` 按 `stream_id <= $3` 删 —— **取走即删**。因此表里存活超过窗口的行
  **只能**是"设备从未回来取"的积压，不需要客户端投递确认就能判定。代价显式接受：
  离线超过窗口的设备会丢积压（与 `DEVICE_LIST_STREAM_RETENTION_DAYS` 同等的取舍），
  设备回来后从 `/sync` 重新拉当前状态。
- **落地**：`synapse-storage/src/pruning.rs` 新增 `TO_DEVICE_MESSAGES_RETENTION_DAYS = 30` 与
  `prune_old_to_device_messages`（复用 `PERF-06` 的 `ctid` 分批；`retention_days == 0` 直接
  `Ok(0)` = 关闭，**不是**"删掉早于此刻的全部行"）；`RetentionConfig` 新增
  `to_device_retention_days`（默认 30）；`src/server/mod.rs` 的 prune 循环加第 11 步；
  `docker/config/homeserver.yaml` 显式写出 30 并注明 `0` 为关闭。
  **不建 `created_ts` 单列索引**：选择性差，`ctid ... LIMIT` 顺序扫描在 30 天量级下更正，
  且不改变 `idx_to_device_ordered` 的写入成本。
- **判据**：`pruning::db_tests::prune_to_device_messages_respects_window_and_the_zero_day_off_switch`
  （真库）—— 40 天前 2 条 + 1 天前 1 条 ⇒ 按 30 天删 2 留 1，且留下的是 1 天前那条；
  `retention_days = 0` 时删 0 行且三行全在。`cargo test -p synapse-storage --features test-utils
  --lib pruning::` **17 passed**；`synapse-common` 的 `config::retention` 1 passed。
- **变异自证**：① 删掉 `if retention_days == 0 { return Ok(0); }` ⇒ 红
  （`0 days disables retention, left: 3, right: 0`）；② `created_ts < $1` 改 `> $1` ⇒ 红
  （`only messages the device never collected are dropped, left: 1, right: 2`）；
  两次还原后均绿（`git diff --stat` 确认无残留）。
- **连带**：`check_sqlx_dynamic_ratio.sh` 动态生产 +1（写的是运行时 `sqlx::query()`，
  不新增 `.sqlx` 缓存条目），基线已同步并在文件内写明 +1 来源，详见
  `SQLX_STATICIZATION_PLAN_2026-09-23.md` 的 2026-10-26 随访块。

### PERF-10（P3）PostgreSQL 实例级未设 `statement_timeout` —— ✅ 已关闭（2026-10-26，`812661672`）

- **事实**：应用为每个连接 `SET statement_timeout='30s'`，并配了 `idle_in_transaction_timeout_secs: 60`；
  缺的只是**实例级**（`postgresql.conf`），因此直连 psql 的运维查询不受保护。
- **落地**：`docker/config/postgres.conf` 新增「会话超时（实例级兜底）」小节 ——
  `statement_timeout = 5min`、`idle_in_transaction_session_timeout = 60s`。取值**刻意不比应用更严**：
  应用连接由 `after_connect` 的会话级 `set_config` 覆盖，实例级只防直连无限挂起；写成比 30s 更小
  只会提前掐断运维手工 `CREATE INDEX` / `VACUUM`。`scripts/ci/check_connection_budget.py` 增加
  第 7 步（两键必须显式存在，缺则 `exit 2`；`实例级 ≥ database.rs 默认`，违例 `exit 1`）；
  `docker/deploy/README.md` 新增同名小节写明「应用侧已兜底，这里防的是直连」。
- **判据/变异**：`statement_timeout` 改 `10s` ⇒ 门禁红（`exit 1`）；删掉该键 ⇒ 门禁红（`exit 2`）；
  还原后绿（三次实测）。
- **边界**：生产实例的 `postgresql.conf` 由部署方管理，本仓只覆盖 docker 这一种形态。

### PERF-11（P3）N+1 查询剩 1 处（原 3 处，风险可控）

- **位置（仅剩）**：`synapse-services/src/sliding_sync_service/filters.rs` —— 列表查询拿到 `rooms`
  后逐个 `build_room_json`（每房间一次成员/状态查询）。
- **已消除 2 处**：
  - `synapse-services/src/user_service.rs`：两处「每个共享房间查一次成员」改成
    `membership_storage.get_members_batch(&rooms, "join")`（代码里有
    `// One query for all shared rooms instead of one per room.`）；
  - `synapse-services/src/room/lifecycle/create.rs`：创建房间后的逐设备/逐 appservice 循环改写为
    `invalidate_room_state` + `dispatch_appservice_events_for_room`。
- **为什么不改剩下这一处**：该路径**已有 bench 组覆盖**（`benches/performance_sliding_sync_benchmarks.rs`
  的 `sync_response` 走 `service.sync()` 全链路，`sliding_sync_p95_p99_latency` 出 p95/p99，
  由 `scripts/ci/sliding_sync_perf_gate.sh` 强制要求执行），但**没有任何一次测量指认它是热点**。
  批量取状态需要新的 storage API 并改变内存形状 ⇒ 按 §10.2「无测量依据不做」处理。
- **2026-10-28 复核：逐房查询是条件性的，现有仪器也分辨不到它**（结论不变，理由写准）：
  - 三条腿都**不是**固定每房 3 次 —— `build_required_state_events` 在 `required_state` 为 `None`
    时直接返回空（0 次），否则走 `cached_room_state`（缓存未命中才 1 次）；`build_timeline`
    在 `timeline_limit` 为 `None`/`0` 时返回空（0 次），否则每房 1 次；sticky 只在配置了
    `sticky_event_storage` 时每房 1 次。典型形态是每房 1 次（timeline），不是 3 次。
  - **bench 只覆盖 timeline 那条腿**：`build_initial_sync_request` 的 `required_state` 为 `None`，
    `timeline_limit` 为 `Some(10)`，状态腿（缓存未命中时的那次查询）**从未进过 bench**。
  - **门禁阈值没有分辨力**：`PerformanceConfig::default().sliding_sync_latency_threshold_ms = 5000`
    而实测 p95 为 0.7–11ms（100 房）；500 房 × 每房数 ms 也够不到 5000ms ⇒ N+1 回归推不红门禁。
  - 因此准确表述是「**仪器在、但只测了一条腿、阈值也分辨不到**」，不等于「测量证明它不是热点」。
    要让这条变成"有据可改"，前置条件是：bench 带上 `required_state`，且阈值按房间数分档收紧。

### PERF-12 / MAINT-07（P3）`INDEXES.md` 与 SQL 的计数漂移（整份生成明确不做）

- **已做**：计数与"人工表行数"进了 `doc-metrics` 块，由 `check_doc_metrics.py` 现算校验（343 / 134）。
- **不做**：整份重生成。人工表里的"为什么需要这个索引"**无法从 DDL 反推**，生成 343 行会让文档
  从"索引治理"退化成"索引清单"（§10.10）。

---

## 五、安全性

> 原 15 条安全项**已全部关闭或裁决**（SEC-01…SEC-08、SEC-10、SEC-15 已修；SEC-09 / SEC-12 / SEC-13
> 本轮或上一轮收口；SEC-11 判为误报、SEC-14 判为原建议有误——均见附录 D）。
> 本节**已无开放项**：最后一条 SEC-12 不是漏洞而是**裁决**（legacy 匿名读是 Matrix 语义），
> 本轮把口径登记进部署文档，并用守卫把「文档 / r1 收紧 / r0·v1 保持匿名」绑在一起。
>
> 下面四条已关闭项保留原正文作历史，判定与证据在附录 D。

### SEC-09（P3）CAS 回调的 `redirect_after` 未做同源校验 —— ✅ 已关闭（2026-10-26，`e83fdbb6f`）

- **位置**：`synapse-web/src/routes/cas.rs`（`/login?service={redirect}`）
- **影响**：开放重定向 —— 攻击者可构造看起来指向本域的 CAS 登录链接，登录后跳去外站。
- **修法**：`redirect_after` 只接受同源路径；非空且非同源 ⇒ 400。
- **落地**：`is_same_origin_path` 要求「以 `/` 开头、不是 `//host`、不含 `\`、不含控制字符」
  （浏览器会把 `/\host` 归一化成 `//host`，所以单独挡 `\`）；非法值回 `400 M_INVALID_PARAM`，
  且**不回显**原值（它是攻击者可控的）；空串等同"没传"。11 组输入的单测覆盖接受/拒绝两侧。

### SEC-11（P3）静态密钥封装存在全零密钥死代码 —— ✅ 不成立（误报，2026-10-26）

- **位置**：`synapse-common/src/key_encryption.rs`（`let mut okm = [0u8; 32]`）一带
- **判定**：`okm` 是 **HKDF 的输出缓冲区**，紧随其后的 `hk.expand(info, &mut okm)` 把 32 字节
  全部覆盖后才使用；`[0u8; 32]` 只是"先分配后填充"的写法，不是"以全零密钥工作"。
  该处已带 `#[allow(clippy::expect_used, reason = …)]` 说明 fail-closed 意图。**不改代码。**

### SEC-12（P3）legacy r0/v1 媒体下载端点匿名可读 —— ✅ 已关闭（2026-10-26，登记 + 双向守卫）

- **位置**：`synapse-web/src/routes/media/mod.rs`（`create_media_r0_router` / `create_media_v1_router`
  合并 legacy 下载 router；handler `download::download_media_v1` 取 `OptionalAuthenticatedUser`）
- **判据**：这是 **Matrix legacy 语义**（媒体仓库在 v1.11 之前允许匿名读），收紧会破坏老客户端。
  **原描述有偏差**：审计原文写「r0/v1/r1 匿名可读」，实际 **r1 早已挂 `auth_middleware`**
  （VULN-01 / VULN-02，`api_security_vuln_fix_tests` 有 401 证据）；匿名的是 r0/v1，
  外加 v3（`download`/`thumbnail` 仍可匿名，`download_signed` 走 HMAC 签名）。
- **修法**：登记为设计决策 + 部署文档说明"如需关闭匿名读，需按客户端版本分布决定"。
- **落地**：`docker/deploy/README.md` 新增「媒体下载的匿名读语义」小节，用一张表列全六类路径
  （r0/v1 匿名、v3 匿名、v3 signed HMAC、r1 必须 token、`/_matrix/client/v1/media` 必须 token）
  与"要关就改代码"的入口。守卫
  `doc_credibility_guard_tests::legacy_media_anonymous_read_is_registered_and_matches_the_router`
  四个断言组把四方钉住：① 部署文档小节必须存在且点名 r0/v1 + `auth_middleware`；
  ② `create_media_r1_router` **必须**挂 `auth_middleware`；③ `create_media_r0_router` /
  `create_media_v1_router` **不得**挂它、且必须合并 `create_media_legacy_download_router()`；
  ④ handler `download_media_v1` 仍取 `OptionalAuthenticatedUser`。用新增的 `fn_body` 按
  **函数体切片**判断，避免"同文件里 r1 挂了鉴权 ⇒ 整文件 `contains` 让 r0 的断言假通过"。
- **变异自证**（三条，均实测）：删 README 小节标题 ⇒ 红（"找不到「媒体下载的匿名读语义」小节"）；
  给 `create_media_r0_router` 挂 `auth_middleware` ⇒ 红；去掉 r1 的 `auth_middleware` ⇒ 红。

### SEC-13（P3）联邦鉴权 DEBUG 日志打印签名原文 —— ✅ 已关闭（2026-10-26，`c2783c680`）

- **位置**：`synapse-web/src/middleware/federation_auth.rs`
- **影响**：DEBUG 默认关闭；一旦为排障打开，日志里就有可重放的签名原文（配合 SEC-14 的窗口问题更危险）。
- **修法**：改成打印签名哈希的前 8 位。
- **落地（原描述有偏差）**：泄露的不是签名串，而是**被签名的规范化请求字节**（含 room 消息、
  to-device 载荷）。改为字段化 `origin` / `len` / `fingerprint`（`signature_fingerprint` 取
  SHA-256 前 8 字符），并加源码棘轮 `debug_logs_do_not_dump_signed_bytes`
  （用 `concat!` 拆针，避免门禁匹配到自身）。
- **残留**：`ROUTE_CONTRACT.md` 的行号引用已随同批刷新（漂移 57 → 55）。

### SEC-14（P3）联邦请求缺 `ts` 仅告警不拒绝 —— ✅ 已关闭（裁决：不改，见附录 D）

- **位置**：`synapse-web/src/middleware/federation_auth.rs`（`signing_ts` 缺失走 warn 分支）
- **影响**：无 `ts` 的请求无法参与重放窗口判定，等价于"跳过新鲜度检查"。
- **修法**：缺失即拒绝（401），并补一条"缺 ts ⇒ 401"的用例（**先写用例证明当前会放行**）。
- **判定**：`ts` **不在规范里**（Authentication 一节只列 `origin`/`destination`/`key`/`sig`，
  且写明 unknown parameters 忽略），本仓出站请求也不带 `ts` ⇒ 按原建议硬拒会拒掉自己发出的
  每一个联邦请求。已改为显式建模 `TimestampCheck` 四态 + 一条"文档说我们不发 ts"与代码
  双向一致的门禁。

### SEC-15（P3）鉴权覆盖率约 94.4%，client/media 面存在未覆盖路由 —— ✅ 已关闭（见附录 D）

- **原口径的问题**：「94.4%」是一次静态统计，**没有可复跑的定义**——"哪些路由本来就该公开"
  没有落成文件，所以既无法重测，也无法判断剩下的 5.6% 里哪些是真缺口。
- **重测（2026-10-26，`scripts/contract/check_route_auth.py` 现算）**：全域 **979** 条路由，
  其中 **818** 条 handler 签名带 `AuthenticatedUser` / `AdminUser` / `OptionalAuthenticatedUser`；
  其余 **161** 条逐条登记（`public 76 · federation 43 · middleware 8 · secret 14 · manual 20`）。
  ⇒ **需凭证 903/979 = 92.2%**，**真正无凭证 76 条 = 7.8%**（`public` 那一档，含 `/_matrix/client/*`
  的登录/注册/能力发现、`/_matrix/media/*` 公开缩略图、well-known、`/_health` 等）。
  若只认 handler 签名提取器这一种形态，则是 **818/979 = 83.6%**，剩下的 65 条靠 route_layer 中间件
  或共享密钥；三个数字**不可互相替代**，引用时必须说明分子口径。
- **与旧 94.4% 的关系**：旧值是按桶估的静态计数（归档记为 client 301/321、media 28/37、
  federation 36/48），分母只有约 406 条，也**没有**区分"签名"、"中间件"、"共享密钥"；
  新口径的分母是 979 条（含 `/_synapse/*` 管理面、worker、appservice、CAS/OIDC 等旧统计未覆盖的桶）。
  两者不是同一件事的两次测量，所以不复述"涨了/跌了"。
- **口径落地**：白名单写在 `scripts/contract/route_auth_public.txt`，格式 `METHOD PATH<TAB>kind<TAB>reason`，
  五种 kind 各自表达"凭证从哪来"：`public`（确实不需要）/ `federation`（X-Matrix 签名）/
  `middleware`（route_layer 上的鉴权中间件）/ `secret`（共享密钥或 HMAC）/ `manual`（handler 自己校验 bearer）。
- **为什么必须逐条而不是前缀规则**：路由抽取器把 `layer` / `route_layer` 当透明装饰器，
  **看不到**中间件挂载；而 `/_matrix/federation/` 下有 8 条规范要求公开的端点、`/_synapse/admin/`
  下同时存在两种保护机制 —— 任何整段前缀规则都会判错。
- **可失败验证**：门禁双向 —— 路由无提取器且未登记 ⇒ 红；登记了但路由已消失、或现已带提取器 ⇒ 也红。
  已做「删掉 `GET /_matrix/client/v3/login` 那一行 → 门禁红 → 还原 → 绿」的探针自证，
  并接了 `--self-test`（16 例）与 `preflight.sh` 的独立 step。

---

## 六、可维护性

### MAINT-01（P2）测试替身、工具二进制与生产 crate 未做特性隔离 —— ✅ 已关闭（2026-10-26，见附录 D）

- **原判**：`user_store_fake`（RD-07）、`federation_test_keys`（RD-08）已 gate，但 `synapse-common`
  的四个测试基建模块（`test_database_guard` / `test_isolation` / `test_schema_guard` /
  `test_schema_orphans`）**无条件编译**，随每个生产二进制进入依赖图。
- **已做**：四个模块改为 `#[cfg(any(test, feature = "test-utils"))]`；根 crate 与 5 个成员的
  `[dev-dependencies]`、`synapse-test-utils` 的 `[dependencies]` 显式打开
  `synapse-common/test-utils`（依赖方的 `cfg(test)` 对被依赖 crate 不可见，所以只能在测试侧
  打开；dev-dep 的 feature 不并入非测试构建）。
- **判据**：`doc_credibility_guard_tests::test_infrastructure_is_feature_gated_and_never_ships_in_production`
  四段断言（gate 紧邻 / dev 段必须开 / 生产段必须没开 / `synapse-test-utils` 只经 `test-utils`
  feature 拉入）；`cargo check --workspace --lib` 与
  `cargo check --workspace --all-targets --features test-utils` 均 0 退出。

### MAINT-04（P2）能力台账手工维护，已出现多处失真 —— ✅ 已关闭（见附录 D）

- **证据**：FUNC-04 的两行（`animated`、`M_APPSERVICE_LOGIN_UNSUPPORTED`）**两轮都被判为"与实现相反"**，
  本轮已改对并加了守卫（`capability_ledger_absence_claims_match_the_code`：代码里有该标识符 ⇒
  台账不许说 `MISSING` / `0 命中`）。
- **根因仍在**：整张表靠手抄。生成器要能反推"为什么没实现"，做不到；**折中**是把"证据必须可复现"
  变成判据（本轮做的），而不是生成整张表。

### MAINT-07（P3）索引台账与迁移说明未同步

> 与 `PERF-12` 合并为 **一条** 登记项计数（§0.2.3 的 `PERF-12 / MAINT-07`），此处只留指针。

- 见 PERF-12：计数已脚本化，整份重生成明确不做。

---

## 七、测试覆盖

### TEST-02（P2）断言强度不足：逐文件状态（2026-10-24 重写）—— ✅ 已关闭（见附录 D）

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

**残留（不在本条内，另登记为 NEW-09 —— 已收口）**：24 个 `NONE` 是**真覆盖缺口**，不是"测过了"。
本条只负责把"假的覆盖"变成"显式的缺口"，补真覆盖是另一件事（每个都要 `setup_fresh_test_app`
- 注册用户 + 逐端点断言，属独立批次）。`NEW-09` 已按这个口径逐族补齐（7 个真缺口 11 族用例、
6 族误判更正、3 族幽灵登记），见附录 D。

---

### TEST-05（P2）测试域 `sleep` 站点 —— ✅ 已关闭（2026-10-24，判据改判）

> ⚠️ 本节在 2026-10-24 的 NEW-05 改写里被误删过一次（那次 splice 的结束锚点越过了本节），
> 现在按当时的内容 + 本轮分类重建；`check_plan_doc_sections.py` 的基线只校验章节数，拦不住这种删法。

分布（`python3 scripts/ci/check_sleep_ratchet.py --print`，**2026-10-24 当时值**）：**PROD 26 /
TEST_INLINE 34 / TEST_FILE 47 = 107**；测试域合计 81。此后 NEW-10 把 4 个轮询折成
`wait_until`，`TEST_FILE` 47 → **44**（当前值以 §0.4 为准，那一行有内联锚点）。

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

### DOC-04（P3）三份文档共 9 组行号漂移 —— ✅ 已关闭（见附录 D）

- **修法**：把"引用行号"改成"引用符号名"（`fn xxx` / `const YYY`），行号每一次改动都会漂。

---

## 九、专项：冗余与过度开发

> W6 五批（`e4ac05ce0` / `b51aafb00` / `f7bf4eec0` / `d9398fed1` + RD-14 接入）已关闭
> **RD-01…RD-26**（含 26 个条目、3 处"审计原文结论作废"的偏差记录）。`RD-27` 已裁决关闭
> （审计前提部分不成立，只删真正死的 2 个方法），判据见附录 D —— **本节已无开放项**。

### RD-27（P2）`to_device` 的 4 个存储方法零调用，其中一个名字与语义相反 —— ✅ 已关闭（见附录 D）

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
| W8 | 测试面（TEST-05/NEW-05/NEW-09/NEW-14）+ 能力台账（MAINT-04/FUNC-05/ARCH-03/CODE-01b/CODE-08）+ 文档漂移（DOC-04） | ✅ 逐条判据见附录 D |
| W9 | 测试基建特性隔离（MAINT-01）+ `#[allow]` 计数纠偏（CODE-11 数字绑基线） | ✅ 本笔（MAINT-01 见附录 D；CODE-11 的"补理由"已由 W12 收口） |
| W10 | P2 数据层保留期：`to_device_messages` 按龄清理（PERF-01） | ✅ 本笔（见附录 D） |
| W11 | P2 文件规模：9 个内联测试模块外移，7 个 >2000 行生产文件全部 ≤1557（ARCH-05） | ✅ 本笔（见附录 D；连带登记 NEW-20） |
| W12 | P3 `#[allow]` 无理由存量清零（CODE-11）：157 处 / 94 文件逐个补 `// reason:`，基线 157/94 → 0/0；顺带把 fmt 棘轮从 9 清到 0 | ✅ 本笔（见附录 D；含"空基线 = 失败"的自相矛盾修正） |
| W13 | P2 文件行数棘轮（NEW-20）：按拍板走**整表 `--update` 一次性批准**，基线 220 → 222 条（67 条增长 + 2 个新增 >600 行文件全部入册），同时收紧 19 条已缩小的 ceiling | ✅ 本笔（见附录 D；闸门 `file_size: 825 files scanned, 222 over 600 (baseline 222) → OK`） |
| W14 | P3 审计 §0.4「数字真值」失真：7 组数字按脚本/基线现算重写（含「84 列」与 43 + 42 的自相矛盾 → **85**、「clippy allow 无理由」160/96 → **0/0** 与 CODE-11 对齐、门禁 47 → **58**）；27 个内联锚点并入 `check_doc_metrics.py`（内联 12 → **39**） | ✅ 本笔（判据：改 §0.4 数字 ⇒ 红；改 `sleep_baseline` 的 `PROD` ⇒ 红；两条均为实测后还原） |

### 10.2 下一批建议

W8 计划的三项（`TEST-05` → `NEW-05` → `FUNC-05`）已全部收口，清单里已无 P0/P1、也无 P3 待裁决项。
剩下 4 条里能动的只有 1 条 P2 结构债，且**卡在一个前提上**，不是排期问题：

| 待办 | 前提 |
|---|---|
| ARCH-04（`events` 分区） | 需要真实数据量证明收益；本机无生产级数据 |
| CODE-10（517 行静默忽略） | 只按"错误被吞有无用户可见后果"分类处理，且**不设配额** |

其余（`PERF-11/12`、`MAINT-07`）都是随触达顺手做或等外部条件的登记项。
PERF-01 已于 W10 收口（`to_device_messages` 按龄清理 + 零天关闭开关，判据与变异见附录 D）；
ARCH-05 已于 W11 收口（生产段全部 ≤1557 行，行数棘轮 7 条逐条收紧，变异自证见附录 D）；
CODE-11 已于 W12 收口（`#[allow]` 无理由存量 157 → 0，基线 0/0，变异自证见附录 D）；
NEW-20 已于 W13 收口（按拍板整表 `--update` 一次性批准，基线 220 → 222 条，闸门转绿，见附录 D）。

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
| NEW-15 | `DELETE /_synapse/admin/v1/background_updates/{job}` 对未知任务返回 204，与同族 `DELETE /registration_tokens/{token}` 的 404 不一致 | **裁决：统一 404 `M_NOT_FOUND`**。理由：静默 204 让"打错任务名"看起来删成功了，运维会误判为已清理，而真正该做的清理漏掉没人发现；同族 admin 删除端点未知即 404，语义应当一致。实现：`BackgroundUpdateStoreApi::delete_update` 返回 `Result<bool, _>`（PG 侧 `rows_affected() > 0`），trait / PG 实现 / trait impl / `InMemoryBackgroundUpdateStore` mock 四处同步；`background_update_service::delete_update` 在 `false` 时 `ApiError::not_found("Update not found")`，handler 不改（错误自动传播）。**幂等保留**：删已存在的任务仍 204，只是第二次删才 404。**判据**：`unknown_update_is_not_found` 的 DELETE 断言从 204 翻成 **404 + `M_NOT_FOUND`** 并转绿；新增服务层单测 `delete_unknown_update_is_not_found`（断言 `error.kind == ApiErrorKind::NotFound`；`ApiError` 是结构体不是 enum）；`test_db_delete_update` 增加「首次 true / 再次 false 且不报错」两条断言 |
| NEW-16 | 不限额（`limit == 0`）时 `GET /_matrix/media/v1/quota/check` 的 `remaining` 报 0，客户端会渲染成"空间已满" | **裁决：`limit == 0` ⇒ `remaining: null`**（`limit` 仍为数值 0，`used` 不变）。理由：`0` 在字节域里天然读作"剩余零"，而本仓 `limit == 0` 的语义是**不限额**（`media/repository.rs::check_quota` 在 `max_storage == 0` 时直接放行）；只把 `remaining` 可空化、不动 `limit`/`used` 的数值语义，对客户端是**单字段的向后兼容**，且让"无上限"变成可判别的显式信号。实现：`synapse-web/src/routes/media/quota.rs` 的 `remaining` 改为 `Option<i64>`。**判据**：`quota_check_reports_consistent_math` 在 `limit == 0` 分支改为断言 `check["remaining"].is_null()`，`limit > 0` 分支仍校验 `remaining == limit - used`；`docs/sdk/media.md` 的 `/quota/check` 响应行补「`limit == 0` 时 `remaining` 为 `null`」 |
| ARCH-02 | `sync` 与 `sliding_sync` 无共享语义基座（时间线裁剪/顺序/`limited` 各写一遍） | **先断言后抽取**：新增 `tests/integration/sync_path_equivalence_tests.rs` 3 条等价断言（完整序列一致 / 裁剪后保留最新 N 条 / 两条路径都置 `limited`）。断言**首次运行即红**，抓到 `get_room_events_batch_inner` 的 `ORDER BY stream_ordering ASC` + `rn <= limit` 取**最旧** N 条 ⇒ 初始同步让长房间的客户端永远看不到最新消息（实测 5 条消息、上限 2：`/sync` 给最旧两条、滑动同步给最新两条）；只对**无 `since`** 的初始同步改 DESC，增量保持 ASC 按批追赶。timeline 层抽成 `sync_helpers::timeline_window`（含 `TimelineOrder`），两条路径共用。判据：修复后 3 passed；`sync_service_tests_migrated` + `sliding_sync_service_tests_migrated` + `sync_handlers_coverage_tests` 全绿；services lib 的 timeline 相关 32 条单测全绿。**state 组装层未共享**，另登记 NEW-11 |
| NEW-13 | `thread` 两处 MSC3440 偏差：`/send` 不登记线程根、`thread_id` ≠ 根事件 id | **已修（两半）**：① `send_message` 在事件落库后 best-effort 调 `ThreadService::ensure_thread_root_for_reply`（幂等：先按根事件查，缺了才建，撞 `UNIQUE(room_id, root_event_id)` 就重读确认；失败只 warn，**不回滚发送**）；② 读侧在 `get_thread` 里"按 thread_id 查不到就按 `root_event_id` 回退"，**不迁移**既有 `thread_roots.thread_id` 值（迁移是破坏性改动，读侧兼容够用）。判据：新增 `threaded_send_alone_is_readable`（纯 `/send` → `GET /threads/{root}` 200 且 `root.root_event_id` 对得上）转绿；`api_thread_read_routes_tests` **6 passed**。遗留（另开 NEW-19）：`/send` 的回复仍不进 `thread_replies` 索引 ⇒ `replies` 为空 —— **NEW-19 已收口**，同一处在登记根之后再写 `thread_replies`（时间戳回查事件本体），见该行 |
| NEW-17 | 滑动同步 `rooms` 结构不符合 MSC3575/MSC4186（扁平 map，规范要 `{join, invite, leave}`） | **已修**：在**序列化边界**分组（新增 `filters::group_rooms_by_membership`），内部仍保留扁平 map —— 因为 `extensions.rs` 的 4 处房间枚举与长轮询的 `has_new_timeline_events` 都按扁平读，改内部会让三处一起漂。分桶依据 payload 的 `invited` 布尔；`leave` 无数据来源但**键必须存在**。判据：`sliding_sync_lists_joined_room` 的断言从"房间在 rooms 顶层"翻成"在 `rooms.join` 里、且顶层不再有该房间"并转绿；`api_sliding_sync_routes_tests`(5) + 滑动同步 service 单测(74) 全绿。**后续（2026-10-25）**：这笔改动让 `sync_path_equivalence_tests` 的三条 ARCH-02 用例失效（它们按扁平 `rooms[room_id]` 读，改桶后一直是红的，当次没复跑）—— 已在 NEW-11 那一笔收口到 `rooms.join[…]` |
| NEW-18 | refresh token 复用检测只吊销 `refresh_tokens`，不断已下发的 access token | **已修**：复用分支在 `revoke_all_user_tokens` 之外补齐 access 侧四步 —— ① 按 `token_hash` 拉黑（校验路径里黑名单检查**早于** token 缓存命中，最可靠）；② `invalidate_revocation_ok_for_user` （必须在删行**之前**，否则枚举为空、标记漏删）；③ `delete_user_tokens` 删 `access_tokens` 行；④ 置 `user:logout_all:{uid}` 标记作纵深防御。`security_audit` 事件补 `revoked_access_tokens` 计数。判据：`reusing_old_refresh_token_revokes_all_tokens` 的断言从 200 翻成 **401** 并转绿；`api_refresh_token_routes_tests`(5) + `api_auth_routes_tests`(8) 共 13 条全绿 |
| NEW-19 | `/send` 的线程回复不进 `thread_replies` 索引 ⇒ 纯 `/send` 的线程读得到根、`replies` 恒空 | **已修**（走审计建议的 (b) 路线：按 `event_id` 回查事件，不动热路径返回结构）：`ThreadService::ensure_thread_root_for_reply` 在登记根与关系行之后补第三件事 —— 写 `thread_replies`。三处细节都是"不能用更省事的写法"的原因：① **时间戳必须抄事件本体**（新增 `ThreadEventPort::canonical_event_origin_server_ts`，生产实现走 `MessagingService::get_event_record` 并在本地比 `room_id`，缺事件回 `Ok(None)` 而不是报错），用本地时钟会得到"线程内顺序与 `/sync` 时间线不一致"；② **`thread_id` 取影子根行的值、取不到退回根事件 id**，与 `get_thread` 读侧的 fallback **同一口径**，否则写进 A、读按 B 仍然读不到；③ **重试同一 txn 必须幂等**：客户端重发会带着同一个 `reply_event_id` 再走一遍，此时撞 `uq_thread_replies_room_event` 按"已索引"处理（`is_duplicate_thread_reply` 只认 `is_unique_violation`，其它 DB 错误一律上报）；`create_thread_reply` 的 `reply_count + 1` 与 INSERT 同事务，INSERT 失败即整体回滚，所以计数不会被重复累加。**判据**：`threaded_send_alone_is_readable` 从"只断言 `replies` 是数组"翻成断言 **包含那条回复** + `reply_count == 1` + `root_event_id`/`in_reply_to_event_id`/`content` 逐字段对得上 + `origin_server_ts` **等于** `GET /rooms/{r}/event/{id}` 的 `origin_server_ts`；新增 `threaded_send_txn_retry_does_not_double_index`（同一 txn 发两次 ⇒ 同一 `event_id`、`reply_count` 仍 1、`replies.len()` 仍 1）；`api_thread_read_routes_tests` **7 passed**。**变异自证**：在 ③ 之前插 `if black_box(true) { return Ok(()) }` ⇒ 两条用例**都红**，失败现场正是修复前的形态（`"replies":[],"reply_count":0`，根仍读得到），还原后 7 passed |
| NEW-10 | 4 个"已带 `break` 条件的轮询循环"仍用裸 `sleep(50ms)` 做间隔（`room_service_tests_migrated` ×3、`api_msc3912_redaction_cascade_tests` ×1）| **已收口**：新增 `tests/common/mod.rs::wait_until(what, budget, condition)` —— 「等什么」与「等多久」是**必填参数**，超时文案统一成 `等待「{what}」超时（{budget:?}）：条件始终不成立`（原先四处各写一遍 `timeout(3s, loop { …; sleep(50ms) })` 或手搓 deadline，失败时读不出等的是谁、等了多久）。4 处调用点全部换掉；间隔从 50ms/20ms 混用统一成 25ms（对"等后台任务"没有区别）。**判据**：`sleep` 棘轮 `TEST_FILE 47 → 44 / 合计 107 → 104`，`scripts/ci/sleep_baseline` 已 `--update` 收紧（棘轮只许下降，收紧后复跑 = OK）；`test_bridge_e2e_send_message_delivers_real_room_event_payload` + `test_appservice_background_sender_flushes_pending_queue` + `api_msc3912_redaction_cascade_tests`(7) 共 **9 passed**（真库）。**变异自证**：把 `deadline` 改成 `Instant::now()`（立即到期）⇒ 上述两条代表性用例**都红**，且失败信息直接给出 `等待「appservice bridge 收到一条 transaction」超时（3s）`；还原后 9 passed |
| NEW-11 | `sync` 与滑动同步的 **state 组装层**（`lazy_load` / `required_state` / MSC4222 `state_after`）各写一遍 —— ARCH-02 只共享了 timeline 层 | **已收口（断言 3/3 就位 + 裁决不抽取）**：`tests/integration/sync_path_equivalence_tests.rs` 新增 ④ `full_state_is_identical_across_both_paths`（`/sync` 无过滤 vs 滑动同步 `required_state=[["*","*"]]`，`(type, state_key)` 集合逐项相同，建房 7 项状态）、⑤ `state_filter_and_required_state_agree`（`filter.room.state.types` 与 `[[type, state_key]]` 两种方言落到同一集合）、⑥ `state_after_is_a_rename_not_a_requery`（MSC4222 打开后只有 `state_after`、不再有 `state`，集合与关闭时相同，不稳定前缀同理）、⑦ `lazy_load_members_only_sends_what_the_client_needs`（双成员夹具：首屏必须带上对端 membership；裁剪到 `limited=true` 的增量默认**抑制**已发过的成员；`include_redundant_members=true` 时重建式分支把成员再发一遍）。**顺带修掉一处测试腐烂**：ARCH-02 的三条用例自 NEW-17（`2377043e5`）起一直是红的 —— 它们按扁平 `rooms[room_id]` 读，而 `rooms` 已分桶成 `{join, invite, leave}`；统一收口到 `rooms.join[…]`。**裁决：断言已守住等价，但本轮不做抽取。** 理由是可抽取面比 ARCH-02 小得多：两边的**过滤方言不同**（Matrix `StateFilter` vs MSC3576 `required_state` 对）、**数据源不同**（`/sync` 走 DB 批次、滑动同步走 `room_state_cache`），且 `/sync` 侧还多一层 `LazyLoadedMemberState` 记账（位置 + event id，抑制已发过的成员）。真正重合的只剩「state 事件 → JSON」（已是 `sync_helpers::state_event_to_json`，本就共用）。要抽出"同一份 state 组装"，前提是**先统一过滤方言**，那是 ARCH 级设计改动，收益（少几十行）低于风险（懒加载抑制逻辑搬错会让客户端永久丢昵称/头像）⇒ 记为决策，不再排期。**判据**：`sync_path_equivalence_tests` **7 passed**（真库）。**变异自证**：① `build_required_state_events` 恒回空集 ⇒ ④⑤ 红；② 忽略 `use_state_after` ⇒ ⑥ 红；③ 重建式分支忽略 `include_redundant_members` ⇒ ⑦ 红（`必须把成员再发一遍…：[]`）。 |
| NEW-09 | 只锁形状的测试文件里曾有 24 个端点族没有 HTTP 往返用例（`shape_only_test_matrix` 里写 `NONE`）| **已收口**：① **更正 6 族误判**（`room_summary`/`federation`/`admin`/`app_service`/`federation_cache`/`worker`—— 它们本来就有真请求文件，只是命名不在 C3 的旧视野内）、② **识别 3 族幽灵**（`admin_extra`/`coverage`/`formatting`，全仓无对应生产模块，矩阵格式表达不了"不存在"，已在 §0.1.1 记明）、③ **补测 11 族**（`push`/`event_report`/`reactions`/`friend`/`telemetry`/`server_notification`/`space`/`thread`、`media_quota`/`sliding_sync`/`refresh_token`、`captcha`），每族一笔提交、每笔把登记行从 `NONE` 换成真覆盖文件（C2 会验证该文件**确实**发请求）；④ **最后一块** `captcha` 的完整状态机（发码→查状态→错码→对码→复用→超限）本轮补齐：`seed_image_template` 往 `captcha_template` 种一条 `image` 默认模板（该类型的投递实现就是 `Ok(())`，不需要 SMTP/SMS），验证码从 `registration_captcha.code` 读、并断言它**不在**任何响应体里。**判据**：`api_captcha_routes_tests` **7 passed**（新增 `captcha_full_state_machine`、`captcha_exhausts_after_max_attempts`）；`check_shape_only_tests.py` 维持 `只锁形状 29 / 有真覆盖 26 / 已登记缺口 3`（3 个即幽灵族）。**变异自证**：把 `verify_captcha` 的 `captcha.code == code` 改成恒真 ⇒ 两条新用例**都红**（`错码必须 verified=false：{"verified":true}`），还原后 7 passed |
| NEW-14 | 「缺口清单不可信」：C3 只审计 `*_api_tests.rs` / `*_route_tests.rs`（15 个文件），而 79 个 `api_<family>_tests.rs` 不在视野内 ⇒ 缺口判定本身失真 | **扩面 + 逐族重跑**：`CLAIM_NAME` 覆盖仓里实际四种命名 ⇒ 审计面 15 → **95** 个文件、不碰生产代码 **0** 个（零误伤）；14 个 `NONE` 族按「路由模块 → 端点字面量 → 真请求文件」三步重跑 ⇒ **4 族误判**（`admin`/`app_service`/`federation_cache`/`worker`，已改登记行）、**3 族幽灵**（`admin_extra`/`coverage`/`formatting`）、**7 族真缺口**（转 NEW-09）。逐族证据见 §0.1.1 |
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
| SEC-09 | CAS 回调的 `redirect_after` 未做同源校验 ⇒ 开放重定向 | **真实缺陷，已修**（`e83fdbb6f`）。`cas_sso_redirect` 原先把 `redirect_after` 原样编码进 `/login?service=…`，登录页据此跳转，攻击者可用 `https://evil.example/steal` 或协议相对 `//evil.example` 借本域域名完成跳转。新增 `is_same_origin_path`：必须 `starts_with('/')`、**不是** `//host`、不含 `\`（浏览器会把 `/\host` 归一化成 `//host`）、不含 `< 0x20` 与 `0x7f`（可用来截断/逃逸）；非法值回 `400 M_INVALID_PARAM` 且**不回显**原值；空串等同未传。**判据**：`test_redirect_after_accepts_only_same_origin_paths` 覆盖接受侧（`/`、带 `?via=` 的房间路径、带 `#` 的客户端路由）与拒绝侧（`https://` / `http://` / `//host` / `/\host` / `javascript:` / 裸域名 / 空串 / 含 `\r\n` / 含 `\u{7f}`）；`cas::tests` **7 passed**（`--features test-utils,cas-sso`） |
| SEC-11 | 静态密钥封装存在全零密钥死代码 | **误报，不改代码**。`synapse-common/src/key_encryption.rs` 的 `let mut okm = [0u8; 32];` 是 **HKDF 的输出缓冲区**，下一行 `hk.expand(info, &mut okm)` 把 32 字节全部覆盖后才读取；`[0u8; 32]` 只是"先分配、后填充"的写法，不存在"用全零密钥工作"的路径。该处已带 `#[allow(clippy::expect_used, reason = …)]` 说明 fail-closed 意图。**没有改动**，本条从清单移除 |
| SEC-13 | 联邦鉴权 DEBUG 日志打印签名原文 | **真实缺陷，但原描述有偏差，已修**（`c2783c680`）。泄露的不是签名串，而是 `canonical_federation_request_bytes` 的**结果**（规范化请求体，含 room 消息与 to-device 载荷）——排障打开 DEBUG 就等于把明文落盘。改为字段化日志：`origin` / `len` / `fingerprint`（新增 `signature_fingerprint`，取 `compute_signature_content_hash` 的前 8 字符；该函数原先只在 `#[cfg(test)]` 下编译，本轮去掉 cfg 供生产日志使用）。**判据**：新增 `signature_fingerprint_is_short_and_deterministic` + 源码棘轮 `debug_logs_do_not_dump_signed_bytes`（用 `concat!` 拆针，避免门禁匹配到自身），`--features test-utils` **2 passed**；同批把 `ROUTE_CONTRACT.md` 的 5 处 `cas.rs` 行号引用刷新，`check_doc_line_refs.py` 漂移 57 → **55**（基线 56） |
| SEC-15 | 鉴权覆盖率约 94.4%，client/media 面存在未覆盖路由 | **已从"静态快照"改成"可重跑门禁"**。新增 `scripts/contract/route_auth_public.txt`（161 条，`METHOD PATH<TAB>kind<TAB>reason`，kind ∈ `public`/`federation`/`middleware`/`secret`/`manual`）+ `scripts/contract/check_route_auth.py`（双向：无提取器且未登记 ⇒ 红；登记了但路由已消失、或现已带提取器 ⇒ 也红），并作为 `preflight.sh` 的独立 step（不放进契约 gate 的 `if/else`，否则会被 SKIP 分支吞掉）。**判据**：门禁输出 `979 route(s): 818 declare an auth extractor, 161 declared (public 76 · federation 43 · middleware 8 · secret 14 · manual 20)` / `credential required by 903/979 = 92.2%; open 76 (7.8%)`；`--self-test` 16 例全过；`check_gate_reachability.py` 58 个 `check_*` 全部可达。**变异**：删掉 `GET /_matrix/client/v3/login` 那一行 ⇒ 红（`handler takes no auth extractor and the route is not declared`，实测），还原 ⇒ 绿 |
| NEW-08 | `synapse-federation/src/device_sync.rs`（735 行）自称"设备列表同步"，实际生产零调用，与 `routes/device.rs::broadcast_device_list_update` 重复实现同一件事 | **删除整模块**。`DeviceSyncManager::new` 全仓唯一构造点在它自己的 `#[cfg(test)]` 与 `tests/integration/federation_service_tests_migrated.rs`（2 条只测死代码的用例，已删）；同步移除 `synapse-federation/src/lib.rs`、`synapse-services/src/lib.rs`、`synapse-web/src/federation/mod.rs` 三处 re-export。棘轮同步：`scripts/ci/sqlx_dynamic_ratio_baseline` static 1357 → 1354（该模块 3 条生产静态查询）、test 动态 791 → 784、总 926 → 925，均就地写明"只因查询确实被删"；`file_size_baseline` / `clippy_allow_baseline` 删除过期文件条目；`TESTING.md` 与 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` 数字块跟改。守卫 `federation_device_list_writer_tests::the_dead_device_revocation_job_does_not_come_back` 改为"文件缺失即跳过"——重加回来仍会被扫描。**判据**：`cargo check --workspace --all-targets --all-features` 通过；`check_sqlx_dynamic_ratio.sh`、`check_file_size_ratchet.py`（该文件无 stale 条目）、`check_clippy_allow_ratchet.py`、`check_sqlx_cache_fresh.sh --static` 全绿。**变异**：把 `pub fn notify_device_revocation() {}` 写回同名文件 ⇒ 守卫红（实测）。真实广播路径 `broadcast_device_list_update` 未动 |
| PERF-10 | PostgreSQL 实例级未设 `statement_timeout`（直连 psql 的运维查询可无限挂起） | **已修**（`812661672`）。应用侧本就按连接兜底（`src/server/database.rs` 的 `after_connect` 对每个连接 `set_config` 三个超时，默认 30s/10s/60s；迁移器逐语句 `SET statement_timeout='30s'`），缺的只是实例级。`docker/config/postgres.conf` 新增小节：`statement_timeout = 5min`、`idle_in_transaction_session_timeout = 60s` —— **刻意不比应用更严**：应用连接会被会话级 `set_config` 覆盖，实例级只防直连；写成 10s 只会提前掐断运维手工 `CREATE INDEX` / `VACUUM`。门禁沿用既有 `scripts/ci/check_connection_budget.py`（不新增脚本，避免 `check_gate_reachability` 登记）：两个键必须显式存在（缺 ⇒ `exit 2`）、实例级 ≥ `synapse-common/src/config/database.rs` 的应用默认（违例 ⇒ `exit 1`）。`docker/deploy/README.md` 同名小节写明「应用侧已兜底，这里防的是直连」。**变异**：`5min → 10s` ⇒ `exit 1`；删键 ⇒ `exit 2`；还原 ⇒ 绿（三次实测）。**边界**：生产实例的 `postgresql.conf` 由部署方管理，本仓只覆盖 docker 形态 |
| SEC-12 | legacy r0/v1/r1 媒体下载端点匿名可读 | **裁决：不是漏洞，是 Matrix legacy 语义**（r0/v1 匿名可读；原描述把 **r1 也算进去了，实际 r1 早已挂 `auth_middleware`**——VULN-01/02，`api_security_vuln_fix_tests` 三条 401 用例是证据）。做法按审计原修法：**登记为设计决策 + 部署文档说明**，不改 `auth`。`docker/deploy/README.md` 新增「媒体下载的匿名读语义」小节，表格列全六类路径（r0/v1 匿名 · v3 匿名 · v3 `download_signed` HMAC · r1 必须 token · `/_matrix/client/v1/media` 必须 token）并写明"要关闭匿名读就得给 legacy router 挂 `auth_middleware`，且先按客户端版本分布裁决"。**判据**：`doc_credibility_guard_tests::legacy_media_anonymous_read_is_registered_and_matches_the_router` 四方绑定（部署文档小节存在且点名 r0/v1 + `auth_middleware` / r1 必须挂 / r0·v1 不得挂且必须 merge legacy router / `download_media_v1` 仍 `OptionalAuthenticatedUser`），用新 helper `fn_body` 按**函数体**切片，防止同文件里 r1 的 `auth_middleware` 让 r0 断言假通过；`doc_credibility_guard_tests` **16 passed**。**变异**：删 README 小节标题 ⇒ 红；给 `create_media_r0_router` 挂 `auth_middleware` ⇒ 红；去掉 r1 的 `auth_middleware` ⇒ 红（三条均实测，还原后 16 passed） |
| PERF-01 | `to_device_messages` 对普通设备无按龄清理（只有脱水设备路径有 `DELETE`），长期不活跃设备的积压永久累积 | **已修（W10）**。**语义裁决**：`claim_messages` 是 `WITH deleted AS (DELETE … RETURNING)`、`delete_messages_up_to` 按 `stream_id <= $3` 删 ⇒ **取走即删**，所以存活超窗的行只能是"设备从未回来取"的积压，不需要投递确认；代价显式接受——离线超窗设备丢积压（与 `DEVICE_LIST_STREAM_RETENTION_DAYS` 同等取舍），回来从 `/sync` 重拉。**落地**：`synapse-storage/src/pruning.rs` 的 `TO_DEVICE_MESSAGES_RETENTION_DAYS = 30` + `prune_old_to_device_messages`（复用 `PERF-06` 的 `ctid` 分批；`retention_days == 0` ⇒ `Ok(0)` 关闭，不是"删光"）、`RetentionConfig::to_device_retention_days`（默认 30）、`src/server/mod.rs` prune 循环第 11 步、`docker/config/homeserver.yaml` 显式写 30 并注明 0 为关闭；**不建 `created_ts` 索引**（选择性差，`ctid … LIMIT` 顺序扫描更划算）。**判据**：`prune_to_device_messages_respects_window_and_the_zero_day_off_switch`（真库；40 天前 2 条 + 1 天前 1 条 ⇒ 删 2 留 1 且留下 1 天前那条；`0` 天删 0 行且 3 行全在），`--lib pruning::` **17 passed**、`config::retention` 1 passed。**变异**：删零天 guard ⇒ 红（`left: 3, right: 0`）；`<` 改 `>` ⇒ 红（`left: 1, right: 2`），两次还原后均绿。`check_sqlx_dynamic_ratio.sh` 动态生产 +1（运行时 `sqlx::query()`，`SQLX_CACHE_ENTRIES` 不变），基线已同步并写明来源 |
| ARCH-05 | 7 个生产文件 >2000 行（`room/mod.rs` 2531、`membership/mod.rs` 2502、`refresh_token/mod.rs` 2478、`error.rs` 2442、`device/mod.rs` 2382、`config/mod.rs` 2324、`client.rs` 2086） | **已修（W11）**。**原判定错在"要证明拆分后行为不变"**：这 7 个文件超 2000 行的主因是**内联测试模块**（`error.rs` 生产段只到 1229 行，测试块 1251 行；`config/mod.rs` 1374 + 916；`room/mod.rs` 1532 + 967），把测试块外移**本身就是零行为变更**，模块路径仍是 `<父模块>::<name>`、`use super::*` 语义不变。**落地**：9 个内联测试模块外移到同级文件（`room/db_tests.rs`、`membership/{tests,db_tests}.rs`、`refresh_token/tests.rs`、`error/tests.rs`、`device/{tests,db_tests}.rs`、`config/tests.rs`、`client/tests.rs`），父文件只留 `#[cfg(test)] mod X;`（`#[allow(...)]` 留在声明上，lint 属性对模块内容同样生效）；拆后 7 个文件分别为 1557 / 1187 / 1258 / 1230 / 1373 / 1408 / 1448，**生产 `>2000` 行数 7 → 0**。**判据（可失败）**：`scripts/ci/file_size_baseline` 的 7 条逐条收紧（不是整表 `--update`）；给 `room/mod.rs` 追加 20 行 ⇒ 闸门报 `room/mod.rs: 1557 -> 1577`，删掉后该条目消失（两次实测）。编译：`cargo check` 覆盖 `synapse-common`/`synapse-federation`/`synapse-storage` 的 `--all-targets` 全绿零警告；测试：`error::tests` **78 passed**、`config::tests` **67 passed**、`room::db_tests` **44 passed**（真库）、`client::tests` **34 passed**、`membership::` **39**、`refresh_token::` **41**、`device::` **54**（后三条含真库 db 用例）。**连带**：外移出的 9 个测试文件里 7 个 >600 行按现状入册；并发现 `check_file_size_ratchet.py` 先前就在 HEAD 上红（67 条无关增长 + 2 个新增超限文件），登记为 **NEW-20**，未整表 `--update` |
| **CODE-11** | `#[allow(...)]` 存量 **157 处无 `// reason:`（94 文件）**，棘轮当时只防"新增无理由"，存量理由一直是空白 | **已修（W12，2026-10-26）**：棘轮现算 **159 site / 0 处无 `// reason:`（0 文件）**，基线 0/0（关闭前 157 处 / 94 文件）。**落地**：94 个文件插入 **129 条** `// reason:`（其中 28 个属性同时抑制 2–3 个 lint，故 129 个属性位覆盖 157 个 lint 计数），理由按 lint 类给出而不是模板字面量 —— `too_many_arguments` 分三种（依赖注入构造器 / 与 SQL bind 列序一一对应的存储方法 / 测试夹具逐列造行）、`unwrap_used+expect_used+panic` 全部落在 `#[cfg(test)]` 模块或 test-mocks（"夹具/断言失败即应 panic"）、`unused_async` 落在 axum handler（"签名约束，body 无 await 不等于能去 async"）、`type_complexity` 落在 `Arc<RwLock<…>>` 的 fake 状态（复合键 HashMap / Vec 队列）与多字段元组返回、`module_inception` 落在既有 `search/mod.rs` 与 `config/tests.rs` 布局。**顺带修掉棘轮的自相矛盾**：`main()` 原把**空基线**判 `exit 2`，而 0 处无理由正是它的目标态（禁止自己成功），改为只有**基线文件缺失**才 fail-closed，扫描器为空的保护（`files == 0 or not sites`）保留；`doc_credibility_guard_tests::clippy_allow_counts_in_the_audit_match_the_ratchet_baseline` 的 `assert!(files > 0)` 同步换成说明注释（0 文件是合法终态）。**判据（三条实测）**：① `--update` 后门禁输出 `159 site(s) in 828 file(s); 0 unjustified in 0 file(s) (baseline 0 in 0 file(s))` / `OK` / `exit 0`；② **变异 1**：删掉 `synapse-common/src/regex_cache.rs` 的一条 `// reason:` ⇒ `FAIL: 1 file(s) have unjustified clippy allow(s) not in the baseline` / `exit 1`，还原 ⇒ 绿；③ **变异 2**：临时移走 `scripts/ci/clippy_allow_baseline` ⇒ `FAIL: baseline … missing — regenerate with --update (fail-closed)` / `exit 2`，还原 ⇒ 绿。**零编译影响**：改动只有注释行（`git diff --numstat` = 94 files / +129 / −0），不触碰任何 `#[allow]` 的属性内容与代码；**行数棘轮**：其中 18 个文件恰好被这 1–3 行顶过各自 ceiling，已逐条上调到新值（其余 67 条既有超限与 2 个未入册文件一概不动，`grown` 计数保持 67 ⇒ 未把 NEW-20 做大）；**格式化棘轮**：HEAD 上本就是红的 9 处（ARCH-05 外移出的 `config/tests.rs` 与 `client/tests.rs` 首行空行、`test_mocks/event.rs` 的对齐组），本笔一并清到 0 —— `check_fmt_ratchet.sh` 报 `current=0 baseline=0`、`cargo fmt --all -- --check` 0 退出 |
| MAINT-01 | `synapse-common` 的 4 个测试基建模块（`test_database_guard` / `test_isolation` / `test_schema_guard` / `test_schema_orphans`）无条件编译，随每个生产二进制进入依赖图 | **已修（本提交）**：4 个模块改为 `#[cfg(any(test, feature = "test-utils"))]`。**为什么只能在测试侧打开**：依赖方的 `#[cfg(test)]` 对被依赖 crate 不可见，所以 `synapse-storage` / `synapse-e2ee` / `synapse-federation` / `synapse-services` / `synapse-web` 的**库内测试代码**看不到被关掉的模块 —— 只有把 `synapse-common/test-utils` 写进各自的 `[dev-dependencies]` 才成立；而 dev-dependency 的 feature 不并入非测试构建（resolver v2），生产依赖图因此保持干净。`synapse-test-utils` 是测试基建 crate 本身，走普通 `[dependencies]`。**判据**：`doc_credibility_guard_tests::test_infrastructure_is_feature_gated_and_never_ships_in_production` 四段断言（每个 `pub mod test_*;` 的紧邻上一行必须恰为 gate / 6 份消费者 manifest 的 `[dev-dependencies]` 必须含 `test-utils` 且 `[dependencies]` 必须不含 / `synapse-test-utils` 必须含 / 根 manifest 的 `synapse-test-utils` 必须 `optional` 且 `test-utils` feature 必须 `dep:synapse-test-utils`）；`cargo check --workspace --lib`（生产形态）与 `cargo check --workspace --all-targets --features test-utils`（规范测试形态）均 0 退出。**变异**：删掉 `test_isolation` 前的 gate ⇒ 红；去掉 `synapse-storage` dev 段的 `features = ["test-utils"]` ⇒ 红（两条均实测，还原后全绿） |
| **NEW-20** | `check_file_size_ratchet.py` **在 HEAD 上就是红的**：67 个已入册文件涨过各自 ceiling，另有 2 个新增 >600 行文件未入册（`media/tests.rs` 646、`url_preview.rs` 607） | **已关闭（W13，2026-10-27）。处理方式由人拍板为「整表 `--update` 一次性批准」**：不逐条回溯、不拆文件，67 条增长与 2 个新增文件**全部入册**，基线 220 → **222** 条。**归因备查**（说明这次批准不是盲批）：以 `6fa889a45`（2026-10-20 上一次全表重设）为钉版，用 `git log 6fa889a45..HEAD --numstat -- <file>` 对 67 条**逐文件**回溯，合计 **+4201 行**，全部落在 **33 个已提交批次**里、**零条无人认领**；贡献最大的六批是 `157a911d0` 池化隔离 +868、`1ced60cd5` RustCrypto 升级 +357、`3cf5c006a` PERF-06/13/14 +346、`5f4a35d5e` 跨进程 schema 复用 +323、`1c8e706a6` B-19~B-21 +316、`575a2d5b8` push rules 排序锚点 +314；同期另有 **19 个文件缩小 261 行**。② **两个新增文件入册，不拆**：`synapse-services/src/url_preview.rs` **607**、`synapse-services/src/media/tests.rs` **646** 各自成为 grandfathered 条目（自此合法，且**只许降**）。③ **重设只改数字**：67 条上调 + 2 条新增、19 条**下调**（−261），不是单向放宽；门禁输出 `825 files scanned, 222 over 600 (baseline 222)` / `OK` / `exit 0`。**判据（两条变异实测，还原后均绿）**：① 给 `synapse-services/src/admin_user_service.rs` 追加 1 行 ⇒ `FAIL: 1 grandfathered file(s) grew past their ceiling: … 965 -> 966`；② 新建 601 行的 `synapse-services/src/tmp_new_offender.rs` ⇒ `FAIL: 1 file(s) exceed 600 lines and are not grandfathered` ⇒ 闸门对"下一条新增长"仍然有效。**代价写在这里**：基线里 67 条新 ceiling 等于"承认这 6 天的增长"，2 个新增文件 607 / 646 自此合法；`--update` 只做这一次，之后仍只许降。此前试过的「归因 + 拆分」版本（`848a736ed`）已按拍板撤回。**未做的事**：ARCH-04（`events` 分区）仍等真实数据量，CODE-10（837 处静默忽略）仍是登记项且不设配额 |
