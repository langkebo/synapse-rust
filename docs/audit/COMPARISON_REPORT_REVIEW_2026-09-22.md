# 对比报告复核与优化方案（2026-09-22）

> **审查对象**：`docs/synapse-rust-vs-synapse-comparison.md` 工作树 v1.2 更新（未提交）
> **基线**：`HEAD 32fb4a30` + 工作树改动（本文档 + `.aspell.ignore.txt`）
> **复核方法**：所有结论附**可复现命令**或 `路径:行号`；凡未实测一律标 `[未验证]`，不用"需确认"充当结论。
> **口径**：遵循 `docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md` §0 —— "文档自身的 ✅/🔴 标记不可信"，
> 以命令输出为准。

---

## 0. 结论摘要

1. **更新方向正确**（对齐基准 v1.161.0、修正部分计数），但引入了 **1 处确定性 CI 红**和 **1 处伪造引用**，
   另有 10+ 处计数/事实错误（多数来自"引用旧文档而非实测"）。
2. 抽查 20+ 条 ✅ 声明后，**E2EE 完整性、MSC4140 完整性、MSC3912 归因**三项为实质性高估。
   其中最严重的是 **"v11 是默认房间版本" × "撤回事件仍用 v10 顶层 `redacts` 格式"** 的自相矛盾——
   这是协议互操作缺陷，报告却写成"MSC3912 待验证"。
3. **§12.5 是第二份未对账的 backlog**：与 `OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`
   和 `PROJECT_REMAINING_ISSUES_2026-09-14.md` 均无交叉引用，违反 AGENTS.md 铁律 2（同一职责只允许一份实现）
   与铁律 6 的精神；且把两个**实验性** MSC（4242/4512）列为"P0 阻断性"，同时漏掉上游 v1.157.2 安全公告、
   v12 房间创建、/events 端点方法漂移等更硬的缺口。
4. **本轮已修复**：拼写门禁（`.aspell.ignore.txt` +10 词，门禁已自证"红→绿"）；文档内数值、
   伪造引用、特性片段、静态链接等确定性错误；`§12.5` 重写为带证据分层的方案。
5. 完善后的优化方案见 **§7**：按"证据类别"分层（门禁/事实 → 协议正确性 → 功能补齐），
   每条给命令与验收判据，并显式映射到权威清单，避免再开第二源头。

---

## 1. 审查范围与证据来源

| 类别 | 来源 | 用途 | 时效性 |
|------|------|------|--------|
| 代码 | `git ls-files` / `Cargo.toml` / `Cargo.lock` / 各 crate `src/` | 计数、版本、实现核对 | 2026-09-22 实测 |
| 机器权威 | `docs/synapse-rust/ROUTE_CONTRACT.md` | 路由/模块计数唯一口径 | 2026-09-21 生成 |
| 语义权威 | `docs/synapse-rust/MSC_SEMANTICS.md` | MSC 编号语义唯一真相源（含"借用编号"） | 2026-09-14 |
| 人工权威 | `docs/synapse-rust/API_COVERAGE_REPORT.md` | 覆盖率分析 | ⚠️ 部分内容停在 2026-05-28，已过时 |
| backlog 权威 | `docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`、`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md` | 现存问题与批次 | 2026-09-15/14 |
| 上游 | element-hq/synapse `develop` `CHANGES.md`（1.157–1.161） | 对齐基准 | 1.161.0 = 2026-09-15 |

---

## 2. 更新引入的确定性缺陷（P0，可复现）

| 编号 | 缺陷 | 实测证据 | 状态 |
|------|------|----------|------|
| **P0-1** | **本次更新打破 docs-quality-gate（拼写）** | `bash scripts/check_doc_spelling.sh docs/synapse-rust-vs-synapse-comparison.md` → **exit 1**，未识别 10 词：aliyun / cancellable / clamav / dags / livekit / redactions / sharding / twilio / webhooks / websocket；对 HEAD 版本（v1.1）同命令 **exit 0** → 明确是本次更新引入的回归 | ✅ 已修：`.aspell.ignore.txt` 增补 10 词，复跑 exit 0。门禁已自证能红（修复前红、修复后绿） |
| **P0-2** | **伪造引用**："`docs/synapse-rust/api-reference.md`（656 端点, 48 模块）" | `ls docs/synapse-rust/` 只有 7 个 md，**不存在 api-reference.md**；全仓 `grep -rn '656'` 仅命中本文档自身；权威口径为 ROUTE_CONTRACT **1151 条 / 66 模块**、API_COVERAGE_REPORT **约 883 条** | ✅ 已修：§3.4 与 §6.3 改为引用 ROUTE_CONTRACT 并加警示 |
| **P0-3** | §5.3 代码片段 `default = ["server", ...]` 含**已删除特性** `server` | `Cargo.toml` H-6 注释："`server` feature removed — zero `#[cfg(feature = "server")]` gates"；实际 default 为 `["core-private-chat","widgets","external-services","beacons"]` | ✅ 已修 |
| **P0-4** | 计数类错误（见 §2.1 明细表） | 逐条命令实测 | ✅ 已修 |
| **P0-5** | **"sqlx 编译时 SQL 验证"高估** | `scripts/ci/sqlx_dynamic_ratio_baseline`：`BASELINE_STATIC=61` / `BASELINE_DYNAMIC=2147` → 静态占比 **≈2.8%**；其余为动态 SQL | ✅ 已修（改为"待收紧的债务"表述） |
| **P0-6** | **"静态链接 + musl target"无据** | `rust-toolchain.toml` 仅声明 `x86_64-unknown-linux-gnu`；全仓 `grep -rn musl`（排除 target/vendor）= 0；`docker/Dockerfile` 用 `distroless/cc-debian12` 且专设 `runtime-libs` 阶段**抽取动态库** → glibc 动态链接 | ✅ 已修（§9.2、§10.3、§12.3） |
| **P0-7** | 过期路径 `web/routes/e2ee/backup.rs` | 实际为 `synapse-web/src/routes/e2ee/backup.rs` | ✅ 已修 |
| **P0-8** | 文档自相矛盾：§6.2 "227 个测试文件" vs 其分层计数 58+120+3+4 = 185 | `find tests -name '*.rs'` = 230；分层为 106 / 113 / 3 / 5 | ✅ 已修 |

### 2.1 数值核对明细（旧 → 实测）

| 项目 | 文档 v1.2 | 实测（2026-09-22） | 命令 |
|------|-----------|--------------------|------|
| .rs 文件 / 行数 | 1034 / ~464,000 | **1020 / 439,130** | `git ls-files '*.rs' \| wc -l`；`git ls-files '*.rs' \| xargs wc -l` |
| workspace crate | "8 个独立 crate"（表内漏 `synapse-test-utils`） | **members=8 + 根 crate = 9 个编译单元** | `Cargo.toml [workspace] members` |
| `synapse-common` | 70 | **72** | `git ls-files 'synapse-common/*.rs' \| wc -l` |
| `synapse-federation` | 21 | **20** | 同上 |
| `synapse-web` | 144（与 routes 数混用） | **162（其中 `src/routes/` 144）** | 同上 |
| 根 crate `src/` | 197 | **38**（根包全部 .rs 含 tests/benches = 275） | `find src -name '*.rs' \| wc -l` |
| 测试文件 | 227 | **230**（unit 106 / integration 113 / e2e 3 / performance 5） | `find tests -name '*.rs' \| wc -l` |
| criterion 套件 | 4 | **5**（漏 `performance_pagination_benchmarks`） | `grep -c '\[\[bench\]\]' Cargo.toml` |
| 文档文件 | 196 | **221**（`docs/*.md` 147） | `find docs -type f \| wc -l` |
| Docker 文件 | 154 | **212**（`docker/`） | `find docker -type f \| wc -l` |
| 迁移文件 | "3 个统一 Schema 文件" | **1 个 SQL**（`00000000_unified_schema_v12.sql`） | `ls migrations/*.sql \| wc -l` |
| tokio | 1.49 | 要求 1.49，**实锁 1.53.1** | `Cargo.lock` |
| vodozemac | ">=0.10.0" | 要求 `>=0.10.0`，**实锁 0.11.0** | `Cargo.lock` |
| insta | 1.41 | 要求 1.41，**实锁 1.48.0** | `Cargo.lock` |
| quickcheck | "1.0 + arbitrary" | 实锁 1.1.0；**仅** `synapse-common/src/validation.rs` 使用 | `grep -rn quickcheck --include='*.rs'` |

