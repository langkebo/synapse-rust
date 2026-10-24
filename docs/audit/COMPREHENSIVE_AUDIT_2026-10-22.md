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
| 架构设计 | 0 | 0 | 4 | 1 | 5 |
| 代码质量 | 0 | 0 | 4 | 1 | 5 |
| 功能完整性 | 0 | 0 | 1 | 1 | 2 |
| 性能（含数据层） | 0 | 0 | 1 | 3 | 4 |
| 安全性 | 0 | 0 | 0 | 6 | 6 |
| 可维护性 | 0 | 0 | 2 | 1 | 3 |
| 测试覆盖 | 0 | 0 | 3 | 0 | 3 |
| 文档完整性 | 0 | 0 | 1 | 1 | 2 |
| 专项：冗余与过度开发 | 0 | 0 | 1 | 0 | 1 |
| **小计** | **0** | **0** | **17** | **14** | **31** |
| 第七轮新发现（仍开着） | 0 | 0 | 1 | 2 | 3 |
| **合计** | **0** | **0** | **18** | **16** | **34** |

**结论摘要**：**P0 = 0，P1 = 0**（原 9 条 P1 全部有实现 + 能失败的门禁，逐条见附录 D）。
剩下的 34 条里，**没有一条会造成数据丢失或越权**；18 条 P2 中真正"用户可见"的只有三类：

1. **契约/能力台账与实现不一致**（FUNC-01 的 GDPR 擦除未实现、FUNC-05 的联邦能力未登记）
   —— 表现是"客户端照文档调用却得到另一套行为"；
2. **测试面盲区**（TEST-02 只断言状态码、TEST-05 90 处 `sleep`、TEST-06 5 个 feature 门控整文件）
   —— 表现是"改坏了也绿"；
3. **长期结构债**（ARCH-02 双同步实现、ARCH-04 `events` 未分区、ARCH-05 7 个 >2000 行文件）
   —— 不修也不会立刻出错，但每加一个功能都在加利息。

**下一批该做什么**（按"能不能写出可失败的验证"筛，见 §0.2.1）：
`TEST-02` → `ARCH-06` → `TEST-05`。三条都能先写出红、再改绿；其余要么需要决策
（ARCH-02/04/05），要么只等外部条件（PERF-10 的实例级 PG 配置、DOC-03 的规则文档入库）。

## 0.1 第七轮新发现（2026-10-24，只列仍开着的）