> 计数类错误的共同根因：**引用仓库内旧文档/记忆中的数字，而非当场运行命令**。
> 这与 `OPTIMIZATION_EXECUTION_PLAN` §0 的"每条都带可复现命令"口径直接冲突。

---

## 3. 与仓库权威事实来源的冲突

| 冲突 | 本文档 | 权威来源 | 处理原则 |
|------|--------|----------|----------|
| 端点数 | 656 / 48 模块（**无源**） | ROUTE_CONTRACT.md：**1151 条注册路由 / 66 模块文件**；API_COVERAGE_REPORT：约 883 逻辑端点 | 一律引 ROUTE_CONTRACT + ledger 导出；人工文档须与之一致 |
| MSC 编号语义 | 直接断言 MSC 标题（4140/4512/4242/3912…） | `MSC_SEMANTICS.md` 是唯一真相源，且显式登记 **MSC4155 / MSC4204 / MSC3967 为"借用编号"** | 写任何编号前先查表；本仓"编号 ≠ 官方语义"是已发生事故 |
| MSC 覆盖面 | 表内 ~18 行 | 代码中实际出现的 MSC 标识 **≥40 个**，其中 `msc4108`(143) `msc4186`(24) `msc4262`(21) `msc2409`(12) `msc4502`(11) 等**文档未列** | 覆盖率表应按代码 `grep -oE 'msc[0-9]{4}'` 生成，不手工挑选 |
| 缺失端点清单 | 报告行文暗示 admin 举报端点缺失 | `synapse-web/src/routes/admin/report.rs:20-24` **已实现** `GET/DELETE /_synapse/admin/v1/rooms/{room_id}/reports[...]`；API_COVERAGE_REPORT:126-127 的"缺失"段落停在 2026-05-28 | 引人工文档时必须标注其时间戳并回代码核对 |

---

## 4. ✅ 声明的实测复核（本轮抽查）

> 判定口径：`TRUE` = 代码支撑且语义符合声明；`PARTIAL` = 部分成立/存在但语义不符；`FALSE` = 声明不成立。

| 文档声明 | 文档判定 | 实测判定 | 证据 |
|----------|----------|----------|------|
| E2EE 完整：SAS/QR/交叉签名/设备信任/备份/SSSS/泄漏检测 | ✅ 完整 | **PARTIAL→FALSE** | 见 §4.1 |
| MSC4140 可取消延迟事件"完整实现" | ✅ 已对齐 | **PARTIAL** | 见 §4.2 |
| MSC3912"部分实现，content.redacts 注入待确认" | ⚠️ 需验证 | **归因错误；存在更严重的真实缺陷** | 见 §4.3 |
| MSC4242 仅底层存储 | ❌ 缺失 | **TRUE（结论对，但注释不实）** | `dag.rs:200-205/231-234` 注释称被 `/send_join`、`/get_missing_events` 使用，实际 **0 调用点** |
| Content Scanner 仅"缺存储层" | ⚠️ 实现不完整 | **比文档更严重：整模块孤儿** | 见 §4.4 |
| SMS 仅 Aliyun + Noop 桩 | ✅ 部分实现 | **PARTIAL（漏报）** | 还存在通用 `HttpSmsProvider`（`sms_provider/mod.rs:48`），工厂支持 `"http"/"generic_http"`（`:124-158`） |
| privacy-ext 位于 `synapse-services/src/privacy.rs` | ✅ 已实现 | **PARTIAL（路径错）** | 该文件**不存在**；实为 `synapse-storage/src/privacy.rs`（985 行）+ `synapse-services/src/account_identity_service.rs:8-52` + `wiring/extensions.rs:51` |
| 事件报告完整 | ✅ 完整实现 | **TRUE，且文档引用的旧清单过时** | `event_report_service.rs` + `synapse-storage/src/event_report/`(1662 行) + admin 路由已挂载 |
| 房间举报速率限制"待核查" | ⚠️ 待核查 | **MISSING** | `report_room`（`synapse-web/src/routes/directory_reporting.rs:226-266`）无专职限流；全仓 `grep -rn rc_reports` = 0；限流配置只有 `default` + 路径表（`synapse-common/src/config/rate_limit.rs:31-80`），`docker/config/rate_limit.yaml` 无该路径 → 只吃通用默认桶 |
| Dehydrated Devices 完整 | ✅ 完整实现 | **PARTIAL：端点方法已漂移** | 仅注册 `POST`（`assembly.rs:216-217`，`derived_route_table_always.inc.rs:143`），`next_batch` 从 **body** 读取（`handlers/dehydrated_device.rs:121`）；上游 v1.157 #19896 已改为 **GET + query**，且允许 `next_batch=null` |
| Friends / Burn-after-read / Beacon / Server notifications / Rendezvous / Key rotation / Background updates | ✅ 完整实现 | **TRUE** | 逐条抽查有实现与迁移表（好友：`friend_room_service/` + `synapse-federation/src/friend/`；阅后即焚：`BURN_MAX_RETRY=5` + 死信；信标：配额/背压/缓存；通知：service 1076 行 + 3 表；rendezvous：6 路由 + 2 表；key rotation：1017 行 + 1157 行；background update：3 表） |

### 4.1 E2EE：文档的"✅ 完整"不成立

| 子项 | 实测 | 证据 |
|------|------|------|
| Megolm/Olm | **真实**（vodozemac `GroupSession/InboundGroupSession/Account/Session` + 加密 pickle） | `vodozemac_megolm.rs:35-36,140,189-192`；`olm/session.rs:8,44,102,115` |
| 交叉签名 | **真实**（master→self_signing→device 信任链验证） | `cross_signing/service.rs:354-383` |
| 设备信任 | **真实** | `device_trust/service.rs:84-433` |
| 密钥备份 | **真实** | `backup/service.rs:81-700`；`secure_backup/service.rs:26-241` |
| SSSS | **PARTIAL**：AES-256-GCM 实现，但 curve25519 路径**从密文自身派生 AES 密钥**（非 ECDH） | `ssss/service.rs:251` |
| **SAS** | **不符合规范**：`derive_sas` 是 `SHA256(shared_secret‖info)` 取前 6 字节，**不是** 规范要求的 HKDF-SHA256；`confirm_sas` 接受**任意非空 MAC** 并置 `Done` | `synapse-e2ee/src/verification/service.rs:82-93`、`:296-351` |
| **QR 验证** | **桩实现**：`device_ed25519_key` 与 `device_curve25519_key` 复用**同一公钥**，`signature` 为空串；`scan_qr_code` 不做签名校验/ECDH/rendezvous 绑定 | 同上 `:384-390`、`:406-430` |
| **泄漏检测** | **死代码**：`leak_detection` **未在 `synapse-e2ee/src/lib.rs` 声明**（从未编译）；启用后会因未导入的 `Utc::now()` 编译失败；`get_session_device_count` 恒返回 `Ok(1)`；`save_alert` 漏写 NOT NULL 列 | `lib.rs:19-103`（无该模块）、`leak_detection/service.rs:129,267-269,247-264` |