> 本轮共发现 4 条。**NEW-01（联邦缩略图 `animated` 未透传 ⇒ JPEG 字节标成 `image/webp`）
> 已在本轮修掉**，判据与证据见[附录 D](#附录-d已关闭条目索引)，不在此重复。

| 编号 | 级别 | 一句话 | 证据 |
|---|---|---|---|
| **NEW-02** | P2 | 项目规则文档**不在版本控制里**（`.gitignore:143` 忽略 `.trae/`，仓内无受控副本）⇒ 引用它的 DOC-03 无法在仓库内修，规则类门禁也对每个开发者各看一棵树 | `git ls-files .trae` 为空；`grep -n "trae" .gitignore` → `:143:.trae/` |
| **NEW-03** | P3 | 依赖分裂组数已从 **38 → 32**（crate 名 54 → 47、同版本多来源 16 → 15），`DEPENDENCY_UPGRADE_TRACKER.md` 的基线与"处置进度"整表过期 | `cargo tree -d --workspace --locked` 按 crate 名聚合：47 / 32 / 15（口径见 §0.4） |
| **NEW-04** | P3 | 源码与配置里共 **30 个**不同的 `scripts/...` 悬挂引用；但其中多数是**相对路径**（相对 `docker/deploy/`）或**历史叙述**（"这里过去调用过 X，已删除"）⇒ 不能做"出现即要求存在"的朴素门禁 | 全仓扫描 `scripts/[\w./-]+\.(sh\|py\|mjs\|js)` 与 `os.path.exists` 求差；逐条看上下文后只有 1 类是真错（`scripts/quality/check_missing_docs_ratchet.sh`，8 处，已在本轮修掉） |

## 0.2 未完成任务清单

> 这份清单是**唯一权威**。一条算"已完成"当且仅当它出现在某次提交里并给出可复现证据；
> 不在本清单、也不在本轮提交里的编号，只可能出现在附录 D（已关闭索引）。

### 0.2.1 可先红后绿（下一批的候选，按性价比排序）

| 编号 | 级别 | 一句话 | 判据（怎么算修好） |
|---|---|---|---|
| **TEST-02** | P2 | 6 个测试文件只断言状态码，不断言响应体 | 每个文件至少一条用例断言 `errcode` 与关键字段；把被测 handler 的响应体字段改名 ⇒ 该用例必须红 |
| **ARCH-06** | P3 | 基线 SQL 仍有 **86** 处 `updated_ts BIGINT NOT NULL`（规则 §2.2 定义为可空） | 先定稿规则（二选一），再补一次性对齐迁移；`grep -c` 归零或规则改口 |
| **TEST-05** | P2 | `tokio::time::sleep` **90 处 / 43 文件**（原记 27/14），构成时序 flake 面 | 逐处分类"等真实事件 / 等固定窗口"，后者改条件轮询；分类表落文档 + 计数棘轮 |
| **SEC-14** | P3 | 联邦请求缺 `ts` 仅告警不拒绝 | 增加拒绝分支 + 一条"缺 ts ⇒ 401"的用例（先写用例证明当前是 200/放行） |

### 0.2.2 需要决策或外部条件

| 编号 | 级别 | 一句话 | 卡在哪 |
|---|---|---|---|
| **ARCH-02** | P2 | `sync` 与 `sliding_sync` 无共享语义基座 | 抽取会触及两条协议路径的公共形状，缺"两条路径可见事件集合一致"的等价断言作为安全网；先写断言才能动 |
| **ARCH-04** | P2 | `events` 未分区、`room_memberships` 无归档 | 需要真实数据量判断分区收益；本机无生产级数据，属**等外部条件** |
| **ARCH-05** | P2 | 7 个生产文件 >2000 行 | 拿不出可失败验证（拆完行为不变），按 §10.0 降级为**登记项**，不排期 |
| **NEW-02** | P2 | 项目规则文档未入库 | 需要决定"规则放哪"（`.trae/rules/` force-add 还是迁 `docs/PROJECT_RULES.md`）；在此之前 DOC-03 修了也不进仓库 |
| **FUNC-01** | P2 | `erase=true` 显式 501（GDPR 擦除未实现） | 是**刻意的 fail-closed**；要么实现"内容擦除"这一层，要么在台账登记为"支持注销、不支持 erase"（后者只差一句文档，可随手做） |
| **MAINT-04** | P2 | 能力台账手工维护，已出现多处失真 | 生成器要能反推"为什么没实现"，做不到；本轮只修了两行错账（FUNC-04），根因仍在 |
| **RD-27** | P2 | `to_device` 的 4 个存储方法零调用，其中一个名字与语义相反 | 删 `pub` 方法是破坏性变更（`synapse-e2ee` 是库 crate），需先确认无外部消费者 |

### 0.2.3 登记项（已知、明确不做或只等时机）

| 编号 | 级别 | 一句话 | 为什么不做 |
|---|---|---|---|
| **CODE-10** | P2 | 生产 **837 处** `let _ =` / `unwrap_or_default()`（238 文件） | 大部分是合法用法；设下降配额会诱导写 `unwrap()` 来"减少计数"（§10.10） |
| **CODE-11** | P3 | `#[allow(...)]` **271 处 / 117 文件**，其中 **160 处 / 96 文件**没有 `// reason:` | 棘轮已在工作（只许下降）；剩"补理由"是纯体力活，随触达文件顺手补 |
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
| **TEST-06** | P2 | 5 个整文件以 feature 门控，默认测试不参与 | 与 RD-14 的 feature 矩阵同源；矩阵已接入 CI（`--compile` 车道），但默认车道仍不跑它们 |
| **DOC-03** | P2 | 项目规则把 MSC3245 标为 "Room summary"（实为语音消息） | 规则文件不在仓库内（NEW-02），修了也不进版本控制 |
| **DOC-04** | P3 | 三份文档共 9 组行号漂移 | 修法是"改引用符号名而非行号"，与各文档下一轮触达时一并做 |
| **FUNC-05** | P3 | 联邦能力项有意不注册，未全部登记为设计决策 | RD-15/16 已登记 9 条，其余随能力台账重做（MAINT-04）一起 |
| **NEW-03** | P3 | 依赖分裂 38 → 32，跟踪文档整表过期 | 更新文档即可，但那一整张"根因族"表要重算才有意义 |
| **NEW-04** | P3 | 30 个悬挂 `scripts/...` 引用，多数是相对路径/历史叙述 | 朴素门禁会产出 30 条假红；要做先解决"引用上下文"分类 |

## 0.3 复核历史（压缩）

| 轮次 | 日期 | 基线 | 这轮干了什么 | 判定变化 |
|---|---|---|---|---|
| 初轮 | 10-22 | `d3d41618b` | 逐文件静态审查，出 104 条 | — |
| 二轮 | 10-22 | `bf15f173a` | 对 P1 逐条实测 | **4 条实质错误**被更正（PERF-01 主结论不成立、FUNC-01 一半错、FUNC-02 与 RD-19 自相矛盾、PERF-10 口径错），3 条是"把已实现当缺失" |
| 三轮 | 10-22 | `88554436b` | 只重跑数字 + 扩散扫描 | CODE-02 影响面 5 → **19**；新增 CODE-12/FUNC-06/PERF-13/PERF-14 |
| 四轮 | 10-23 | `d86d825fd` | 核 W0–W5 批次是否真落地 | 全部成立；**3 条数字被改错**（`#[ignore]` 11 被"更正"成 16/7 是对的）、`check_no_default_admin_credential.sh` 自匹配自身恒红 |
| 五轮 | 10-23 | `63f030154` | W6 收尾 + 文档数字脚本化 | 新增 `check_doc_metrics.py`；DOC-08 前提不成立 |
| 六轮 | 10-24 | `629110931` | 只重算会漂移的数字 | TEST-04 8 → 4、TEST-05 27/14 → 84/40、CODE-01 维持 🟡 |
| **七轮** | **10-24** | **`01345e38f`** | **把"已解决"整体移出文档 + 新增 4 条 + 收口 TEST-04/MAINT-05/CODE-09/FUNC-04** | TEST-04 收口（0 个死门禁）、MAINT-05 5 → **8 处**、FUNC-04 两行改 `TRUE`、新增 NEW-01…04 |

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
| 基线 SQL `updated_ts BIGINT` | **86**（可空 `created_ts` 2 处） | `grep -c` 基线 SQL |

> 漂移是正常的（代码在动）。**不要把本表当常量**：下一轮先跑上面这几条命令再引用。

---

## 一、架构设计

### ARCH-02（P2）`sync` 与 `sliding_sync` 无共享语义基座

- **位置**：`synapse-services/src/sync_service/mod.rs`、`synapse-services/src/sliding_sync_service/mod.rs`
- **事实**：全仓成对交叉比对**未发现复制粘贴的函数体** —— 两条路径各有独立实现。所以这条**不是**
  "重复代码"，而是**同一套语义（懒加载成员、时间线裁剪、`limited`、账户数据）没有共享基座**，
  每改一处语义就要在两个服务里各写一遍。
- **影响**：已经发生过语义漂移（limited timeline 重放成员变更只修了其中一条路径）。
- **修法**：先写两组**等价断言**（同一房间状态下两条路径的可见事件集合一致），用断言锁住语义，
  再抽 timeline/state 组装层。**没有断言就不要抽** —— 那是重写而不是重构。

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

### ARCH-06（P3）Schema 规范漂移

- **位置**：`migrations/00000000_unified_schema_v12.sql`
- **事实**：**86 处** `updated_ts BIGINT NOT NULL`；2 处可空 `created_ts`；3 处列名写作 `timestamp`。
  项目规则 §2.2 把 `updated_ts` 定义为 **NULLABLE**，与 86 处 NOT NULL 直接冲突。
- **修法**：**先修规则**（二选一定稿），再补一次性对齐迁移。规则没定稿就动 SQL 会在下一轮被改回去。

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

### FUNC-01（P2）`erase=true` 显式 501：GDPR 擦除未实现（注销本身可用）

- **位置**：`synapse-web/src/routes/account_compat.rs`（`erase: true` 在**任何副作用之前**返回 501）
- **事实**：`erase=false`（默认）走完整注销；只有 `erase: true` 被拒，且有注释写明
  "接受 `erase: true` 却只注销**比诚实拒绝更糟**"。这是**有意的 fail-closed**，不是能力缺失。
- **影响**：客户端若固定带 `{"erase": true}` 会得到 501 且**不会**被注销，用户侧表现为"注销失败"。
- **修法（二选一）**：① 实现"用户内容擦除"这一层（`AccountDeactivationService` 已有，缺的是内容侧）；
  ② 只在能力台账登记"支持注销、不支持 erase"，并让 501 的 message 指向该条目（**这条只差一句文档**）。

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

### TEST-02（P2）6 个测试文件只断言状态码，不断言响应体

- **位置**：`api_admin_target_guard_tests.rs`、`api_security_headers_tests.rs`、
  `api_worker_replication_auth_tests.rs`、`msc4108_rendezvous_route_tests.rs`、
  `api_route_ledger_tests.rs`、`api_federation_key_fetch_limits_tests.rs`
- **影响**：返回体结构/`errcode` 变更无法被发现（已经因此漏过一次 401 挑战体结构变更）。
- **修法**：至少断言 `errcode` 与关键字段；结构稳定的用 insta 快照。
- **为什么它排在下一批第一位**：这是本清单里**最容易写出"先红后绿"**的一条 ——
  改一个响应字段名，用例必须红。

### TEST-05（P2）`tokio::time::sleep` 90 处 / 43 文件，构成时序 flake 面

- **数字已抬升**：原记 27 处 / 14 文件 → 实测 **90 / 43**（§0.4 有命令）。
- **影响**：每一条 `sleep` 都是一个"机器快就过、慢就红"的潜在 flake；CI 上表现为随机失败，
  进而诱导 `--retries`（把真缺陷也一起吞掉）。
- **修法**：逐处分类成"等真实事件"（→ 条件轮询/oneshot）与"等固定窗口"（→ 保留但注明理由），
  分类表落文档 + 计数棘轮（只许下降）。

### TEST-06（P2）5 个整文件以 feature 门控，默认测试不参与

- **事实**：RD-14 的 feature 矩阵已接入 CI（`ci.yml` 静态层 + `--compile` 车道），所以这些文件
  **至少会被编译**；但默认 `--test unit` 车道仍不跑它们的用例。
- **修法**：二选一 —— ① 让矩阵车道跑 `all-extensions` 的 lib 测试；② 明确登记为"仅编译验证"。

---

## 八、文档完整性

### DOC-03（P2）项目规则把 MSC3245 标为 "Room summary"（实为语音消息）

- **位置**：`.trae/rules/project_rules.md:465`（原文 `| MSC3245 | Room summary | ✅ 已实现 |`）
- **卡点**：该文件**不在版本控制里**（NEW-02）—— 改了也不进仓库、CI 也看不到。
- **修法**：先解决 NEW-02（规则文档入库或迁 `docs/PROJECT_RULES.md`），再改这一行。

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

**顺序**：`TEST-02` → `ARCH-06` → `TEST-05`。

理由：前两条都能在一条提交内完成、都能先写出红；`ARCH-06` 需要先定规则（**这一步是决策，
不是编码**）；`TEST-05` 的 90 处分类是纯体力活但收益最长尾，放最后。

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
| ARCH-01 | Thread 双实现写影子表，数据对 `/sync` 不可见 | `3789583ef`（写真实 `m.thread` 事件，影子表降级为索引） |
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