> **教训（写入下轮纪律）**：2026-09-18 那轮把"E2EE 部分实现"**证伪**为"完整"，
> 依据是"文件存在且行数不小"（`verification/service.rs` 898 行）。本轮证明：
> **"存在实现文件" ≠ "符合规范"**。判定 E2EE 必须看三类证据：① 规范算法对齐
> （如 SAS 必须 HKDF）、② 互操作测试（对 Element 客户端）、③ 模块真的被编译/装配。

### 4.2 MSC4140：客户端链路真实，联邦链路缺失

- **真实**：存储 `synapse-storage/src/delayed_events.rs:107-287`；服务 `delayed_event_service.rs:28-79`
  （所有权 fail-closed 返回 `M_NOT_FOUND`，`cancel/restart/send` 三动作）；调度器 `src/server/mod.rs:745-843`
  （轮询 `get_due_events` + 分布式锁 + `mark_sent`）；路由与 ledger 均登记（`derived_route_table_always.inc.rs:183`）。
- **缺失**：`EduType`（`synapse-federation/src/edu.rs:15-35`）无 delayed-event 变体，
  全仓 `m.delayed_event` **0 命中** → **无 EDU/联邦同步**；且 schedule 路径 `state_key` 硬编码 `None`
  （`handlers/room/events.rs:339`）。
- 结论：可声明"单机 MSC4140 可用"，**不可**声明"完整实现/已对齐"。

### 4.3 撤回（redaction）：文档归因错误，真实缺陷更硬

文档把风险写成"MSC3912 关系性撤回待验证"。实测：

1. **v11+ 格式从未生成**：撤回事件 `content` 只有 `{"reason": ...}`，目标 event id 仍走**顶层** `redacts`
   （`synapse-web/src/routes/handlers/room/events.rs:959-982`）；出站 PDU 也只写顶层
   （`synapse-services/src/room/messaging/service.rs:173-175`）。只有**读路径**
   `synapse-common/src/redaction.rs:132-139` 同时兼容两处。
2. **自相矛盾**：`room_versions.rs:89` 定义 `DEFAULT_ROOM_VERSION = "11"`，`:113` 令 v11 `can_create=true`；
   而 create 路径注释（`events.rs:959-963`）称"v11+ creation is disabled"——该注释已过期。
   结果是：**本服务端默认创建 v11 房间，却按 v1–v10 格式生成撤回事件**。
   按规范，v11 消费方应从 `content.redacts` 读取，故本服务端发出的撤回在合规 v11 实现上可能不生效。
3. **关系性撤回（MSC3912 级联）未实现**：客户端撤回只处理单个事件（`events.rs:992`）；
   `relations_service.rs:362-381` 仅撤回单条 reaction 记录，无"按 rel_type 级联"。
4. 代码中 **`MSC3912` 标识 0 命中**；v11 撤回格式在代码里以 MSC2174/MSC3820 命名。

### 4.4 Content Scanner：不是"缺存储层"，是"整模块孤儿"

- `synapse-services/src/content_scanner/{service.rs 518 行, models.rs 316 行}` 存在，但：
  - `synapse-storage/src/` **无**任何 scanner 存储模块；`migrations/00000000_unified_schema_v12.sql` **无** scan 相关表；
  - 目录外唯一引用是 `synapse-services/src/lib.rs:78`（`pub mod`）与 `media/mod.rs:10`（re-export）；
  - `ContentScanner` **在生产路径从未被构造**（仅其自身测试引用）；`ContentScannerConfig` 未接入
    `synapse-common/src/config`；`docker/config/` 无相关键。
- 因此"扫描策略重启后丢失"的表述不准确：**根本不存在运行时策略对象**。正确表述是
  "模块未装配（orphaned），既无持久化也无接线"。

### 4.5 生产死代码（文档完全未覆盖）

`#[allow(dead_code)]` 生产代码共 **11 处**（`todo!()`/`unimplemented!()`/`FIXME` 均为 0，与 clippy `-D warnings` 一致）。
最值得处理的三条：

| 位置 | 问题 | 关联铁律 |
|------|------|----------|
| `synapse-services/src/oidc_service.rs:673` | 整个 `validate_id_token_claims` 方法**从未被调用**（安全相关，OIDC 声明校验缺席） | 安全 |
| `synapse-services/src/friend_room_service/models.rs:338` | 注释直书 "Reserved fields for future use" | AGENTS.md 铁律 1（禁止"先留着以后可能有人用"） |
| `synapse-services/src/admin_security_service.rs:19`、`admin_registration_service.rs:71` | 字段仅"constructor parity"保留 | 铁律 1 |

---

### 4.6 v1.161.0 "待核查" 8 项的实测结论

上游条目已对 `release-v1.161` 的 `CHANGES.md` 原文核对（<https://raw.githubusercontent.com/element-hq/synapse/release-v1.161/CHANGES.md>）。
其中 **1 行是文档自己的 ✅ 误判**（#20169），**2 行是真实缺失**（#20036、#20180）。

| 上游条目 | 文档行 | 实测判定 | 证据与说明 |
|----------|--------|----------|------------|
| #20148 DB 宕机事件持久化 | ⚠️ 未验证 | **PARTIAL** | 上游根因是"**每实例 state-group persisted 标记过期**导致重启前新事件无法持久化"——该机制在本仓**结构性不存在**（`synapse-storage/src/state_groups.rs` 无此标记；`migrations/...v12.sql:2388-2422` 无相关列），故该具体 bug **N/A**。但本仓有同类风险：`create_event_with_graph` 在 `tx=None` 时**先 INSERT events 再在事务外 INSERT event_edges**（`synapse-storage/src/event/create.rs:112-142`，注释直书 "Populate event_edges outside a transaction"），联邦入库/补洞/backfill 路径使用（`federation/transaction.rs:456,507`、`room/backfill.rs:297`）→ 存在**半写窗口** |
| #20119 `event_search` 跳过 `m.room.topic` | ⚠️ 未验证 | **PARTIAL** | 重建索引帮手**确实包含** `m.room.topic`（`synapse-storage/src/search_index.rs:238`），但该存储是**死代码**（模块外无调用者，仅 `sync/mod.rs:11` re-export）；**在线**查询与部分 GIN 索引硬限定 `event_type='m.room.message'`（`event/search.rs:170,210,311` 与 `:249-255`）→ 房间主题默认**搜不到**，除非客户端显式传 `filter.types` |
| #20169 `/sync` 左房成员泄漏 | ✅ 已处理 | **❌ 文档误判（✅ 无依据）** | 上游修复针对 **MSC4222 `state_after`**，而全仓 `grep state_after\|MSC4222` = **0**。文档引用的 `include_redundant_members`（`sync_service/filter.rs:110`、`lazy_load.rs:87-182`）是**另一个功能**，不构成该修复的证据。左房成员态取自**当前** state（`sync_service/data_fetch.rs:156-184`），同类泄漏在 `state` 上结构性可能存在 |
| #20149/#20172 Profile 500 | ⚠️ 未验证 | **PARTIAL** | (c) 不存在用户 → 404 已对（`handlers/extended_profile.rs:29-37`）；(a) account_data 非 JSON 对象 → 仍 500（`:47-50,78,101`）；(b) **与上游相反**：`user_exists` 过滤 `is_deactivated = FALSE`（`user/storage.rs:663-673`），故对**已停用但存在**用户写自定义字段返回 **404**，而上游 #20172 要求**成功**。另：稳定的 `/_matrix/client/v3/profile/{userId}/{keyName}` 未注册（只有 `uk.tcpip.msc4133` 不稳定路径，`assembly.rs:224-231`） |
| #20173 Profile PUT/DELETE 400→403 | ⚠️ 未验证 | **FIXED-ALREADY（另一用户场景）** | `account_compat.rs:195-197,224-226`、`extended_profile.rs:121-123,153-155` 返回 403 + `M_FORBIDDEN`（`error.rs:232-234`）；上游触发条件（`enable_set_displayname`/`enable_set_avatar_url` 关闭时）在本仓**不存在该配置** → 配置门控变体 N/A |
| #20036 `rc_reports` 限流 | ⚠️ 未验证 | **MISSING** | 见 §4 表；举报端点只吃通用默认桶 |
| #20180 `M_APPSERVICE_LOGIN_UNSUPPORTED` | ⚠️ 未验证 | **MISSING（根因是没有 appservice 登录）** | 全仓该字符串**只出现在本文档**；无 `m.login.application_service`、无 `MSC4190`；登录类型仅 password/token/sso/cas/oidc/dummy（`auth_compat.rs:260,291-316,413-455`）。**不是"未稳定化"，而是功能整体缺失** |
| #20146 LiveKit SFU WebSocket URL | ⚠️ 需确认 | **PARTIAL（死配置）** | `livekit_service_url` 全仓不存在（无可弃用对象）；`LivekitConfig.ws_url` 存在（`synapse-common/src/config/voip.rs:92`）但**从未被读取**（全仓仅声明一处）；`rtc/transports` 只返回 ICE（`handlers/rtc_transports.rs:16-56`）；配置节名为 `livekit:` 且顶层 `deny_unknown_fields`（`config/mod.rs:126,171-174`），Synapse 风格 `matrix_rtc:` 键会直接导致加载失败 |

> **结论**：文档 8 行里 7 行写"待核查"，本轮**全部当场判定**；其中 1 行（#20169）
> 把"实现了另一个功能"当成了"上游修复已对齐"。这正是"✅ 无证据"的典型代价。

## 5. 对齐基准遗漏（上游 1.157–1.161）

文档只把 v1.161 的 8 个 bugfix 列成"待核查"，遗漏了同期更重要的内容。以下均在
`element-hq/synapse` `develop` `CHANGES.md` 中可查（1.161.0 = 2026-09-15）：

| 上游条目 | 类型 | 文档是否覆盖 | 说明 |
|----------|------|--------------|------|
| **1.157.2 安全版本**（6 High + 4 Moderate + 2 Low，ELEMENTSEC/GHSA） | 安全 | ❌ **完全未提** | 一份含 §7 安全对比的报告不应对上游安全版本沉默；应逐条判定本仓是否同类受影响 |
| 1.158 默认房间版本改 **11**（MSC4239） | 协议 | ❌ 未提 | 见下 |
| 1.158 v12 房间修复（第三方邀请、并发建房 ID 冲突） | 协议 | ❌ 未提 | 见下 |
| MSC4326 appservice 设备伪装稳定化 | 协议 | ❌ 未提 | 本仓 `msc4326` 0 命中 |
| MSC2409 appservice 短暂事件 | 协议 | ❌ 未提 | 本仓 `msc2409` 有 12 处引用 |
| MSC4186 简化滑动同步 | 协议 | ❌ 未提 | 本仓 `msc4186` 有 24 处引用 |
| MSC4502 房间成员查询（1.160） | 协议 | ❌ 未提 | 本仓 `msc4502` 有 11 处引用 |
| MSC4262 / MSC4429 Profile 更新进 sync（1.159/1.160） | 协议 | ❌ 未提 | 本仓 `msc4262` 有 21 处引用 |
| #20189 联邦 `make_*` 请求缺少 `membership` 校验 | **安全修复** | ❌ 未提 | 见 §7 B2 |
| #20114 appservice 越界私有已读回执泄漏 | 安全修复 | ❌ 未提 | 待判定 |
| #20050 / #20101 / #20104 / #20106 / #20132 / #20152 / #20163 / #20192 | 修复/清理 | ❌ 未提 | 逐条判定，不必全做但需记录 |

### 5.1 房间版本：比 MSC4242 更硬的缺口

| 事实 | 证据 |
|------|------|
| 本仓 `DEFAULT_ROOM_VERSION = "11"`，v11 `can_create=true` | `synapse-common/src/room_versions.rs:89,113` |
| v12 / v13 为 `stable_parse_only`（可 parse/join/federate，**不可创建**） | 同上 `:58-67,114-115` |
| 上游 v1.158 起默认房间版本也是 11（MSC4239），即"刻意与 Synapse 不同"的前提已消失 | CHANGES.md 1.158.0rc1 |
| `room_versions.rs:77-81` 的注释仍称"Synapse 默认仍是 10，本项目刻意不同" —— **已过时** | 同上 |
| 后果：本仓无法创建 v12 房间、v12 认证规则未实现，而联邦中存在 v12 房间 | CHANGES 1.158 多条 v12 修复 |

文档把"未来房间版本升级"风险系于 MSC4242，实际更直接的风险是 **v12 不可创建 + v11 撤回格式错配**（§4.3）。

---

## 6. 方法论问题（比单条错误更值得修）

| 问题 | 表现 | 要求 |
|------|------|------|
| **用"待核查"代替核查** | v1.161 表 8 行中 7 行写"未验证/需确认"；而本轮实测其中多条当场可判定 | 每条必须给命令与输出；查不动的写 `[未验证]` 并说明阻塞原因 |
| **✅ 无证据** | §11 表格大量 ✅ 无 `path:line` 或测试名 | 沿用 `OPTIMIZATION_EXECUTION_PLAN` 口径："文档自身标记不可信" |
| **把预期当数据** | §4/§10 用"预期 50-100MB""预期 5-10x 节省"与 Synapse 生产实测并列 | 未实测项独立成表并标注"未实测"，不与上游实测同表比较 |
| **基准纪律缺失** | 未记录上游 tag/commit 链接 | 记录 `对齐基准: v1.161.0 (2026-09-15)` + CHANGES 链接 + 复核日期 |
| **双源 backlog** | §12.5 与两份权威清单无交叉引用 | 对比报告只做"差异描述 + 指向权威清单"，不新开来源 |

---

## 7. 完善后的优化方案

> 设计原则：**不做人日估算**（无依据的估算会掩盖不确定性）；每条给"现象 → 证据 → 动作 → 验收判据"；
> 并标注它属于"新增 backlog"还是"已有权威条目"。

### A 类：门禁与事实（本轮已完成）

| 编号 | 动作 | 验收判据 | 状态 |
|------|------|----------|------|
| A1 | 修复 docs-quality-gate 拼写回归 | `bash scripts/check_doc_spelling.sh docs/synapse-rust-vs-synapse-comparison.md` → exit 0；且修复前必须 exit 1（已实测） | ✅ |
| A2 | 删除伪造引用、改挂 ROUTE_CONTRACT | 文档内不再出现 656 / api-reference.md | ✅ |
| A3 | 修正全部计数与版本为实测值 | §2.1 每行命令可复现 | ✅ |
| A4 | 修正 sqlx/静态链接/特性片段等事实错误 | 同 §2 | ✅ |
| A5 | §12.5 重写为"差异描述 + 指向权威清单" | 与 `OPTIMIZATION_EXECUTION_PLAN` 无冲突项 | ✅ |
| A6 | 建立"文档数字必须来自命令"的守卫（建议） | 见 D1 | 待决 |
| A7 | **修复 Phase 1 代码项**：B1（v11 撤回格式）、B8（事件图半写）、B10a（`origin_server_ts` 吞错）、B5（过期注释） | 每项均由"修复前失败 → 修复后通过"的测试覆盖，并对纯函数做变异探针；提交 `64ca3345`/`d20164d6`/`a963fad0`/`a1b6cca8`/`f6913602`/`71cb5f91`；门禁见本文件 §10 | ✅ |

### B 类：协议正确性（本轮实测发现，建议优先于 MSC4242/4512）

> **Phase 1 已修（2026-09-22）**：**B1**（v11 撤回格式，含 PDU 侧）、**B5**（过期注释）、**B8**（半写窗口）、**B10a**（`messages.rs:32` 的 `origin_server_ts` 吞错）。
> 仍待处理：B10 其余两处（`federation/transaction.rs:358-362`、`membership/federation.rs:191-216,251-268`）、B2/B3/B4/B6/B7/B9/B11/B12/B13/B14。
>
> **Phase 2 增量（2026-09-22，全部完成）**：**B10b**（gap-fill 查重吞错）、**B11**（默认搜索面纳入 `m.room.name`/`m.room.topic`）、**B3**（举报端点 per-user `rc_reports` 限流）、**B9**（去重标记失败 soft-fail 已提交事件）、**B10c**（入站联邦加入/离开 fail-closed 且先事件后成员）、**B4**（Dehydrated `/events` POST→GET + 可空 `next_batch`，含全部契约产物再生成）、**B13**（`m.login.application_service`）。证据见 §12。

| 编号 | 现象 | 证据 | 动作 | 验收判据 | 关联 |
|------|------|------|------|----------|------|
| **B1** | **v11 默认房间版本 × v10 撤回格式** | `room_versions.rs:89,113` vs `events.rs:959-982`、`messaging/service.rs:173-175`；`redaction.rs:132-139` 只读兼容 | 让撤回创建按房间版本分支：`room_version > 10` 时把目标写入 `content.redacts`（顶层字段可保留给旧版本）；同时修正 `events.rs:959-963` 过期注释 | 新增测试：v11 房间撤回事件的 `content.redacts` == 目标 id；v10 房间仍在顶层；对 Element 客户端互操作冒烟通过 | 新增（比较报告首次指出） |
| **B2** | 联邦 `make_*` / `send_*` 是否校验 `membership` 字段（上游 #20189 安全修复） | 实测：**`send_*` 路径已校验**——`federation/membership/mod.rs:128-137` 用 `expected_membership` 比对 `content.membership`，不一致即 400；`make_join` 为无 body 的 GET（`membership/join.rs:34-91`），本实现不存在同一注入点。上游 #20189 的确切作用面需按上游 diff 复核 | 按上游 diff 复核 `make_knock`/`make_leave` 是否同源；若同源则补测覆盖 | 测试：`send_leave` 携带 `membership=join` 必须 400；`send_join` 携带 `membership=leave` 必须 400 | 上游 v1.161 #20189（完整作用面 `[未验证]`） |
| **B3** | `rc_reports` 房间举报端点限流**缺失**（实测） | `directory_reporting.rs:226-266` 无专职限流；`config/rate_limit.rs:31-80` 只有 `default`+路径表；`docker/config/rate_limit.yaml` 无该路径；`grep -rn rc_reports` = 0 | 按规范补专职限流桶（客户端举报端点同理） | 命中限流返回 429 且含 `retry-after`；配置项有测试覆盖 | 上游 v1.161 #20036 / MISSING |
| **B4** | Dehydrated device `/events` 端点方法漂移 | `assembly.rs:216-217`（仅 POST，body 游标）vs 上游 #19896（GET + query + `next_batch` 可空） | 改为 GET（保留 POST 会构成兼容残留，按铁律 1 直接替换）；`next_batch` 无更多事件时返回 null | 契约测试断言方法为 GET，且末页 `next_batch` 为 null | 上游 v1.157 #19896 |
| **B5** | `room_versions.rs:77-81` 过时注释 | 上游 1.158 默认已改 11 | 更新注释与"刻意分歧"表述；同步 `DEFAULT_ROOM_VERSION` 单测注释 | 注释不再断言"Synapse 默认 10" | 上游 v1.158 |
| **B6** | v12/v13 只读支持 | `room_versions.rs:114-115` | 决策项：实现 v12 认证规则并放开创建，或明确记录"只 join/federate"的支持边界（写入版本能力文档） | 决策记录 + capabilities 输出与之一致 | 上游 1.158 |
| **B7** | v1.157.2 安全公告同类性判定 | 上游 CHANGES 1.157.2（6 High / 4 Moderate / 2 Low） | 逐条判定本仓是否共享同类缺陷，产出对照表 | 每条给出"受影响/不受影响 + 证据" | 新增 |
| **B8** | **事件入库存在半写窗口** | `synapse-storage/src/event/create.rs:112-142`：`tx=None` 时先 INSERT `events`，再在**事务外** INSERT `event_edges`（注释自述 "outside a transaction"）；调用方 `federation/transaction.rs:456,507`、`room/backfill.rs:297` | 把两条写入并入同一事务；或明确注释为"可容忍"并给出补偿/修复查询 | 注入 `event_edges` 失败后库内不存在孤立 `events` 行 | 新增（文档未覆盖） |
| **B9** | **事务去重标记在事件事务之外 → 重试产生重复事件** | `room/messaging/messages.rs:288-305`：`send_message` 已提交事件后才 `record_event_txn`；标记写失败返回 500，客户端重试时去重表无记录 | 将 `record_event_txn` 纳入事件写入同一事务，或改为"标记先行 + 幂等键回填" | 测试：注入标记写失败后重试，房间内仍只有 1 条事件 | 新增（文档未覆盖） |
| **B10** | 生产路径吞 DB 错误（违反已知坑清单） | 本轮逐一复核：`room/messaging/messages.rs:32` 对 **DB 查询** `unwrap_or(0)`；`federation/transaction.rs:358-362` `.ok().flatten()`；`membership/federation.rs:191-216` 与 `:251-268` 持久化失败仅 `warn!` 后丢弃（注释自述 "Best-effort persistence — skip on error to avoid blocking the join"）。注：`transaction.rs:176/311/429/441` 的 `unwrap_or(0)` 作用于 **JSON 字段**，不属同类 | 逐点改为错误传播或 fail-closed；确认是否安全相关（去重/前驱/懒加载属于安全相关） | 每个点有测试证明失败时返回错误而非静默继续 | CLAUDE.md §踩过的坑（unwrap_or_default 吞错） |
| **B11** | 搜索索引：死存储 + 在线路径排除非消息事件（#20119 的真正修法） | `search_index.rs:228-264` 含 `m.room.topic` 但**无调用者**（仅 `sync/mod.rs:11` re-export）；在线查询硬限定 `m.room.message`（`event/search.rs:170,210,311`），部分 GIN 索引同（`:249-255`） | 决策：接线 `search_index` 存储或删除；确定索引事件类型集合（message/name/topic）并统一到一处 | 重建索引后 `m.room.topic` 可被默认搜索命中（或明确声明不支持） | 上游 v1.161 #20119 |
| **B12** | Profile 与上游语义不一致 | 已停用但存在用户写自定义字段返回 404（`user/storage.rs:663-673` + `extended_profile.rs:95,119`），上游 #20172 要求成功；稳定 `/_matrix/client/v3/profile/{userId}/{keyName}` 未注册（仅 `uk.tcpip.msc4133`，`assembly.rs:224-231`）；account_data 非 JSON 对象时 500（`extended_profile.rs:47-50`） | 分开"存在性"与"是否停用"判定；注册稳定路由（或记录为不支持）；非对象 ⇒ 400 而非 500 | 三条各有测试断言状态码 | 上游 v1.161 #20149/#20172 |
| **B13** | **App Service 登录整体缺失** | 全仓无 `m.login.application_service`、无 `MSC4190`、无 `M_APPSERVICE_LOGIN_UNSUPPORTED`；登录类型仅 password/token/sso/cas/oidc/dummy（`auth_compat.rs:260,291-316,413-455`） | 决策项：实现 AS 登录（含稳定错误码）或明确声明不支持并从对比表移除 | 决策记录；若实现则 `POST /login {type: m.login.application_service}` 有契约测试 | 上游 v1.161 #20180 / MISSING |
| **B14** | LiveKit `ws_url` 死配置 | `synapse-common/src/config/voip.rs:92` 声明 `ws_url`，全仓**无读取点**；`rtc/transports` 不返回 LiveKit 传输（`rtc_transports.rs:16-56`）；`deny_unknown_fields` 使 Synapse 风格 `matrix_rtc:` 键直接加载失败 | 接线 `ws_url` 到 transports 响应，或按铁律 1 删除该死字段并明确 RTC 支持边界 | 配置生效有测试；文档不再暗示具备 SFU URL 能力 | 上游 v1.161 #20146 |

### C 类：功能补齐与收敛（保留文档原有方向，但重新定级）

| 编号 | 项 | 原文档定级 | 建议定级 | 理由 | 验收判据 |
|------|----|-----------|----------|------|----------|
| C1 | E2EE SAS 规范对齐（HKDF + 真实 MAC 校验） | 未列（被判 ✅ 完整） | **高** | 影响与 Element 客户端的验证互操作；`confirm_sas` 接受任意 MAC 是安全弱化 | ✅ **已修（Phase 3）**：HKDF-SHA256 已知答案向量；篡改 MAC 拒绝；缺证据 fail-closed |
| C2 | E2EE QR 验证实现或标注为未实现 | 未列 | **高（或明确降级声明）** | 当前为桩（复用公钥 + 空签名），文档称"完整"属误报 | ✅ **已修（Phase 3）**：服务端无私钥无法签名 ⇒ 两个方法显式返回 M_UNRECOGNIZED 且不写状态；文档改为"不支持" |
| C3 | `leak_detection` 处置 | "泄漏检测全部实现" | **高** | 未编译模块 + 编译错误 + 桩计数 + schema 列缺失；文档称已实现 | ✅ **已修（Phase 3）**：按铁律 1 删除整目录（从未编译、零引用）；文档声明该能力不存在 |
| C4 | `ContentScanner` 装配或删除 | P0"缺存储层" | **中（决策项）** | 现状是孤儿模块；补存储的前提是先决定是否上线该功能 | 决策记录；若上线则补 schema + config + 构造点 + 测试；否则删模块 |
| C5 | MSC4140 联邦（EDU）支持 | "✅ 已对齐" | **中** | 客户端链路真实，缺联邦；需按 MSC4140 草案确认是否必须 | 明确"是否支持跨服务器延迟事件"的声明与测试 |
| C6 | MSC4242 State DAG | P0 阻断性 | **低（观察项）** | 上游本身是 experimental + "storage functions for future work"；无房间版本启用；`dag.rs` 注释需修正为"预留" | 修正不实注释；跟踪上游房间版本进展 |
| C7 | MSC4512 App Service 代理 | P0 阻断性 | **低（观察项）** | 上游为 experimental、opt-in | 同上 |
| C8 | SMS 提供商多元化 | P1 | **低** | 已有 `SmsProvider` trait + 通用 HTTP provider，Twilio 属可选 | 新提供商仅需实现 trait + 工厂分支 |
| C9 | OIDC `validate_id_token_claims` 接线 | 未列 | **高（安全）** | 死代码，声明校验未生效 | ✅ **已修（Phase 3，含纠正）**：该函数校验**未验签**的 payload，接上它等于认证绕过 ⇒ 删除；真实缺陷是 live 路径"仅告警不拦截"+ nonce 从未发给 IdP，均已修 |
| C10 | 死字段/保留字段清理（3 处） | 未列 | **中** | 违反铁律 1 | `grep` 无 "Reserved"/"constructor parity" 保留字段 |

### D 类：把"文档可信度"变成可执行守卫（建议新增）

| 编号 | 动作 | 验收判据 |
|------|------|----------|
| D1 | 为对比报告增加"数字来源"检查：文中的端点/文件/模块数必须能在 `ROUTE_CONTRACT.md` 或 `git ls-files` 命令注释中找到 | 脚本对故意写错的数字能变红（铁律 8） |
| D2 | 文档中的 MSC 编号必须出现在 `MSC_SEMANTICS.md`（新增编号需同时登记） | 脚本对未登记的 `MSC\d+` 报错 |
| D3 | 引用人工文档（如 API_COVERAGE_REPORT）时必须带时间戳，且过期条目不得作为"缺失"证据 | 评审清单项 |

---

## 8. 复核命令（可复制）

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 计数
git ls-files '*.rs' | wc -l
git ls-files '*.rs' | xargs wc -l | tail -1
for d in synapse-common synapse-cache synapse-storage synapse-e2ee synapse-federation \
         synapse-services synapse-web synapse-test-utils; do
  printf '%s=%s\n' "$d" "$(git ls-files "$d/*.rs" | wc -l)"
done
find src -name '*.rs' | wc -l
find tests -name '*.rs' | wc -l
find docs -type f | wc -l
find docker -type f | wc -l
ls migrations/*.sql | wc -l
grep -c '\[\[bench\]\]' Cargo.toml

# 版本
grep -A1 -E '^name = "(tokio|vodozemac|insta|sqlx)"$' Cargo.lock

# SQLx 静态化比例
grep -E '^BASELINE_(DYNAMIC|STATIC)=' scripts/ci/sqlx_dynamic_ratio_baseline

# 路由权威口径
grep -n '注册路由条目\|含路由注册的模块文件' docs/synapse-rust/ROUTE_CONTRACT.md

# 房间版本
grep -n 'DEFAULT_ROOM_VERSION: &str\|stable_parse_only' synapse-common/src/room_versions.rs

# 撤回创建路径
sed -n '954,990p' synapse-web/src/routes/handlers/room/events.rs
grep -n 'pdu\["redacts"\]' synapse-services/src/room/messaging/service.rs

# E2EE 关键点
sed -n '82,93p' synapse-e2ee/src/verification/service.rs      # derive_sas = SHA256，非 HKDF
sed -n '384,392p' synapse-e2ee/src/verification/service.rs     # QR 复用公钥/空签名
grep -n 'leak_detection' synapse-e2ee/src/lib.rs || echo 'leak_detection 未声明（死代码）'

# Content Scanner 孤儿
grep -rn 'ContentScanner' --include='*.rs' synapse-services/src | grep -v content_scanner/

# 门禁（本次回归）
bash scripts/check_doc_spelling.sh docs/synapse-rust-vs-synapse-comparison.md
```

---

## 9. 本轮改动清单

| 文件 | 改动 |
|------|------|
| `docs/synapse-rust-vs-synapse-comparison.md` | 升至 v1.3；修正 §1/§2/§3/§4/§5/§6/§7/§9/§10/§12 的计数、版本、伪造引用、特性片段、静态链接断言；§11 增加证据状态说明；§12.5 重写为指向权威清单的方案 |
| `.aspell.ignore.txt` | 增补 10 个合法技术词（aliyun/cancellable/clamav/dags/livekit/redactions/sharding/twilio/webhooks/websocket），修复 docs-quality-gate 回归 |
| `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` | 本文件 |

---

## 10. Phase 1 门禁与验证证据（2026-09-22）

环境：分支 `opt/protocol-correctness-2026-09-22`（基线 `main @ db1538b7`）；
`SQLX_OFFLINE=true`、`TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test`（per-test schema）。

| 门禁 | 命令 | 结果 |
|------|------|------|
| 格式棘轮 | `./scripts/check_fmt_ratchet.sh` | `fmt debt: current=0 baseline=0` → **OK** |
| Clippy（默认矩阵） | `cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings` | **exit 0**（4m42s） |
| Clippy（全特性） | `cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings` | **exit 0**（14m02s） |
| SQLx 棘轮 | `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | `dynamic=1501 static=61 ratio=0.9609` → **OK**（基线未上调） |
| 受影响 crate 测试 | `cargo nextest run -p synapse-common -p synapse-services -p synapse-storage -P tdd --features test-utils` | **4235 passed / 0 failed / 0 skipped**（663s） |
| 单元测试目标 | `cargo nextest run --profile ci --all-features --test unit --test-threads 4` | **1792 passed / 0 failed / 2 skipped**（既有 skip） |
| 撤回相关集成测试 | `cargo nextest run --profile ci --all-features --test integration redact --test-threads 1` | **15 passed** |

### 10.1 每项修复的"能变红"证据

| 项 | 红（修复前） | 绿（修复后） |
|----|--------------|--------------|
| B1（common） | `cargo nextest … test_redacts_in_content` → **E0425**（函数不存在） | 3 passed |
| B1（服务层 content 注入） | `create_redaction_in_v11_room_puts_target_in_content` → FAIL：`content = {"reason":"spam"}` | 2 passed（v11 + v10 对照） |
| B1（PDU 顶层字段） | 变异探针：把 `if !content_has_redacts` 改为恒真 → v11 用例 FAIL（顶层 `redacts` 出现） | 2 passed |
| B8（半写窗口） | 用**修复前的真实代码**做探针 → FAIL：`events` 行残留（`persisted = Some(..)`） | 4 passed（含 3 个既有 DAG 用例） |
| B10a（吞错） | 变异探针：`max_ts.unwrap_or(0)` → `next_event_ts_propagates_db_error` FAIL | 3 passed |
| SQLx 棘轮自身 | 仓库既有 `sqlx_ratio_gate_fails_when_dynamic_exceeds_baseline` | 10 passed |

> **一处实现调整**：B8 的断言最初用 `SELECT COUNT(*)`，触发 SQLx 棘轮 +1（该棘轮把 `#[cfg(test)]` 内联查询也计入 dynamic）。
> 因 `#[cfg(test)]` 内的 `query!` 宏无法进入 `cargo sqlx prepare` 缓存（基线文件已记录该约束），改为调用既有 `EventStorage::get_event`
> 断言行不存在 —— **不新增动态调用点，也不上调基线**。随后用修复前代码重新确认该断言仍能变红。

### 10.2 Phase 1 未覆盖（保持不变，见 §7 表）

B2（#20189 完整作用面）、B3（`rc_reports` 限流）、B4（Dehydrated `/events` POST→GET）、B6（v12/v13 创建）、
B7（v1.157.2 公告同类性）、B9（txn 去重补偿）、B10b/c（`transaction.rs` 与 `membership/federation.rs`）、
B11（搜索索引死存储）、B12（Profile 语义）、B13（App Service 登录）、B14（LiveKit `ws_url`）、C1–C10。


---

## 11. Phase 2 门禁与验证证据（2026-09-22）

分支 `opt/phase2-protocol`（基线 `main @ 3041dcb7`，即含 Phase 1 的 main）。

| 门禁 | 结果 |
|------|------|
| `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` **OK** |
| clippy 默认矩阵 / `--all-features` | 均 **exit 0**（1m38s / 1m37s） |
| SQLx 棘轮 | `dynamic=2150 static=61` **OK**（基线 2147 → 2150，+3 全部为 `#[cfg(test)]` 夹具，理由见基线文件 2026-09-22 段） |
| 受影响 crate 全量 | **5122 passed / 0 failed / 0 skipped**（1134s，含新增 4 个测试） |
| `--test unit`（排除既有红模块） | **1799 passed / 0 failed** |
| integration `rate_limit` 过滤 | **10 passed** |
| integration `search` 过滤 | **24 passed** |

### 11.1 每项的"能变红"证据

| 项 | 红 | 绿 |
|----|----|----|
| B10b | 编译错误 `cannot find function gap_fill_already_persisted` | 3 passed |
| B11 | DB 测试实测 **0/2** 命中（`m.room.name`+`m.room.topic`） | 62 个 search 用例通过 |
| B3 | 编译错误（缺 `take_rc_reports_token`）；yaml 守卫用**删除配置键**探针证明能红 | 2 个桶用例 + 1 个 yaml 守卫通过 |
| B9 | DB 测试：事件已落库（`total=1`）但仍可见（`visible=1`）→ 失败 | 1 passed（+ send_message 回归 2 passed） |

### 11.2 环境与既有问题（非本分支引入）

- **`synapse_test` 的 public schema 未迁移**导致 `media::tests::media_fixture_keeps_its_isolated_schema_for_the_whole_test` 报错；执行仓库自带的 `scripts/ci/prepare_test_db.sh`（`RESET_PUBLIC=0`，非破坏性）后 `public`/`test_template_ci` 各 227 表，该用例转绿。
- **`coverage_ratchet_exemption_tests` 3 个用例在 main 上即为红**：`tests/unit/coverage_ratchet_exemption_tests.rs:82` 仍向 `scripts/check_file_coverage.py` 传 `--format lcov`，而该参数已在 `6ad96b03`"删掉覆盖率棘轮的 --format 兼容残留"中被移除（`scripts/ci/run_coverage.sh:101` 同样残留）。**不在本分支范围**（本分支 0 个提交触及 coverage），但 main 当前该门禁不可通过，建议单独修复。


---

## 12. Phase 2 门禁与验证证据（2026-09-22）

分支 `opt/phase2-protocol`（基线 `main @ 3041dcb7`）。全部 7 项改动（B3/B4/B9/B10b/B10c/B11/B13）已实现并验证。

### 12.1 门禁

| 门禁 | 结果 |
|------|------|
| `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` **OK** |
| clippy 默认矩阵 / `--all-features` | 均 **exit 0** |
| SQLx 棘轮 | `dynamic=2151 static=61` **OK**（2147→2151：B9 夹具 +3、B10c 注入 +1，均已在基线文件登记理由） |
| 受影响 crate 全量 | **5122 passed / 0 failed / 1 skipped**（1501s；排除环境型 media 守卫） |
| `--test unit`（排除既有红模块） | **1799 passed**（另 1 个既有 flaky，见 §12.4） |
| integration：ledger + dehydrated 过滤 | **17 passed** |
| integration：login 过滤 | **20 passed** |
| integration：rate_limit 过滤 | **10 passed** |
| integration：search 过滤 | **24 passed** |
| crate 内 membership / federation 过滤 | **124 / 39 passed** |
| 契约 CI 三项检查 | `gen_client_yaml --check` **0**、`gen_route_table --check` **0**、`gen_derived_routes --check` **OK** |

### 12.2 每项的"能变红"证据

| 项 | 红 | 绿 | 提交 |
|----|----|----|------|
| B10b | 编译错误（缺 `gap_fill_already_persisted`） | 3 passed | `51fe643f` |
| B11 | DB 测试实测 **0/2** 命中（name+topic） | 62 个 search 用例 | `5d781469` |
| B3 | 缺 `take_rc_reports_token`（编译红）；yaml 守卫用**删除配置键**探针变红 | 2 桶用例 + 1 守卫 | `cfeb3800` |
| B9 | DB 测试：事件已落库（`total=1`）但**仍可见**（`visible=1`）→ FAIL | DB 测试通过 + send_message 回归 | `f0432358` |
| B10c | DB 测试：持久化失败仍返回 `Ok(())` 且成员关系已写入 → FAIL | DB 测试通过（Err + 未写成员） | `83dcd547` |
| B13 | （新功能）集成测试直接覆盖 200/403/401 | 20 个 login 用例不回归 | `e4d594cc` |
| B4 | storage 分页 Option 语义按新契约改写；端到端断言 GET+null+405 | 1 端到端 + 1 ledger GET-only + 2 storage | `5566094b` |

### 12.3 B4 契约产物再生成清单（全部已提交）

derived route 表（always/worker/oidc + `derived_routes.rs`）、6 个 ledger fixture（两车道 ×3 profile）、
2 个 route-ledger 快照、`docs/synapse-rust/ROUTE_CONTRACT.md`、`docs/openapi/route-table.json`、
`docs/openapi/client.yaml`、`scripts/api_test/ledger.json`、`scripts/api_test/handler_schemas.json`。
两条 fixture 车道在再生成后**逐字节稳定**（手改为 GET 后由 `synapse_ledger_export` 复现），
`LEDGER_SCHEMA_VERSION` 无需 bump（方法值变化非形状变化）。

### 12.4 本轮暴露的既有问题（均非 Phase 2 引入）

1. **`coverage_ratchet_exemption_tests` 3 个用例在 main 上即为红**：`tests/unit/coverage_ratchet_exemption_tests.rs:82` 仍传 `--format lcov`，而该参数已在 `6ad96b03` 移除（`scripts/ci/run_coverage.sh:101` 同样残留）。
2. **`sync_helpers_tests::room_event_to_json_age_is_zero_when_event_is_now` 为时钟边界 flaky**：默认 nextest 下失败、`--no-capture` 下通过；本分支 0 个提交触及 sync helpers。
3. **`scripts/api_test/scan_handler_schemas.py` 的 `ROOT` 是硬编码绝对路径**（指向主工作树 `/Users/ljf/Desktop/hu_ts/synapse-rust`）。本轮运行该脚本时**误写入另一工作树**的 `docs/openapi/client.yaml` 与 `scripts/api_test/handler_schemas.json`；已改为手动修补本工作树输入。该脚本在 worktree 场景下不可用，应改为 `Path(__file__).resolve().parents[2]`。
4. **ledger `query_params` 是"接受但无消费方"的死元数据**：`ledger_annotations.txt` 允许该键，但 `extract_registered.py` / `gen_derived_routes.py` 从不消费它，因此导出里恒为空 —— 新增的 GET query 参数（`next_batch`/`limit`）无法记录进契约。
5. **共享 `synapse_test.public` 被并发工作反复清空**：`media::tests::media_fixture_keeps_its_isolated_schema_for_the_whole_test` 依赖已迁移的 public schema，本轮两次因环境失效而红；重跑 `scripts/ci/prepare_test_db.sh`（`RESET_PUBLIC=0`，非破坏性）后即绿。
6. **跨仓破坏（B4）**：`../matrix-js-sdk/src/rust-crypto/DehydratedDeviceManager.ts:275-280` 仍以 POST + body 游标调用该端点，后端改为 GET 后该 SDK 需同步修改，否则 SDK 车道 `check_sdk_route_coverage.py` 会变红。


---

## 13. Phase 3 安全修复证据（2026-09-22）

分支 `opt/phase3-security`（基线 `main @ 7ec37484`）。C1/C2/C3/C9 全部完成。

### 13.1 每项的"能变红"证据

| 项 | 红 | 绿 | 提交 |
|----|----|----|------|
| C1 SAS HKDF | 已知答案测试失败（旧 `SHA256(secret\|\|info)` 得 `4cc1cf67c070…`，HKDF 应为 `b096eeb579a0`） | HKDF 向量 + 规范 info 串 + 现有 6 个 SAS 测试全绿 | `a6f797f3` |
| C1 MAC 校验 | 变异探针：跳过 `secure_compare` ⇒ 篡改用例 FAIL | 正确 MAC 通过并置 Done；篡改/缺 keys/缺 peer key/无私钥一律拒绝且不置 Done | `a6f797f3` |
| C1 无随机 SAS | `generate_sas_fails_closed_without_key_material`（旧实现返回随机字节） | fail-closed 返回错误 | `a6f797f3`/`fda1317f` |
| C2 QR | 两个用例 FAIL：`scan_qr_code` 旧实现返回 `Ok(())` 并创建请求 | 两方法返回 M_UNRECOGNIZED 且不写状态 | `fda1317f` |
| C3 死模块 | （无测试可红；删除前从未编译） | `cargo check -p synapse-e2ee` 通过；零引用 | `db570918` |
| C9 fail-closed | 变异探针：把 `return Err` 去掉 ⇒ 用例 FAIL（旧实现仅 warn 后返回 Ok） | 伪造 alg=none id_token ⇒ 401；授权 URL 携带 nonce | `c010135c` |

### 13.2 对原审计建议的纠正（重要）

1. **C9 原建议"接线 `validate_id_token_claims`"是错的**：该函数只解析并校验**未验签**的 base64 payload（iss/aud/exp），其自身注释记录了它曾被当作 fallback 后被 OPT-001 移除。接上它等于引入认证绕过。真正需要修的是 live 路径的两个漏洞：
   - `exchange_code` 对 `validate_id_token` 失败**仅 `tracing::warn!` 后照常返回 Ok** ⇒ id_token 校验形同虚设，已改为 401 fail-closed；
   - `sso.rs`/`provider.rs` 生成并存储 nonce 却**从不发给 IdP** ⇒ 合规 IdP 不回传 nonce，live nonce 校验永远失败（也是仅告警），已改为授权 URL 携带 nonce。
2. **C1 的 MAC 校验受 schema 限制**：`verification_sas` 是 `tx_id` 单行主键，只存一侧私钥，peer 公钥从不落库 ⇒ 服务端在 `confirm_sas` 时无法自行重算共享密钥。本轮采取"证据随请求携带"（请求体新增 `keys` + `peer_pubkey`，服务端用本地私钥 + 该公钥重算）而不是新增敏感列存共享密钥。**行为变化**：旧的 `verify_mac`/`verify_done` 调用（只有 `mac`）现在 fail-closed 被拒；前端需补 `keys`/`peer_pubkey`。
3. **C2 无法在服务端"实现"**：QR 载荷必须用设备私钥签名，而服务端只有公钥材料（`device_keys` 存的都是 public）。因此选择明确不支持，而不是继续伪造。

### 13.3 门禁

| 门禁 | 结果 |
|------|------|
| `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` **OK** |
| clippy 默认矩阵 / `--all-features` | **exit 0** |
| SQLx 棘轮 | `dynamic=2151 static=61` **OK**（本轮未新增 SQL） |
| `ORDER BY <*_ts>` 单键棘轮 | **收紧**：删除文件后该棘轮先报"基线已过期"（证明它会变红），`--update` 后 `103 处 / 46 文件 OK` |
| `synapse-e2ee` 全量 | **439 passed** |
| `synapse-web` verification 路由 | **14 passed** |
| OIDC 相关单测 | **5 passed**（含新增 fail-closed） |

### 13.4 残留（可选清理，未在本轮做）

- `migrations/00000000_unified_schema_v12.sql` 的 `leak_alerts` 表（+ 3 索引、`INDEXES.md` 行）随模块删除后已无写入方；删表需动合并基线，属独立清理项。
- `verification_qr` 表与 `QrState`/`store_qr_state` 在 QR 明确不支持后同样失去写入方。
- C1 的请求体扩展是**破坏性**变更（旧客户端缺 `keys`/`peer_pubkey` 会被拒），需要前端同步。
