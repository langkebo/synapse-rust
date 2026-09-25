# PROJECT_REMAINING_ISSUES 复核：13 项报项真实性核验 + 优化方案

> 输入：`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`（当前口径 = §0.2 + §22.3，§23 追加 E2EE 删除面）
> 方法：**不采信文档任何既有标记**，逐条回到源码取证（`路径:行号` 或可复现命令 + 实测输出）；
> 外部事实（Matrix 规范、上游 Synapse）用**主源**（spec 仓库 YAML / Synapse 源码与 CHANGES）核对。
> 日期：2026-09-25。核验基线：`opt/consolidated` @ `40f618356`（2026-09-25 11:06）+ **工作树在途改动**。

---

## 0. 基线与方法（先读这一节）

### 0.1 核验环境

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
git log --oneline -1          # 40f618356 docs(audit): 登记 D-48 ...
git status --short | wc -l    # 70 个文件处于 staged（含 §23 E2EE 删除面，未提交）
git worktree list             # 另有 .worktrees/c19b（同 HEAD）、/Users/ljf/...-worktree/invite-policy
```

**铁律（本轮踩过的坑，写入以免后人误判）**：

1. **所有全仓 grep 必须排除 `.worktrees/` 与 `target/`** —— 本仓存在与 HEAD 同步的第二份工作树
   （`.worktrees/c19b`，同 `40f618356`），不过滤会把同一份代码计两次。
2. **必须区分 HEAD 与工作树**：§23 的 E2EE 删除面**已 staged 但未提交**，
   故 `HEAD` 仍含 `verification/`、`device_trust/`、`verification_routes.rs`，
   而**工作树没有**。凡引用这些文件的行号，一律标注取自哪一侧。
3. **行号会漂移**：`migrations/00000000_unified_schema_v12.sql` 在工作树被删 105 行
   （§23 删表），§22 里的 `:2839` 是 HEAD 行号，工作树为 `:2750`。
4. **口径纪律**：本文件所有数字都附生成命令；凡文档既有数字与本轮实测冲突，以实测为准并标注。

### 0.2 一句话结论

**13 项报项中，8 项确认存在、2 项部分成立、2 项应降级/证伪、1 项因对象被删除而作废。**
另在核验过程中**新发现 8 项**（其中 2 项比报项本身更严重：出站 PDU 缺 `depth`/`auth_events`、
`sign_and_broadcast_event` 存在两份策略相反的实现）。

**最紧迫项：O-4**（统一 `sign_and_broadcast_event`，1 周，无依赖）—— 两份实现策略相反
（fail-closed vs fail-open）且都缺 `depth`/`auth_events`，直接导致联邦 PDU 无效。

**新增优化项（4 项）**：

| 编号 | 级别 | 问题 | 建议行动 | 优先级 |
|------|------|------|----------|--------|
| **O-4** | **🔴 最紧迫** | 两份 `sign_and_broadcast_event` 策略冲突 + 出站 PDU 缺 `depth`/`auth_events` | 统一为 fail-closed，补全 PDU 字段，合并实现 | **P0（本周）** |
| **O-1** | **P0** | v12 房间版本默认仍为 v11 | 阶段 1: 实现 v12 验证；阶段 2: 升级默认版本 | **高（本月）** |
| **O-2** | 中 | 动画缩略图缺失 | 阶段 1: 参数支持；阶段 2: 动画检测；阶段 3: WebP 编码 | 中 |
| **O-3** | 低 | MSC4133 不完整 | 记录为已知差距，待上游稳定 | 低 |

**推荐执行顺序**: O-4（本周） → O-1（本月） → O-2 → O-3

| 报项 | 文档标记 | 本轮判定 | 关键判据 |
|---|---|---|---|
| P0 `/send_join` PDU | 已修 + 残余 3 条 | **残余 3 条仍存在**，另新增 1 条更直接缺陷 | `synapse-storage/src/event/create.rs:14-19`（无图元数据列）；`synapse-services/src/room/messaging/service.rs:157-167`（出站 PDU 无 `depth`/`auth_events`） |
| 高 E2EE SAS 3 处偏离 | 🔴 | **⚪ 对象消失（作废）** | 工作树 `synapse-e2ee/src/verification/` 已删（staged）；全仓 0 命中 |
| 高 客户端撤回不级联 | 🔴 | **🔴 存在** | `synapse-web/src/routes/handlers/room/events.rs:954-1001` |
| 高 Content Scanner 零调用点 | 🔴 | **🔴 存在** | `synapse-services/src/content_scanner/service.rs:203` 起的 `#[cfg(test)]` 是唯一调用方 |
| 中 `dag.rs` 注释幻觉 | 🔴 | **🔴 存在**（另有 2 处同类） | `synapse-storage/src/event/dag.rs:203-205`；`get_state_dag_edges` 生产调用点 0 |
| 中 `msc2965/auth_issuer` 在册 | 🔴 | **🔴 存在** | `synapse-web/src/routes/assembly.rs:197` |
| 中 Profile 两条 | 🔴 | **🔴 存在**，且缺口比文档描述更大（缺整个 v1.16 稳定面） | `synapse-web/src/routes/handlers/extended_profile.rs:124`；`capability_governance.rs:507` |
| 中 MSC4502 / MSC4262 未收敛 | 🔴 | **❌ 证伪（维持 §22.4）**，本轮抽查未推翻 | MSC4502：`room.rs:53 → members.rs:387-424 → membership/service.rs:610-706 → membership/mod.rs:716-796`；MSC4262：`federation/edu.rs:31,628-702` + `user/storage.rs:783` + `sliding_sync_service/extensions.rs:254` |
| 中 Admin 媒体族缺口 | 🔴 部分 | **🔴 存在**；文档"上游 18"**无法复现**，本轮给出可核验的 15 条 | `synapse-web/src/routes/admin/media.rs:16-22` |
| 中 `animated` + 配额错误码 | 🔴 部分 | **🔴 两项均存在** | `animated` 全仓 0 命中；`synapse-services/src/media/mod.rs:250-252` 返回 400 |
| 低 v12/v13 不可创建 | 🔴 | **⚪ 设计取舍（非缺陷），但需升级** | `synapse-common/src/room_versions.rs:108-115`；**上游 v1.162 已将默认版本提升至 v12** |
| 低 `search_index` 遗留表 | 🔴 | **🔴 存在**（+ 派生物 `INDEXES.md` 漂移） | baseline `:2750-2761` + 4 索引 `:3735-3738`；生产消费者 0 |
| 低 ledger `query_params` 无消费方 | 🔴 | **🟡 部分证伪**（字段死，但序列化被 fixture 钉住并被 `gen_route_table.py` 消费） | `route_ledger.rs:83-84,106-110` 调用点 0；`ledger_export.rs:156` + 6 份 fixture |

**新发现（8 项，文档未列）**：

| 编号 | 级别 | 新发现 | 判据 |
|---|---|---|---|
| **N-1** | **P0** | 出站 `/send` PDU **缺 `depth` 与 `auth_events`** —— 本地事件广播给远端时只有 `prev_events`，不是 v3+ 合法 PDU | `synapse-services/src/room/messaging/service.rs:157-167`（json! 仅 9 键） |
| **N-2** | 高 | `sign_and_broadcast_event` **两份实现、策略相反**（违反反冗余铁律 2）：messaging 版 fail-closed、membership 版 fail-open 且**无 room-version 感知的 `redacts` 放置** | `messaging/service.rs:131-151` vs `membership/service.rs:486-520` |
| **N-3** | 中 | `create_event` 的文档注释**自述"delegates here"是假的**：它有自己的 INSERT，从不调用 `create_event_with_graph` | `synapse-storage/src/event/create.rs:8-20` vs 注释 `:51-58` |
| **N-4** | 中 | `block_on_scan_failure=false`（fail-open）**只对 webhook 生效**；ClamAV 路径 4 条失败路径全部直接 `Err` | `content_scanner/service.rs:62-100`（对照 `on_webhook_failure` `:159-172`） |
| **N-5** | 中 | Spec v1.16 已稳定 MSC4133：缺稳定 `/{keyName}` 路由、`m.tz`、`M_PROFILE_TOO_LARGE`/`M_KEY_TOO_LARGE`；且 `/versions` 只声明到 **v1.14** | spec `profile.yaml:19,22,27,104-106,305-320`；仓库 0 命中 | **⚠️ 修正**：上游 Synapse v1.162 **仍未完全实现 MSC4133**。`ProfileFieldRestServlet` 仅在 `msc4133_enabled` 实验开关下支持不稳定前缀 `uk.tcpip.msc4133`，**非稳定端点**。错误码 `KEY_TOO_LARGE` 存在但 `M_PROFILE_TOO_LARGE` 未使用。`m.tz` 字段**未实现**。 |
| **N-6** | 低 | `pdu.rs` 模块注释里的 event_id 格式仍是**错的**（`$<ts>_<rand>:<server>`，实际分隔符是 `$`）——正是 §22.1 N-5 已修正而此处未同步 | `synapse-web/src/routes/federation/pdu.rs:37` vs `synapse-common/src/crypto.rs:153` |
| **N-7** | 低 | `migrations/INDEXES.md` 与 baseline 漂移：baseline 为 `search_index` 建 **4** 条索引，文档列 **0** 条 | `grep -c search_index migrations/INDEXES.md` → 0；baseline `:3735-3738` |
| **N-8** | 低 | CI 契约 `TABLE_CONTRACTS["search_index"]` 漏列 GIN 索引 `idx_search_index_content_trgm`，且校验是**单向**的（只查"契约写的索引是否存在"）⇒ 该方向永远绿（铁律 8 同类） | `scripts/check_schema_contract_coverage.py:177-194` vs `:302-305` | **已随 search_index 表删除而失效** |

---

### 0.3 执行状态总览（2026-09-25 同步）

> **同步基线**：本分支 `fix/local-event-graph-metadata` 的 HEAD（B0+B1 全部完成，9 个提交；
> 具体哈希见 `git log --oneline fix/local-event-graph-metadata -10`，本文不钉死以避免自我失效）
> 与 `opt/consolidated` @ `3b1d28598`（并发会话的进展）。§3 各 Task 标题上的状态标记与
> §5 的未完成清单都由本节派生；main 侧每一条都是**本轮实测**（判据见 §5），不是沿用旧结论。
>
> ⚠️ 本分支**尚未合并**：main 目前**仍没有** B1 的任何一项（出站 PDU 仍缺 `depth`/`auth_events`、
> 仍有 `sign_and_broadcast_event` 两份相反实现、入站仍不落源服务器签名）。

**A. 本分支已完成（9 个提交，工作树干净）**

| 项 | 提交 | 验证 |
|---|---|---|
| B0：T0.2 审计文档口径 / T0.3 `pdu.rs:37` / T0.4 `create_event` 注释幻觉 | `281c8f4e4` | fmt ratchet `OK (0)` |
| B1-1a 规范的 Auth events selection（此前全仓无实现） | `e14be2eb1` | 9 单测 + 3 变异自证 |
| B1-1b `GraphMetadataWriter`（覆盖全部 auto-commit 本地写入） | `bf90f430f` | 8 单测 + 3 变异 |
| B1-1e 建房 `CreationGraph` + 真前向极值查询（修 1b 前提错误） | `900938510` | 6 单测 + 3 变异；DB 极值测试 + 变异；建房集成 8/8 |
| B1-1d 出站 PDU 单实现 + 补 `depth`/`auth_events` + 统一 fail-closed | `864d0f6b2` | 6 单测 + 3 变异；membership 126/126 |
| Task 2 入站（`/send` + backfill）落源服务器签名材料 | `efbe4af73` | 谓词单测 + DB 回环测试 + 2 变异 |
| SQL 宏化（Phase B2）+ 两处 §23 失效守卫 + `rand::rng()` baseline | `4bee43fcd` `fbffa58d0` | 守卫 21/21 |
| **最终门禁（冻结提交）** | — | lib **6308/6308**、unit **1773/1773**、集成子集 **15/15**、clippy exit 0、fmt `OK (0)` |

**B. 并发会话已在 main 完成（`3b1d28598`，本分支不含）**

| 原计划项 | main 现状 | 实测判据 |
|---|---|---|
| Task 5 Content Scanner **接线** | ✅ 已接（媒体上传 + 发消息） | `routes/media/upload.rs:87,128 scan_media(...)`；`handlers/room/events.rs:306 scan_text(...)`、`:1007 is_enabled()` |
| Task 6 稳定 `/{keyName}` 路由 | ✅ 已注册 | `assembly.rs:231 /_matrix/client/v3/profile/{user_id}/{key_name}`（unstable 仍在 `:224`） |
| Task 6 停用用户写自定义字段 | ✅ 改为"存在即通过" | `synapse-storage/src/user/storage.rs:698` 去掉了 `is_deactivated` 过滤 ⚠️ **见 §5 R-1** |
| Task 10 `msc2965/auth_issuer` 路由 | ✅ 已摘除 | `assembly.rs` 已无该路由（handler `auth_discovery.rs:66` 成为死码） |
| Task 10 `dag.rs` 注释幻觉 | ✅ 注释已改 | 原 "Used by `/send_join` …" 文本已不存在 |
| T11 `search_index` 遗留表 | ✅ 已删表 + 契约用例同步 | 提交 `00271cf91`、`acac1f74c`；baseline `grep -c search_index` = **0** |
| N-4 ClamAV 失败策略不对称 | ✅ 已统一 | `content_scanner/service.rs:47,48,124-137` 全部走 `on_scan_failure` |

---

### 0.4 本轮目标（§6.7 顺序）逐项证据总览（2026-09-26，`opt/consolidated` @ `7cbf6594c`）

> 本节由**执行者**维护，只登记**本轮实测过**的证据；每条都给了可复核的判据，
> 便于下一轮直接接着做，而不是重新推导。

| 目标项 | 状态 | 提交 | 测试 / 变异自证 | 门禁实测 | 残留与归属 |
|---|---|---|---|---|---|
| ① M-1（B1 合并 + 合并结果门禁） | ✅ | `7c88cae1a` | 合并后 lib **6316/6316**、unit **1777/1777**、集成子集 15/15 | fmt `OK(0)`、clippy exit 0 | 合并暴露的 4 项既有红（指纹常量、DDL allowlist 陈旧项、3 份 ledger 金样本、`assembly_route` 的 `auth_issuer`）均已修 |
| ② U-7 媒体配额错误码 | ✅ | `d70b4fc20` → `cf02fafa5` | 类型化 `QuotaRejection` + 4 个生产构造点；用户配额 403 `M_RESOURCE_LIMIT_EXCEEDED`、单文件上限 413 `M_TOO_LARGE`（从不使用 `M_USER_LIMIT_EXCEEDED`） | 随批 fmt/clippy/targeted nextest | 无 |
| ② U-4 profile 校验与错误码 | ✅ | `5e739ef8a` → `cf02fafa5` | `M_KEY_TOO_LARGE`（255 字节 key）+ `M_PROFILE_TOO_LARGE`（64 KiB value） | 同上 | **不升 `/versions`**（§6.3 决策：能力与稳定路由已在） |
| ② U-1 MSC3912 单层级联 | ✅（并发会话实现） | 见其提交 | —— | —— | 语义缺口：不产生真 `m.room.redaction`、`redacted_by` 丢失、无逐事件 `can_redact_event`（§5 **U-1b**） |
| ② U-3 扫描拒绝 + 隔离裁定 + **hash 级自动隔离** + 指标 + 显式配置 | ✅ | `338395f98` + `4f7299e82` + `cbe6517c5` | 验收①②③④全覆盖：集成 `hash_level_quarantine_auto_quarantines_identical_reupload` 1/1（**断言扫描器关闭**，证明与扫描无关；含命中计数==1 与不同内容负例）、storage `admin_media` 17/17、crypto 3/3；**变异 7 个**（扫描半批 4 + hash 半批 3）全部转红后还原 | fmt `OK(0)`、clippy exit 0 | SQLx 棘轮：**本批生产动态增量 0**（反查用静态宏）；残留红=既有 +3 生产 / +1 测试基础设施，本批另 +1 测试夹具（D-13/14 必须动态），两基线**故意不动**（见 §6.2） |
| ② U-10 ledger `query_params` 真填值 + 反向守卫 | ✅ | `fd178425b` | 123 条注解；5 层守卫（夹具双向 + handler `Query<Struct>` 双向）；**我独立变异**（删注解里的 `limit`）⇒ 抽取器 exit 1 并同时报出三类失败；unit 44/44 | `check_route_contract.sh` exit 0（修绿了 U-10 之前**既有**的红门禁）；`gen_client_yaml.py --check` 与 `gen_route_table.py --check --ledger <fresh>` 双双 exit 0 | `scripts/api_test/ledger.json` 仍是旧导出（1096 条、字段空），**故意不刷**：它是 `client.yaml` 的字节级输入，刷新会弄红那条门禁 |
| ② U-13 reference hash | 🟡 **第 1 步 ✅ / 前置 P0 ✅ / 第 2–3 步 ⬜** | `64ffc13a6`（第 1 步）、`0880f6a5f`（content hash P0） | 第 1 步：上游 Synapse **v10/v3 已知答案向量**逐字节通过 + 上游 `redact()` 全量单测期望转 v1–v12 矩阵（24/24）；content hash：上游 `test_sign_minimal`/`test_sign_message` 向量通过；**变异 6 个**全部转红后还原 | fmt `OK(0)`、clippy exit 0、federation lib 198/198 | **第 2 步阻塞**（连续 4 轮取证）：并发会话未提交地新增 `EventWriter::create_event_with_pdu`（6 文件 169 行，覆盖第 2 步要改的全部 writer 文件）。算法（5 步链）、30 个调用点清单、7 个签名站点、6 条验收测试均已冻结于 §6.6 |
| ① 合并结果上的 fmt / clippy / lib / unit / 集成 | ✅（集成按受影响面） | `b83cbcaac` + `94fc91442`（本轮修掉 lib 的 2 个真红与扫描面 3 个真红） | fmt `current=0`、clippy exit 0（多轮、多 tip）。**lib 全量（最终 tip，清理 634 个残留 schema 后在干净库上定格）：6358 例 6358 passed / 0 skipped** ✅（`bash-114`，2026-09-26 06:05→06:32）。**unit 全量（同一 tip）：1777 例 1773 passed / 4 failed / 2 skipped**，4 个失败**全部**是已归因的 SQLx 棘轮守卫（`sqlx_ratio_gate_*` ×3 + `no_new_production_literal_dynamic_sql` ×1，逐项归因见 §6.2） | 集成：按受影响面跑子集（media/quota、profile/route/ledger、federation_transaction/create_room） | 「lib 曾红」的两段根因都已定位并处置：①5 个 `schema_validator::db_tests` 是本机 `public` 未按 baseline 播种（见 **U-17**，已用 `RESET_PUBLIC=0` 播种为 222 表）；②2 个 `room::auth` 用例是真缺陷（见 **U-16**，已修 `b83cbcaac`）。**全量集成批次（约 1446 例）未跑**（时间预算；且实测每轮会新建数百个测试 schema，DDL 随残留超线性变慢——清理后复跑是必要前置），不得当作通过 |
| 集成全量分块（`--threads 1`，逐块清 schema） | — | — | **partition 1/6：241 例 235 passed / 6 failed**，6 例**全部在 `338395f98` 复现为既有红**：`api_admin_room_lifecycle_tests::test_admin_room_history_purge`、`api_placeholder_contract_p1p2_tests::test_scanner_info_contract_is_not_empty_success`、`api_relations_authorization_tests::relation_is_allowed_for_members`、`api_route_snapshots_tests::snapshot_versions_endpoint`、`api_search_thread_tests::test_room_context_rejects_non_member_and_admin_override`、`api_sync_filter_tests::test_sync_filter_applies_room_timeline_matchers_before_limit` | 分块跑是为了避开"每轮新建数百 schema ⇒ DDL 超线性变慢"；剩余分块在后续轮次继续 |
| 集成实测（受影响面，`--test-threads 1`） | — | — | media+quota **34/34 绿**（修复前 32/34）；federation_transaction+create_room **15/15 绿**；profile/route/ledger 子集 139 例中 **132 passed / 7 failed**（U-20 修复后再跑全套既有红清单：**18 例中 11 例转绿、7 例仍红**，仍红的 7 例为：`api_admin_room_lifecycle_tests::test_admin_room_lifecycle_management`、`api_auth_routes_tests::test_auth_issuer_returns_unrecognized_when_oidc_is_disabled`、`declared_route_manifest_entries_are_actually_wired`、`declared_route_manifest_full_snapshot_matches_{default,worker}_state`、`snapshot_capabilities_v3`、`snapshot_versions_endpoint`）；此前记的 7 例里含 U-20 的病根，其中 5 例（`declared_route_manifest_*` ×3、`snapshot_capabilities_v3`、`snapshot_versions_endpoint`）与 `test_global_thread_routes_return_real_data` 已在 `338395f98`（U-10 之前）逐条复现；第 7 例 `api_auth_routes_tests::test_auth_issuer_returns_unrecognized_when_oidc_is_disabled` 单独在最终 tip 复现为 `left: 404, right: 400` | 既有红的成因：① 路由 manifest 里 app_service 的通配符代理路由（`/_matrix/{app,client}/v1/proxy/{as_id}/{*path}`）与 live router 不一致；② `/versions`、`/capabilities`、线程路由快照落后；③ **`auth_issuer` 路由被 `76e5f9136` 摘除，但 `api_auth_routes_tests` 仍断言旧行为（期待 400，实得 404）⇒ 摘路由时漏改用例**。本轮**未**擅自 `cargo insta accept`，也未改这些既有红（属并发会话/其它批次范围） |


### 0.5 收尾状态与交接（2026-09-26，`opt/consolidated`）

**逐项结论**：§0.4 的表就是权威结论。7 个目标项中 6 项已完成并落地
（M-1、U-7、U-4、U-1、U-3、U-10），**仅 U-13 的第 2–3 步未完成**，且其未完成
**不是**难度或时间问题，而是被外部写者阻塞（见下）。

**唯一阻塞项：U-13 第 2 步（接线 v4+）+ 第 3 步（互操作门槛）**

- 阻塞事实（连续 5 轮取证，2026-09-26 05:42 复核仍成立）：并发会话在 `synapse-rust/`
  主工作树里**未提交**地修改 6 个文件，其中三个正是第 2 步必须改的写入缝：
  `synapse-storage/src/event/writer.rs`（新增 `EventWriter::create_event_with_pdu`）、
  `synapse-services/src/notifying_event_writer.rs`、`synapse-services/src/graph_metadata.rs`
  （另有 `synapse-storage/src/event/reader.rs`、`synapse-services/src/room/messaging/events.rs`、
  `synapse-storage/src/test_mocks/event.rs`）。
  `git show HEAD:synapse-storage/src/event/writer.rs | grep -c create_event_with_pdu` = **0**
  ⇒ 该 trait 变更只在工作树里；任何改动这些文件的提交都会让 `git merge` 因"本地修改会被覆盖"被拒。
- **解锁条件（任一即可）**：① 对方提交或 stash 那 6 个文件；② 明确授权接管
  （此时需先由对方把在途改动落盘，再按 §6.6 的 5 步链实施）。
- **实施依据已全部冻结**（无需重新推导）：§6.6 的 5 步算法、30 个生产调用点清单（16 文件）、
  7 个签名站点的房间版本可用性表、"三份房间版本解析先收敛为一份"的结论、
  6 条验收测试清单、以及第 1 步已落地的 `event_id::compute_event_id`。
- **不要**只做签名半边：已论证——若 `hashes`/`signatures` 变正确而 `event_id` 仍是随机值，
  对等端会按 reference hash 派生出与本仓不同的 ID，`prev_events`/`auth_events` 指向的 ID
  在对端不存在，事件仍被拒（只是错误类型变了），反而掩盖真实失败。必须整链一起落地。

**已知红门禁（均有逐项归因，非本会话新增）**

| 门禁 | 现状 | 归属 |
|---|---|---|
| `sqlx_ratio_gate`（unit） | 生产 516 > 基线 513；测试基础设施 713 > 基线 711 | 生产 +3 全为并发会话 v12 的既有提交；测试 +2 = 既有 +1 + U-3 新增 db_tests 夹具 +1（`#[cfg(test)]` 宏不进 `.sqlx`，按 D-13/14 必须动态）。**两基线故意未动**（不替他批改棘轮） |
| `sqlx_dynamic_literal_guard`（unit） | `synapse-storage/src/event/depth.rs:41` 1 > 基线 0 | 既有（本会话未触碰该文件） |
| 集成 7 例 | 见 §0.4 集成行 | 路由 manifest 通配符代理路由、`/versions`+`/capabilities` 快照、线程路由、`auth_issuer` 摘路由漏改用例（`76e5f9136`） |

**复验命令（本会话实际用过，逐字可跑）**

```bash
# 本地前置：播种 public（否则 schema_validator::db_tests 假红，见 U-17）
TARGET_SCHEMA=public RESET_PUBLIC=0 TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test \
  bash scripts/init_test_public_schema.sh

SQLX_OFFLINE=true cargo nextest run --workspace --lib --all-features --locked --test-threads 4
SQLX_OFFLINE=true cargo nextest run --test unit --features test-utils --locked --test-threads 4
SQLX_OFFLINE=true TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test \
  TEST_DB_TEMPLATE_SCHEMA=test_template_ci \
  cargo nextest run --profile ci --all-features --test integration --test-threads 1 -E 'test(/media/) | test(/quota/)'
./scripts/check_fmt_ratchet.sh
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
```

> ⚠️ 跑失败的 insta 快照用例（`api_route_snapshots_tests::snapshot_*`）会在
> `tests/integration/snapshots/` 留下 `*.snap.new`；CI 的快照门禁会因遗留文件变红，
> 调查后必须删掉（本会话已清理两个 worktree 的遗留）。

---

## 1. 逐条核验

### 1.1 ⚪ E2EE SAS 三处偏离 → **对象消失，整条作废**

§0.2 第 3 条与 §22.3 的"高｜E2EE SAS —— 5 个子项中 1 修 4 存"**均已过期**，§23 已自行声明作废，
但 §0.2（文档自称"唯一权威"）**没有同步**，仍把 SAS 列为"当前须处理"。

**判据**：

```bash
# 工作树：目录不存在（staged 删除）
git status --short synapse-e2ee/src/verification synapse-e2ee/src/device_trust
#   D  synapse-e2ee/src/verification/{mod,models,service,storage}.rs
#   D  synapse-e2ee/src/device_trust/{mod,models,service,storage}.rs
git status --short synapse-web/src/routes/verification_routes.rs   # D
# 偏差代码全仓 0 命中（排除 .worktrees/target）
grep -rn "verification.commitment\|% 64\|generate_decimal_from_emoji\|compute_shared_secret" --include=*.rs .
#   → 无输出
```

- HEAD 侧（未提交前）确有：`git show HEAD:synapse-e2ee/src/verification/service.rs` →
  `:146 compute_commitment`、`:149 hasher.update(b"verification.commitment")`、
  `:151 STANDARD.encode`（带 padding）、`:314 let idx = (byte as usize) % 64;`；
  `git show HEAD:synapse-web/src/routes/verification_routes.rs:405 generate_decimal_from_emoji`。
- **措辞纠正**：§23 表格写"`e2ee/devices.rs`（6 个 handler）"被读成"文件删除"；
  实测该文件**仍在**（16459 字节，status `M`），只有 6 个 handler 被删。
- **无悬挂引用**：`src/e2ee/mod.rs`、`synapse-e2ee/src/lib.rs`、`routes/assembly.rs`、
  `wiring/e2ee.rs`、`routes/context.rs`、`e2ee_audit/audit_service.rs`、`synapse-e2ee/Cargo.toml`
  的对应 re-export/字段/依赖均已同步移除；`cargo check -p synapse-web --locked` **exit 0**（实测 1m28s）。
- **7 张表 + 6 条索引**已随 baseline 删除，`EXPECTED_BASELINE_FINGERPRINT` 已同步为
  `e151e5956fb64914`（`tests/unit/test_isolation_unification_tests.rs:120`）。
- **跨仓 follow-up（本仓无法闭合）**：`CryptoDeviceAdapter.ts` **不在本仓**（`find` 为空），
  客户端改走 `m.key.verification.*` to-device 属 matrix-sdk-fork 的任务。

**结论**：不需要"修 SAS"，需要的是**收尾**：把在途删除提交入库，并把 §0.2/§22.3 的 SAS 行标注作废。

---

### 1.2 🔴 客户端撤回不级联（MSC3912）→ **存在，且实现层次本身不符合规范**

**文档结论正确**，但 §22.5 给的修法（"按 `redacts` 关系调 `cascade_redact_event`"）**不够**：
本仓已有的 cascade 是**存储层内容擦除 + 递归 BFS**，与 MSC3912 定义的语义不是同一种东西。

**判据（本仓）**：

| 事实 | 判据 |
|---|---|
| 客户端 `PUT /rooms/{roomId}/redact/{eventId}/{txnId}` 只撤单条 | `handlers/room/events.rs:920-1006`：`:954` 只读 `reason` → `:962-964` content 仅 `{"reason"}` → `:969-985` 建 1 条 `m.room.redaction` → `:990 redact_event_content(&event_id)` |
| 级联入口**只有管理端** | `admin/room/mod.rs:244`（路由）+ `:818`（唯一调用点，`AdminUser` 提取器 `:789`）；全仓 `cascade` 在 synapse-web 路由侧仅此 3 处 |
| 现有 cascade 是**递归 BFS（depth 5）** | `synapse-storage/src/event/cascade.rs:61 find_cascade_targets`、`:66 for _ in 0..max_depth` |
| 现有 cascade **擦内容、不产生 redaction 事件**（不联邦、不做逐事件鉴权） | `:105 cascade_redact_event` → `redaction.rs:88 redact_event_content`（`UPDATE events SET content=...`） |
| 关联查询**无索引** | `grep -n "idx_events_content_rel\|content->'m.relates_to'" migrations/00000000_unified_schema_v12.sql` → **无输出**（全表扫描） |
| `/versions` 未声明 MSC3912 | `capability_governance.rs:113-130 BASE_UNSTABLE_FEATURES` 无 `org.matrix.msc3912`；全仓 `msc3912` 0 命中（`.rs`） |

**主源（规范原文）**：MSC3912《Redaction of related events》规定的是**客户端面新增字段**：

- `PUT /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}` 的 **body 新增可选 `with_rel_types`**
  （未稳定前缀 `org.matrix.msc3912.with_relations`），值为关系类型列表，含 catch-all `"*"`。
- **缺省或空列表 ⇒ 只撤目标事件**；**只撤"与目标相关"的事件，不追父事件**；
  规范例子明确：撤 `$a` 会撤 `$b`（`$b` 是 `$a` 的 edit），但**不会**撤 `$c`（`$c` 是 `$b` 的 edit）⇒ **单层**。
- 逐事件权限不足者**忽略**（不报错）。响应体不变，可先返回主 redaction event_id、后台再撤其余。
- `/versions` 的 `unstable_features` 应含 `org.matrix.msc3912`。

来源：[proposals/3912-relation-based-redaction.md](https://raw.githubusercontent.com/matrix-org/matrix-spec-proposals/1b3176cfe105a8d572ce27052f3b9e5e44cc0d9c/proposals/3912-relation-based-redaction.md)。

**上游 Synapse（release-v1.161 实测源码）**：这是判断"该怎么做"的最强参照，且**上游也只做了一半**：

```python
# synapse/rest/client/room.py:1374-1377（release-v1.161）
with_relations = None
if self._msc3912_enabled and "org.matrix.msc3912.with_relations" in content:
    with_relations = content["org.matrix.msc3912.with_relations"]
    del content["org.matrix.msc3912.with_relations"]      # ← 必须从落库 content 中摘掉
...
:1407-1415
if with_relations:
    self.hs.run_as_background_process("redact_related_events",
        self._relation_handler.redact_events_related_to, ...)
```

- 开关：`synapse/config/experimental.py:160 self.msc3912_enabled = experimental.get("msc3912_enabled", False)`（**默认关**）。
- 上游读的是 `org.matrix.msc3912.with_relations`（**未稳定名**），稳定名 `with_rel_types` + `*` 的更新**尚未实现**
  （上游 issue [#15687](https://github.com/element-hq/synapse/issues/15687) 仍 open）。
- `handlers/relations.py:191-263 redact_events_related_to`：**单层**（`get_all_relations_for_event[_with_types]`）、
  对每个相关事件**新建一条真正的 `m.room.redaction` 事件**（`ratelimit=False`）、
  权限失败只 `logger.warning` 后继续、`"*"` 走全类型查询。
- 上游**未实现**"请求结束后新到事件补撤"（MSC 有此要求，上游明确缺，见 #15687）。

**影响**：本仓既没有客户端面（功能缺失），管理端那套又与规范语义冲突（递归 + 无 redaction 事件 + 无逐事件鉴权），
`docs/audit/2026-09-23-msc3912-cascade-redaction.md` 还自称 "✅ Implementation Complete"（该文 §Testing 自述
"Unit Tests (Disabled)"），属文档幻觉。

---

### 1.3 🔴 Content Scanner 零生产调用点 → **存在（三路取证一致）**

**判据**（MSC 口径教训"模块存在 / 配置接上 ≠ 功能在工作"）：

| 检查 | 结果 |
|---|---|
| 公开方法 | `content_scanner/service.rs`：`new:14`、`is_enabled:20`、`scan:25`、`scan_text:183`、`scan_media:193` |
| `new()` 调用方 | 生产 **1 处**：`wiring/core.rs:179`（构造） |
| `scan/scan_text/scan_media/is_enabled` 调用方 | **全部**落在 `service.rs` 自己的 `#[cfg(test)] mod tests`（`:203` 起）内 |
| 字段是否被读 | `CoreServices.content_scanner`（`wiring/core.rs:66`）**全仓无读取点**；`grep '\.content_scanner'` 只命中 `:180` 的**配置**字段 |
| 配置默认 | `ContentScannerConfig`（`synapse-common/src/config/mod.rs:241-242`）默认 `enabled:false`、`ClamAv`（`content_scanner/mod.rs:109-110`）；`docker/config/` 无该键 |
| 持久化 | migrations 无 scan/scanner 表；`synapse-storage/src/` 无 scanner 模块 |
| 是否接入上传/发消息 | 上传链 `media/mod.rs:65 → upload.rs:120 → :127 → :77 → media/mod.rs:284-305` **无扫描钩子**；`room/` 内 `scan` 0 命中 |

**"装配了但永不扫描"确实比纯缺失更危险**：配置可开、管理员以为已防护。
补充 N-4：即便接通，`block_on_scan_failure` 的 fail-open 语义**只在 webhook 路径生效**
（`on_webhook_failure:159-172` 是唯一消费点，ClamAV 的 4 条失败路径 `:62-100` 直接 `Err`）。

---

### 1.4 🔴 `dag.rs` 注释幻觉 → **存在（并发现 2 处同类）**

**判据**：`synapse-storage/src/event/dag.rs:203-205` 注释：

> "Used by `/send_join` (federation) to provide the full state DAG to joining servers, and by
> `/get_missing_events` to walk the state DAG when backfilling missing state events (MSC4242)."

实测调用点（排除 `.worktrees/target`）：

| 函数 | 生产调用点 | 测试 |
|---|---|---|
| `get_state_dag_edges`（`dag.rs:208`） | **0** | `event/db_tests.rs:2144` |
| `find_missing_event_ids`（`dag.rs:14`） | `federation/transaction.rs:322`、`room/backfill.rs:102` | db_tests 多处 |
| `get_missing_events_between`（`dag.rs:42`） | `federation/events.rs:66` | db_tests 多处 |
| **`get_prev_state_events`（`dag.rs:179`）** | **0**（新发现） | — |
| **`find_events_referencing_missing_state`（`dag.rs:237`）** | **0**（新发现；其注释同样声称被 `/get_missing_events` 使用） | — |

旁证：`grep -rn "prev_state_events\|state_dag" synapse-web/src/routes/federation/` → **0**，
即 `/send_join` 压根没消费 state DAG。同类事实：`create_state_event_with_dag`（MSC4242 写路径）
**生产调用点也是 0**（只有 `pdu.rs:16`、`models.rs:106` 的注释提到它）。

---

### 1.5 🔴 `msc2965/auth_issuer` 仍在册 → **存在**

- 注册：`synapse-web/src/routes/assembly.rs:196-199`（路径 `:197`，handler `:198`）；
  派生表 `derived_route_table_always.inc.rs:114`。**无** `/v1/auth_issuer`。
- `auth_metadata` **必须保留**：unstable `assembly.rs:193` + **stable v1** `assembly.rs:202`，
  同一 handler `auth_discovery.rs:47`。
- 上游证据（主源）：Synapse `release-v1.161/CHANGES.md:48` ——
  "Drop `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer` endpoint which never ended up being used. (#20163)"；
  上游 **只删 issuer**，`auth_metadata` 未删 ⇒ 文档 §22.3 的"口径收窄"警告正确，照做。
- 删除代价极低：唯一消费者是 handler `get_auth_issuer`（`auth_discovery.rs:62`）；
  `oidc_available`/`build_oidc_discovery` 仍被 `get_auth_metadata` 使用，不会连带死。
  需同步：`tests/integration/api_auth_routes_tests.rs:247-254` + 全部派生产物（见 §4.5）。

---

### 1.6 🔴🟠 Profile：文档两条都成立，**且真实缺口比文档大**（缺整个 v1.16 稳定面）

**子项 A（停用用户写自定义字段应成功）— 成立**：
`synapse-storage/src/user/storage.rs:698`：

```sql
SELECT 1 AS "exists!" FROM users WHERE user_id = $1 AND is_deactivated = FALSE LIMIT 1
```

→ `user_exists` = "存在**且未停用**"。调用链
`handlers/extended_profile.rs:30-38 ensure_extended_profile_user_exists` →
`account_identity_service.rs:70-73` → `user_service.rs:112-116` → 上述 SQL；`:124` 在**所有权检查之前**就 404。
上游主源确认：`release-v1.161/CHANGES.md:36`（#20172）：

> "this now **succeeds for existing (e.g. deactivated) users** and returns a 404 error if the user does not exist"

**注意**：`user_exists` 有 **14 处生产调用点**（`membership/actions.rs:63`、`moderation.rs:37/183/352`、
`federation/edu.rs:238`、`auth_compat.rs:118`、`admin/room/management.rs:380/407/440/496/525` …）
⇒ **绝不能直接摘掉 SQL 里的 `AND is_deactivated = FALSE`**（会把停用判定泄漏到鉴权/成员路径），
必须**新增**一个"包含停用"的存在性查询，只给 profile 写路径用。

**子项 B（稳定 `/{keyName}` 未注册）— 成立，且已升级为规范缺口**：
- 仓库只声明 capability `m.profile_fields`（`synapse-services/src/capability_governance.rs:504-507`，属 `/capabilities`），
  `/versions` 只声明未稳定 flag `uk.tcpip.msc4133`（`capability_governance.rs:116`）；
  **稳定路由只到** `profile/{user_id}`、`/displayname`、`/avatar_url`（`assembly.rs:376-378`），
  泛化 `{key_name}` 只在 unstable（`assembly.rs:228-231`；派生表 `:269/:277/:285`）。
- **主源**：matrix-spec `data/api/client-server/profile.yaml:19` 已有
  `"/profile/{userId}/{keyName}"`，`:22` 明写 `x-changedInMatrixVersion: "1.16"`，
  即 **MSC4133 已进规范 v1.16**（PUT/GET/DELETE 三法），`keyName` 允许 `avatar_url|displayname|m.tz|自定义命名空间`
  （`:52-53`），并定义 `M_PROFILE_TOO_LARGE`（`:104-106`）、`M_KEY_TOO_LARGE`（`:106`）、
  总 profile 上限 **64 KiB**（`:27`）、`GET /profile/{userId}` 响应新增 `m.tz` 与 additionalProperties（`:305-320`）。
  ⚠️ **口径纠正**：MSC4133 提案文本提到 `/profile/{userId}` 的 `field=` 查询参数，
  但**合并进 spec 的版本里没有它**（`profile.yaml` 该端点只有 `userId` 路径参数）⇒ 本仓不存在该缺口，
  不得据提案文本立项。
- **仓库 0 命中**：`M_PROFILE_TOO_LARGE`、`M_KEY_TOO_LARGE`、`"m.tz"`、`GET /profile` 的 `field=`。
- **`/versions` 只到 v1.14**（`capability_governance.rs:79-104`）⇒ 声明 `m.profile_fields`
  （v1.16 capability）**同时**不声明 v1.16，客户端会得到自相矛盾的信号。
- 另：稳定 `PUT /displayname`、`/avatar_url` 也走 `ensure_active_user_exists`（`account_compat.rs:229,225` 一线），
  与 GET（对停用用户 200）**前后不一致**。

**结论**：本项的正确修法不是"二选一"这么简单，而是三选一（见 §3.1 T3.1）。

---

### 1.7 🔴 Admin 媒体端点族缺口 → **存在；文档"上游 18"不可复现，本轮给出可核验的 15 条**

**本仓实测**（`admin/media.rs:16-22`，`grep -c '"admin::media"'` = 7）：

| # | 本仓路由 |
|---|---|
| 1 | `GET /_synapse/admin/v1/media`（全局列举，**上游无此条**） |
| 2 | `GET /_synapse/admin/v1/media/{media_id}` |
| 3 | `DELETE /_synapse/admin/v1/media/{media_id}` |
| 4 | `GET /_synapse/admin/v1/media/quota`（**上游无此条**） |
| 5 | `GET /_synapse/admin/v1/users/{user_id}/media` ✅ 与上游一致 |
| 6 | `DELETE /_synapse/admin/v1/users/{user_id}/media` ✅ 与上游一致 |
| 7 | `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes`（**形状与上游不同**） |

**上游主源**（`element-hq/synapse@release-v1.161/docs/admin_api/media_admin_api.md`，逐行实测）+ 交叉引用的
`user_admin_api.md:756,883`，共 **15** 条：

```bash
curl -sS https://raw.githubusercontent.com/element-hq/synapse/release-v1.161/docs/admin_api/media_admin_api.md \
  | grep -nE "^(GET|POST|DELETE) "
# 13 条（见下），另 2 条在 user_admin_api.md（GET/DELETE users/<user_id>/media，本仓已有）
```

| 上游端点（行号 = media_admin_api.md） | 本仓 | 缺口类型 |
|---|---|---|
| `GET /room/{room_id}/media`（:18） | ✗ | **真缺口**（房间级列举） |
| `GET /media/{origin}/{media_id}`（:49） | 有 `media/{media_id}` | **形状偏差**（缺 `{origin}` 段） |
| `POST /media/quarantine/{server_name}/{media_id}`（:112） | ✗ | **真缺口**（鉴权白名单 `admin_auth.rs:386` 已预留前缀） |
| `POST /media/unquarantine/{server_name}/{media_id}`（:133） | ✗ | **真缺口** |
| `POST /room/{room_id}/media/quarantine`（:154） | ✗ | **真缺口**（房间级隔离） |
| `POST /user/{user_id}/media/quarantine`（:186） | ✗ | **真缺口**（用户级隔离） |
| `POST /media/protect/{media_id}`（:217） | ✗ | **真缺口**（防隔离保护） |
| `POST /media/unprotect/{media_id}`（:237） | ✗ | **真缺口** |
| `GET /media/quarantine_changes?from=`（:267） | 有 `quarantine_media/{media_id}/changes` | **形状偏差**（上游是全局 + `from` 游标） |
| `DELETE /media/{server_name}/{media_id}`（:300） | 有 `DELETE media/{media_id}` | **形状偏差**（缺 `{server_name}` 段） |
| `POST /media/delete?before_ts=`（:331） | ✗ | **真缺口**（按时间批量删） |
| `POST /media/{server_name}/delete?before_ts=`（:339） | ✗ | **真缺口** |
| `POST /purge_media_cache?before_ts=`（:385） | ✗ | **真缺口**（远端缓存清理） |
| `GET/DELETE /users/{user_id}/media`（user_admin_api.md:756,883） | ✅ | 一致 |

**已具备但无路由的能力**（说明缺口是"没接线"而非"没能力"）：
`synapse-services/src/media/mod.rs:119 quarantine_media`、`:155 unquarantine_media`（仅测试调用 `:1071`）；
`synapse-storage/src/media/quarantine_stream.rs:14,42,52,74,154,180`；
`container.rs:490-493` 已装配 `with_quarantine_stream`；但 `admin_media_service.rs:12-85` 只暴露
`get_media_quarantine_changes`，**没有** quarantine/unquarantine 动作方法。

**文档"上游 18"**：唯一来源是 `API_COVERAGE_REPORT.md`（HEAD:137 / 工作树:155）的一个裸数字，
该表自带 `[人工口径·未机器复核]` 标注，无明细 ⇒ 不可核验。本轮以主源替换为 **15**。

---

### 1.8 🔴 `animated` + 媒体配额错误码 → **两项均存在**

**`animated`（未支持）**：
- 全仓 `grep -rin animated --include=*.rs`（排除 `.worktrees/target`）= **0 命中**。
- 参数解析：`routes/media/download.rs:263-268 thumbnail_request_params` 只读 `width/height/method`；
  handler `:440/:453` 用 `Query<Value>`；服务签名（`media/mod.rs:511-518`、`media_service.rs:405-412`）无该参数。
- **主源**：matrix-spec `data/api/client-server/content-repo.yaml:436-453` 定义 `animated` 查询参数，语义明确：
  "`true` → SHOULD 返回动画缩略图；`false` → MUST NOT；`true` 但素材无法动画（JPEG/PDF）→ SHOULD 视作 `false`"，
  响应体 `:497-502` 还区分了 `thumbnail`/`animated thumbnail` 两种 key。

**配额错误码（归因待议，但字面成立）**：
- 强制存在：`media/mod.rs:246-256 ensure_upload_allowed` → `:247 check_upload_quota` →
  `media_quota_service.rs:19-68`；`:250-252` 超限返回 `ApiError::bad_request(...)`。
- 实测响应：`ApiError::bad_request`（`error.rs:180-182`）→ kind=BadRequest → **HTTP 400 + errcode `M_BAD_JSON`**
  （`error.rs:63,822-824,858-864`；测试固化于 `media/mod.rs:893,950`）。
- `M_USER_LIMIT_EXCEEDED`：注册于 `error/code.rs:83-84`，注释明写 "**user account limit (MSC4335)**"，
  `:176` 映射 429；**生产使用点 0**（全仓只有注册表与测试）。
- 可用的候选码都已存在：`M_TOO_LARGE` 413（`code.rs:71,169`）、`M_RESOURCE_LIMIT_EXCEEDED` 403（`:75,171`）、
  `M_LIMIT_EXCEEDED` 429（`:27,147`）⇒ **不需要新造 errcode**。
- 结论：文档 §22.3 "该期望本身需要决策"的判断**正确**，本轮把它落成明确建议（见 §3.3 T3.4）。

---

### 1.9 ⚪ v12/v13 不可创建 → **设计取舍，应从"问题清单"移出**

`room_versions.rs:110-112` 注释自述"尚未完整实现 v12/v13 认证规则，降级为 parse+join+federate-only
可用，避免创建无法产生合规 PDU 的房间（fail-safe）"；`:114-115 stable_parse_only("12"|"13")`，
`stable_parse_only` ⇒ `can_create:false, can_join:true`（`:58-63`）；
`client_room_versions_capability:154-167` 过滤 `can_create` ⇒ `m.room_versions` 里不出现 v12/v13，
`/versions` 也不含房间版本。测试已钉住（`:239-241,:255-257`）。

**这不是缺陷**，是与本仓 PDU 能力（见 §2.1）一致的自觉取舍。**建议**：移入"已知取舍"章节，
并补一条守卫测试（断言 `can_create == false` 的理由常量存在），防止有人"顺手打开"。

---

### 1.10 🔴 `search_index` 遗留表 → **存在（+ 新的派生物漂移）**

- 工作树 baseline `:2750-2761` 仍 `CREATE TABLE IF NOT EXISTS search_index`，4 条索引
  `:3735-3738`（含 GIN `gin_trgm_ops`）；HEAD 行号为 `:2839`/`:3840-3843`（§22 引用的是 HEAD）。
- `synapse-storage/src/search_index.rs` **不存在**；生产 SQL 对 `search_index` 的引用 **0**
  （只有 `tests/integration/schema_contract_p0_tests_migrated.rs:336,1177-1312`）。
- 事件搜索走 `events` 表（`synapse-storage/src/event/search.rs`）；`config.search.search_index_name`
  是 Elasticsearch 索引名，与本表无关。
- **新发现（N-7，低）**：`migrations/INDEXES.md` 列出的 `idx_search_index_*` 条目数为 **0**，
  而 baseline 实际创建 4 条 ⇒ **索引文档与 baseline 已经不一致**（`INDEXES.md` 是人工维护的派生物）。
- **新发现（N-8，低，属铁律 8 同类）**：CI 契约 `scripts/check_schema_contract_coverage.py:177-194`
  只列了 **3** 条索引（`room/user/type`），漏了 GIN 的 `idx_search_index_content_trgm`
  （baseline `:3735`）；而该脚本的校验是**单向**的（`:302-305` 只检查"契约里写的索引是否存在"），
  **不会**发现"baseline 有而契约没写"⇒ 这个方向的门禁永远绿。
- 另有 CI 契约仍钉死该表：`scripts/check_schema_contract_coverage.py:177-194 TABLE_CONTRACTS["search_index"]`
  （被 `ci.yml:120`、`db-migration-gate.yml:73` 调用）。
- 删除需同步 **baseline 指纹**（硬规则，见 MEMORY/`test_isolation_unification_tests.rs:120`）。

---

### 1.11 🟡 ledger `query_params` → **部分证伪**

- **字段与 builder 确实是死的**：`route_ledger.rs:83-84 pub query_params`、`:106-110 with_query_params`，
  全仓调用点 **0**（只有定义）。
- **但"无消费方"不成立**：`ledger_export.rs:156` 把它序列化进导出产物，
  且 **6 份 fixture 逐字节钉住** 该字段（`tests/unit/ledger_export_tests.rs:136-179` 的
  `assert_fixture_matches` + `tests/unit/fixtures/ledger_export*/**.json`，
  当前 1033–1135 条全为 `[]`）；`tests/unit/placeholder_scan_tests.rs:224-255` 还钉了字面量
  `"query_params": []`；下游 `scripts/api_test/gen_route_table.py:64` 读取它并写入
  `docs/openapi/route-table.json`。
- **真实缺陷**是"**没有任何路径能产出非空值**"（字段+builder 是死代码），而不是"无消费方"。
- 决策影响契约：删除该字段会改动导出 schema ⇒ 需同步 `SCHEMA_VERSION`（`ledger_export.rs:45` = `"4"`）
  与 SDK lane pin（`LEDGER_SCHEMA_VERSION`）。

---

## 2. P0 残余复核（§22.2 的三条 + 本轮新增两条）

### 2.1 本地事件不落图元数据 → **成立**（并升级为两个独立缺陷）

**残余 ①：`create_event` 不写 `depth`/`prev_events`/`auth_events`/`origin`** —— 成立。
`synapse-storage/src/event/create.rs:14-15`（INSERT 列清单）对比 `:83-84`
（`create_event_with_graph` **有**这三列）。表结构允许（baseline `events.depth/prev_events/auth_events`
均为 NULL 列，`:343-345`）。

**影响链（实测，非推测）**：
- 建房走 `create_event`：`synapse-services/src/room/lifecycle/create.rs:76,116,167,188,216,238,308,350,382`
  ⇒ **本地新建房间的全部状态事件图元数据为 NULL**。
- `/send_join` 的 state/auth_chain 由 `routes/federation/pdu.rs::state_pdu` 投影，
  遇到 NULL 返回 `PduCompleteness::MissingGraphMetadata`（`:119-129`），
  `build_pdus` 遂走 `SignatureAction::RefuseIncomplete`（`:158-160`），
  **故意不发签名**（`:223-231`）。
- 结论：**远端服务器加入本仓本地创建的房间时，拿到的是一组无签名的 state PDU**，
  无法验签 ⇒ 联邦加入路径实质不可用。这不是"字段不齐"的美化描述，是 P0。

**残余 ②：入站 `signatures` 部分路径未覆盖** —— 成立（口径与 §22.2 一致）。
`/send` transaction 走 `create_event_with_graph`（`federation/transaction.rs:450,501`），
其 INSERT **不含** `signatures`/`hashes`；只有入站成员路径回填
（`federation/membership/mod.rs:238 → messaging/events.rs:507 → event/signature.rs:10`）。
⇒ 非成员入站事件再被转发时只剩本机签名。

**残余 ③：`event_id` 非 reference hash** —— 成立。
`synapse-common/src/crypto.rs:149-153`：

```rust
pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = current_timestamp_millis();
    let mut bytes = [0u8; 18];
    rand::rng().fill_bytes(&mut bytes);
    format!("${}${}:{}", timestamp, URL_SAFE_NO_PAD.encode(bytes), server_name)
}
```

即 `$<ts>$<b64>:<server>`（§22.1 N-5 的口径修正**正确**）。
调用点 >40 处（`lifecycle/create.rs`、`membership/actions.rs`、`handlers/room/{events,state}.rs` …）。
v4+ 房间要求 `event_id = "$" + unpadded_base64(reference_hash)`，故 v11 对等端无法把本仓 PDU
当规范事件接受 ⇒ **独立于字段完备性的语义缺口**。

### 2.2 **新发现 N-1（P0）：出站 `/send` PDU 缺 `depth` 与 `auth_events`**

本地发消息后广播给远端的 PDU 在 `synapse-services/src/room/messaging/service.rs:157-167` 拼装，
**只有 9 个键**：`event_id/room_id/sender/user_id/type/content/origin_server_ts/origin/prev_events`
（+ 可选 `state_key`、`redacts`）。**没有 `depth`、没有 `auth_events`**。

对照仓库自己的规范纪律（`pdu.rs:19-23`）：

> "Substituting would be actively harmful: a peer that trusts a fabricated `prev_events: []`
> files the event as a DAG root and corrupts its own room graph."

即：**入站投影路径严格拒绝伪造图元数据，出站广播路径却直接省略两个必填键**——
两者对同一问题的处理自相矛盾，且出站那条是每个本地消息都走的路。

### 2.3 **新发现 N-2（高）：`sign_and_broadcast_event` 两份实现、策略相反**

| | `messaging/service.rs:131` | `membership/service.rs:486` |
|---|---|---|
| `prev_events` 取不到时 | **fail-closed**：`return Ok(())`，**不广播**（`:142-150`） | **fail-open**：`Vec::new()` 继续广播（`:496-505`），注释自述 "PDU may be incomplete" |
| `redacts` 放置 | 走 `apply_redacts`（`:174`），room-version 感知（v11+ 不写顶层） | **无条件**写顶层 `redacts`（`:531-533`） |
| `depth`/`auth_events` | 无 | 无 |

调用方：messaging 版被 `messaging/events.rs:204`、`messaging/messages.rs:241` 使用；
membership 版被 `membership/actions.rs:140,215,436`、`membership/moderation.rs:126,248,321,419` 使用。

这同时违反**反冗余铁律 2**（同一职责两份实现）与 pdu.rs 自述的安全立场（fail-open + 空 `prev_events`
正是"污染对端房间图"的那条路）。修复方式应是**收敛为一份**，而不是分别打补丁。

### 2.4 **新发现 N-3（中）：注释幻觉两处（与 §1.4 同类）**

1. `synapse-storage/src/event/create.rs:51-58` 注释称：
   "Callers without graph data … can continue to use `create_event`, **which delegates here** with
   empty arrays and depth 0." —— 实测 `create_event`（`:8-49`）**有自己的 INSERT，从不调用**
   `create_event_with_graph`。两者行为差异实质：前者图列为 **NULL**，后者为 `[]`/`0`；
   `pdu.rs:119` 的判定恰好区分 NULL 与 `[]`（`event_id_array` 对 NULL 与非数组都返回 None，
   但空数组会返回 `Some(vec![])` ⇒ **`[]` 会被判为 Complete 并签名**）。注释若被采信会导致
   错误实现（把本地事件改成"委托 + 空数组"，等于伪造 DAG 根）。
2. `synapse-web/src/routes/federation/pdu.rs:37` 仍写 `$<ts>_<rand>:<server>`
   （实际 `$<ts>$<b64>:<server>`，`crypto.rs:153`）—— §22.1 N-5 已修正文档，**代码注释未同步**。

### 2.5 **新发现 N-5（中）：MSC4133 已进规范 v1.16，本仓只到 v1.14**

见 §1.6 子项 B。要点：`profile.yaml:22` 明写 "1.16"，本仓 `/versions` 止于 v1.14
（`capability_governance.rs:79-104`），却声明 v1.16 的 capability `m.profile_fields`。

---

## 3. 优化方案

> **For agentic workers:** 本节是可直接执行的实施计划。若由 agent 执行，建议用
> `superpowers:subagent-driven-development`（每任务一个 fresh subagent + 任务间复核）。
> 步骤用 `- [ ]` 勾选跟踪。**禁止**在执行本计划时改动其他会话在途的 staged 文件（见 §3.0）。

**Goal:** 消除 §1/§2 中确认存在的缺陷，优先恢复联邦事件的规范可接受性（P0），其次补齐
MSC3912 客户端级联与 Content Scanner 接线（高），最后清理中低项与文档幻觉。

**Architecture:** 三类改动：①**写入路径统一**——把本地事件的图元数据（`depth`/`prev_events`/
`auth_events`）在创建期计算并落库，让出站广播与 `/send_join` 投影共用同一份真相；
②**同职责收敛**——两份 `sign_and_broadcast_event` 合成一份，扫面上传/发消息经单一钩子接入；
③**规范面补齐**——按 spec v1.16 / MSC3912 原文补路由与字段，同时删除上游已删面。

**Tech Stack:** Rust（axum / sqlx / tokio）、PostgreSQL 18（JSONB + GIN）、nextest、insta。

> **勾选口径（2026-09-25 起）**：`- [x]` = **已完成**，行内跟*证据锚点*（提交号或 `路径:行号`/实测命令）；
> `- [x] ~~原文~~ → 说明` = 该步骤被**替代或作废**（原文保留供追溯，不再是待办）；
> `- [ ]` = 未完成；行尾 `（待核）` = 尚无实测证据。勾选状态一律由 §0.3 / §6 的实测结论派生，
> **不得**凭计划文本自行打勾；回勾与实测证据在同一次提交里。
>
### Global Constraints

- 项目状态：**未发布、无外部用户、无生产数据 ⇒ 无兼容义务**（AGENTS.md 铁律 1）。
  不得为"兼容旧行为"保留双实现/旧字段；直接替换。
- **同一工作树同一时刻只允许一个写者**（铁律 9）。当前工作树有另一会话的 **70 个 staged 文件**
  （含 §23 E2EE 删除面）。执行者必须先 `git status --short` 确认，且**只 `git add` 逐路径**，
  禁止 `git add -A` / `git add .`。
- 提交前必过：`cargo fmt --all` + `./scripts/check_fmt_ratchet.sh`（baseline 0，`current>0` 与
  `current<0` 都失败）、`SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils
  --all-features --locked -- -D warnings`、`cargo test --workspace --all-features --locked`
  （`--workspace` 不可省）。
- 迁移只有一个真相源 `migrations/`；改 baseline 必须同步
  `tests/unit/test_isolation_unification_tests.rs:120 EXPECTED_BASELINE_FINGERPRINT`。
- **门禁必须自证能变红**（铁律 8）：本计划每个新增守卫测试都要附**变异自证**步骤。
- 每个任务结束时：`git diff --cached --stat` 复核 → 提交 → `git status --short` 确认未带走他人改动。

---

### 3.0 批次 B0：前置收尾（不写业务代码，先让基线一致）

| 任务 | 动作 | 验收 |
|---|---|---|
| **T0.1** | 由**在途会话的 owner** 把 §23 E2EE 删除面提交入库（本计划执行者**不得**代提交） | `git status --short` 中 `D synapse-e2ee/src/{verification,device_trust}/*`、`D .../verification_routes.rs` 消失；`cargo check -p synapse-web --locked` exit 0 |
| **T0.2** | 修正 `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`：§0.2 的第 3 条（SAS）标注 **⚪ 对象消失（见 §23）**，§22.3 的 SAS 表头加"整表作废"；§22.1 N-5 的 `$<ts>$<b64>` 口径回填到 §21.1 | `grep -n "SAS" PROJECT_REMAINING_ISSUES_2026-09-14.md` 中不再有"当前须处理"表述；不改动该文件的其他在途 hunk（该文件已 staged，**必须由 owner 合并**） |
| **T0.3** | 修正 N-6：`synapse-web/src/routes/federation/pdu.rs:37` 注释 `$<ts>_<rand>:<server>` → `$<ts>$<base64>:<server>` | `cargo fmt --all`；`./scripts/check_fmt_ratchet.sh` → `OK (0)` |
| **T0.4** | 修正 N-3①：`synapse-storage/src/event/create.rs:51-58` 注释删除"delegates here with empty arrays and depth 0"，改为事实描述（"`create_event` writes NULL graph columns; `create_event_with_graph` writes `[]`/`0`"） | 注释与 `:14`/`:83` 的 INSERT 逐字一致；无测试影响 |

### 3.1 批次 B1（P0）：联邦写入/广播路径的图元数据

**背景**：`/send_join` 无签名（§2.1残余①）与出站 PDU 缺 `depth`/`auth_events`（N-1）**同根**：
本地事件的图元数据从未被计算与持久化。因此**一个任务解决两个缺陷**。

#### Task 1：在创建期计算并落库 `depth`/`prev_events`/`auth_events`

> **⚠️ 执行修订（2026-09-25，实施中实测）** —— 原设计（给 `CreateEventParams` 加 3 个字段）
> 经实测有**两个问题**，已按下方新设计执行：
>
> 1. **爆炸半径**：`CreateEventParams {` 字面量全仓 **160 处**（`grep -rn "CreateEventParams {"`），
>    加必填字段要改 160 处（含 46 处 storage db_tests、15 处 test_mocks）——纯机械 churn，
>    且每处都要回答"该不该有图数据"，反而更容易错。
> 2. **缺少前置件**：`auth_events` 的正确性依赖规范的 **Auth events selection** 算法，
>    而全仓**没有任何实现**（`grep -rn "auth_events" synapse-services/src/room | grep fn` 为空）。
>    不先补它就无法产出合法 `auth_events`。
>
> **修订后的设计（三步，可分别提交）**：
>
> - **1a（已完成）** 新增 `synapse-services/src/room/state/auth_events.rs`：纯函数
>   `auth_types_for_event` / `select_auth_events` + `AuthStateSnapshot`。
>   算法与上游 Synapse `synapse/event_auth.py::auth_types_for_event`（release-v1.161）逐条对齐，
>   含 v9 无 restricted join rule、v10/v11 有。9 个已知答案单测（无需 DB）+ 3 个变异自证。
> - **1b（已完成，提交见分支）** 写入侧单一拦截点：`NotifyingEventWriter` 的模块文档已自证
>   "**Every service (messaging, membership, lifecycle, moderation, federation backfill) persists
>   through `Arc<dyn EventWriter>`**"（`synapse-services/src/notifying_event_writer.rs:11-13`）。
>   因此不改 160 处调用点，而是在该 seam 上加一层 `GraphMetadataWriter` 装饰器
>   （`synapse-services/src/graph_metadata.rs`）：
>   `create_event` → resolver 算 `(prev_events, auth_events, depth)` → 转调
>   `inner.create_event_with_graph(...)`；`create_event_with_graph` **原样透传**
>   （入站/backfill/建房已有图数据，不得被重算覆盖）。
>   依赖用**窄接口** `GraphMetadataSource`（4 个方法：forward_extremities / event_depths /
>   state_events / room_version）+ `StorageGraphMetadataSource` 适配真实
>   `Arc<dyn EventReader>` + `Arc<dyn RoomStoreApi>`；`depth` 走 `get_events_map`，**无需新增查询**。
>   装配点唯一（`wiring/rooms.rs`，Graph 在外、Notifying 在内，两条路径都仍会唤醒 sync）。
>   **失败策略 fail-closed**：房版本读不到 / 前驱事件读不到 / 无前驱 ⇒ 拒绝写入并返回错误，
>   绝不伪造 `[]`/`0`（与 `pdu.rs` 的立场一致）。
>   **事务边界（重要）**：装饰器**只在 `tx.is_none()` 时解析** —— 事务内的读看不到未提交行，
>   强行解析会产出错误的 `auth_events`/`prev_events`。事务调用方（建房批量写初始状态）
>   走原行为，由 1e 显式提供图数据。
>   8 个单测（无需 DB）+ 3 个变异自证（深度算术 / 自环过滤 / 装饰器改走 plain write ⇒ 各自转红）。
> - **1e（已完成，提交见分支）** 建房路径显式提供图数据：新增
>   `synapse-services/src/room/lifecycle/creation_graph.rs` 的 `CreationGraph`
>   （线性序列追踪：`prev_events` = 上一个事件、`depth` 递增、`auth_events` 走 1a 的选择算法，
>   且在**记录自身之前**做选择 ⇒ 事件永不自我授权），并在 `create_events.rs` 加
>   `write_creation_event(...)` 单一写法；`create.rs` 的 9 处 + `set_room_metadata` 2 处 +
>   `process_invites` 1 处全部由 `create_event` 改为 `create_event_with_graph`。
>   6 个纯单测 + 3 个变异自证（深度不递增 / 先记账再选 auth ⇒ 自授权 / prev 恒空 ⇒ 各自转红）。
>   **这一步才让新房间的 state 具备可签名条件**（装饰器在事务内不解析）。
> - **1d（已完成，提交见分支）** 出站 PDU 收敛为**一份**实现并补 `depth`/`auth_events`：
>   新增 `synapse-services/src/room/federation_broadcast.rs`（`build_broadcast_pdu` 纯函数 +
>   `sign_and_broadcast_event(ctx, event)`），`messaging/service.rs` 与 `membership/service.rs`
>   各剩一个薄适配器（各自从字段拼 `BroadcastContext`）。后者原先是 **fail-open**（取不到
>   extremities 就用 `prev_events: []` 广播）且 `redacts` 不看 room version —— 正是 `pdu.rs`
>   文档判为"污染对端房间图"的写法，现已与 messaging 侧统一为 **fail-closed**。
>   图字段**从刚落库的行读回**（`EventReader::get_event_graph_fields`），不再用"最新事件"查询
>   冒充 extremities。`event_id_array` 提升到 `synapse-common/src/event_utils.rs` 单实现，
>   `pdu.rs` 改用它（此前 web/services 两侧各有一份）。
>   6 个纯单测 + 3 个变异自证（PDU 省 depth / 缺 depth 静默写 0 / 缺数组静默写 `[]`）。
> - **1c（待做）** storage 单写入口收敛：`create_event` 与 `create_event_with_graph` 目前是
>   **两条独立 INSERT**；抽成一个私有 helper，`create_event` 传 `None`（写 SQL `NULL`），
>   `create_event_with_graph` 传值，**公开签名不变**（避免 1a 之外的第二波 churn）。

**原设计（保留作对照，勿照抄）**：

**Files:**
- Modify: `synapse-storage/src/event/models.rs`（`CreateEventParams` 增加三个可选图字段）
- Modify: `synapse-storage/src/event/create.rs:8-49`（`create_event` 写这四列；统一走一条 INSERT）
- Modify: `synapse-services/src/room/messaging/events.rs`（`RoomMessagingService::create_event` 统一计算）
- Modify: `synapse-services/src/room/messaging/service.rs:131-214`（广播复用已落库图字段，删除二次查询）
- Modify: `synapse-services/src/room/membership/service.rs:486-560`（删除本文件实现，改调 messaging 版）
- Test: `synapse-storage/src/event/db_tests.rs`、`tests/unit/federation_state_pdu_tests.rs`

**Interfaces:**
- Produces: `CreateEventParams { depth: Option<i64>, prev_events: Option<Vec<String>>, auth_events: Option<Vec<String>>, .. }`
- Produces: `EventStorage::create_event` 落库图列（NULL 仅当调用方显式不传）
- Produces（合并后唯一实现）：`RoomMessagingService::sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()>`

- [x] ~~**Step 1: 写失败测试（RED 1 —— 落库）**~~ — **作废**：原设计要求改 `CreateEventParams`（全仓 160 处字面量），已否决；等价覆盖见 1b 装饰器与 1e 建房追踪的 DB/单测

`synapse-storage/src/event/db_tests.rs` 新增：

```rust
#[tokio::test]
async fn test_create_event_persists_graph_metadata() {
    let pool = setup_test_pool().await;
    let storage = EventStorage::new(pool.clone());
    let params = CreateEventParams {
        event_id: "$graph1:example.com".into(),
        room_id: "!r:example.com".into(),
        user_id: "@a:example.com".into(),
        event_type: "m.room.message".into(),
        content: serde_json::json!({"body": "x"}),
        state_key: None,
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
        depth: Some(7),
        prev_events: Some(vec!["$p1:example.com".into()]),
        auth_events: Some(vec!["$a1:example.com".into()]),
    };
    storage.create_event(params, None).await.expect("create_event");
    let row: (Option<i64>, Option<serde_json::Value>, Option<serde_json::Value>) = sqlx::query_as(
        "SELECT depth, prev_events, auth_events FROM events WHERE event_id = '$graph1:example.com'",
    ).fetch_one(&*pool).await.expect("row");
    assert_eq!(row.0, Some(7));
    assert_eq!(row.1, Some(serde_json::json!(["$p1:example.com"])));
    assert_eq!(row.2, Some(serde_json::json!(["$a1:example.com"])));
}
```

- [x] ~~**Step 2: 运行并确认失败**~~ — **作废**（随原设计一并替代）

Run: `SQLX_OFFLINE=false cargo nextest run -p synapse-storage --lib -E 'test(test_create_event_persists_graph_metadata)'`
Expected: 编译失败（`CreateEventParams` 无 `depth` 字段）。

- [x] ~~**Step 3: 实现（GREEN 1）**~~ — **作废**：实现改为「图数据走 `create_event_with_graph` + 自动提交路径用 `GraphMetadataWriter` 装饰」（`bf90f430f` / `900938510`）

`CreateEventParams` 加三个字段；`create_event` 的 INSERT 改为与 `create_event_with_graph`
**同一条语句**（删掉 `create_event_with_graph` 的重复 INSERT，只保留一个写入口）：

```sql
INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key,
                    origin_server_ts, is_redacted, redacts, depth, prev_events, auth_events, origin)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,false,$9,$10,$11,$12,$13)
```

并在 `create_event_with_graph` 内改为构造带图字段的 `CreateEventParams` 后调用 `create_event`
（**消除第二份 INSERT**，满足铁律 2）。

- [x] ~~**Step 4: 运行确认通过**~~ — **作废**；等价门禁：1b 8 单测+3 变异、1e 6 单测+3 变异、storage 极值 DB 测试+变异

Run: `SQLX_OFFLINE=true cargo nextest run -p synapse-storage --lib -E 'test(graph_metadata)'`
Expected: PASS。

- [x] **Step 5: 写失败测试（RED 2 —— 出站 PDU 字段完备）** — 已完成（1d，`864d0f6b2`）：`room/federation_broadcast.rs::build_broadcast_pdu` 的 12 键单测

`tests/unit/federation_state_pdu_tests.rs` 同级新增 `tests/unit/federation_outbound_pdu_tests.rs`：

```rust
#[test]
fn outbound_pdu_includes_depth_and_auth_events() {
    let event = room_event_fixture_with_graph(/*depth=*/ 7, /*prev=*/ &["$p"], /*auth=*/ &["$a"]);
    let pdu = build_outbound_pdu("example.com", &event);
    assert_eq!(pdu["depth"], serde_json::json!(7));
    assert_eq!(pdu["prev_events"], serde_json::json!(["$p"]));
    assert_eq!(pdu["auth_events"], serde_json::json!(["$a"]));
    // 反向断言：图元数据缺失时**不得**伪造（与 pdu.rs 的立场一致）
    let incomplete = room_event_fixture_without_graph();
    assert!(build_outbound_pdu("example.com", &incomplete).get("depth").is_none());
}
```

（实现时把 `service.rs` 里的 PDU 拼装抽成 `pub(crate) fn build_outbound_pdu(server_name, &RoomEvent) -> Value`，
便于单测；这是本次重构的关键 seam。）

- [x] **Step 6: 实现（GREEN 2）** — 已完成（1d）：字段取自**已落库**图元数据而非重查 extremities；缺失即拒签拒播（fail-closed）
  `depth`/`prev_events`/`auth_events`（Task 1 Step 3 已保证有值），**取不到时按 `pdu.rs` 的同一立场处理**：
  不伪造，记 `federation_pdu_incomplete_total` 计数并跳过广播（与 messaging 版现有 fail-closed 一致）。

- [x] **Step 7: 合并两份 `sign_and_broadcast_event`** — 已完成（1d）：`grep -rn "fn sign_and_broadcast_event"` = 3 处（1 实现 + 2 薄适配器）

删除 `synapse-services/src/room/membership/service.rs:486-560` 的整份实现，让
`membership/actions.rs`、`membership/moderation.rs` 的 8 个调用点改调 messaging 版
（若字段不可达，用 `RoomMessagingService` 的引用注入，不要复制代码）。
验收：`grep -rn "fn sign_and_broadcast_event" --include=*.rs .` → **恰好 1 处**。

- [x] **Step 8: 变异自证（铁律 8）** — 已完成：1d 三个变异（PDU 省 `depth` / 缺 `depth` 静默写 0 / 缺数组静默写 `[]`）各自转红，还原 sha256 一致

分别制造 3 个变异并确认对应测试转红、还原后 sha256 一致：
① 把 `create_event` 的 `depth` bind 改成常量 `0`；② 从 `build_outbound_pdu` 删掉 `auth_events`；
③ 把 `RefuseIncomplete` 分支改成 `SignLocally`。
每次：`cargo nextest run --test unit -E 'test(federation_)'` 读到 nextest 的 `Summary [` 行并断言有 failed。

- [x] **Step 9: 全量回归与提交** — 已完成：B1 冻结提交上 lib 6308/6308、unit 1773/1773、集成子集 15/15、clippy exit 0、fmt `OK (0)`

```bash
cargo fmt --all && ./scripts/check_fmt_ratchet.sh
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
cargo nextest run --workspace --lib --all-features --locked --test-threads 4
cargo nextest run --test unit --features test-utils --locked --test-threads 4
git add synapse-storage/src/event/create.rs synapse-storage/src/event/models.rs \
        synapse-services/src/room/messaging/events.rs synapse-services/src/room/messaging/service.rs \
        synapse-services/src/room/membership/service.rs synapse-services/src/room/membership/actions.rs \
        synapse-services/src/room/membership/moderation.rs tests/unit/federation_outbound_pdu_tests.rs
git diff --cached --stat && git commit -m "fix(federation): persist local event DAG metadata and emit complete outbound PDUs"
```

#### Task 2：入站非成员事件的 `signatures`/`hashes` 回填（§2.1 残余②）—— **已完成**

> **实施记录（2026-09-25）**：采用**既有机制**而非新增第二种写法 —— 入站 PDU 落库后调用
> 服务端已有的 `update_event_signatures_and_hashes`（本地签名路径与入站成员路径本就走它），
> 因此"签名材料怎么落库"仍然只有一份实现。
> 新增共享谓词 `synapse_common::event_utils::signature_material(hashes, signatures)`
> （两侧都要求对象且非空、`sha256` 非空字符串；仅一半材料一律拒绝 —— 半份材料描述的是
> 两个不同的字节序列），并让 `pdu.rs` 的 `stored_signature_material` 改为委托它
> （此前 web 侧另有一份同语义实现）。
> 接线两处非成员入站路径：`/_matrix/federation/v1/send`（`transaction.rs`）与
> backfill（`services/room/backfill.rs`），各自在落库成功后从收到的 PDU 提取并持久化**源服务器**的
> 签名/哈希 —— 此前只有本机签名，转发时对端会因缺少发送方签名而拒绝。
> 变异自证：删掉 `sha256` 非空校验 ⇒ 该用例转红；删掉"签名对象非空"校验 ⇒ 亦转红；
> 两次还原后 sha256 与原文一致。
> 证据：`signature_material` 单测（6 组不可用组合）、
> `inbound_pdu_signature_material_round_trips` DB 测试（落库 → 提取 → 持久化 → 读回，
> 从 `get_state_event` 拿到同一对，并再次通过谓词校验）、`federation_state_pdu` 9/9。
> **仍待补**：HTTP 层的端到端断言（发一条真实 `/send` 事务后查库）—— 现有
> `api_federation_transaction_tests::test_send_transaction_with_signed_pdu_accepted` 的
> 房间未预建、结果容忍 success/error 两种，需先建房再断言；已登记为后续项。

**原始计划（保留作对照）**：

- **Files:** `synapse-web/src/routes/federation/transaction.rs:440-510`、
  `synapse-services/src/room/messaging/events.rs:507`、`synapse-storage/src/event/create.rs`
- [ ] Step 1: RED —— 在 `tests/integration` 加用例（**未按原文做**：改用 storage 层回环测试 `inbound_pdu_signature_material_round_trips` + 谓词单测；**HTTP 端到端断言仍缺** → §5 U-8）
  断言 `events.signatures` 落库且 `/send_join` 再投影时 `signature_action == KeepStored`。
- [x] ~~Step 2: GREEN —— 把 PDU 的 `hashes`/`signatures` 作为参数传入 `create_event_with_graph`~~ → **改为复用既有 `update_event_signatures_and_hashes`**（不新增第二种写法；`efbe4af73`）
  并在 INSERT 中落库（与图元数据同一批字段），不再依赖事后 `update_event_signatures_and_hashes`。
- [x] ~~Step 3: 断言不再需要 `membership/mod.rs:238` 的专用回填~~ → **不适用**：该处回填写的是**本机**签名（F-03 本地签名），与源服务器材料是两码事，保留
- [x] Step 4: 变异自证 + 门禁同 Task 1 Step 8/9 — 已完成：`signature_material` 2 变异（删 `sha256` 校验 / 删空签名校验）各自转红；fmt/clippy 通过

#### Task 3（决策项，原"不建议本轮动手"）：`event_id` 改 v4+ reference hash —— 🟡 **第 1 步已完成**（`64ffc13a6`，尚未接线；main 仍为 `$<ts>$<b64>:<server>`）

> - [x] **第 1 步（纯函数 + 已知答案向量）** — ✅ 完成，见 §6.6"第 1 步执行结果"：`synapse-common/src/event_id.rs`
>   + `redaction::{redaction_rules, redact_event}`；上游 Synapse release-v1.161 的 v10/v3 两个已知答案
>   向量逐字节通过，24/24 绿，3 个变异自证均转红，fmt/clippy exit 0。
> - [ ] **第 2 步（接线 v4+）** — ⬜ 未开始；**前置**：既有非版本化 redaction 表迁移（铁律 2）、
>   `compute_event_content_hash` 先 redact 后哈希的语义修正（✅ 已完成 `0880f6a5f`）、v12 语义裁定（✅ MSC4304/MSC4291，见 §6.6）——原三项前置现只剩签名半边。
>   **接线破坏面（实测 2026-09-26）**：`generate_event_id` 生产调用点 **30** 处（另 2 处在 `#[cfg(test)]`），
>   分布 16 个文件：`room/membership/{actions,federation,moderation}.rs`（11）、
>   `routes/handlers/room/{state,events}.rs`（5）、`room/{service,lifecycle/create_events,messaging/*,state/info}.rs`（7）、
>   `relations_service.rs`（3）、`burn_after_read_service.rs`（2）、`friend_room_service/mod.rs`、
>   `routes/federation/{membership/invite,membership/knock,transaction}.rs`（3）。
>   复现命令：对每个命中取最内层 `fn`（脚本见本文件附录 A 的同类扫描式）。
>   **另有两个硬顺序约束（本轮新发现，决定第 2 步做法）**：
>   ① reference hash **包含 `hashes`**（redaction 保护 `hashes`，只去 `signatures`/`unsigned`/`age_ts`），
>      而本仓的 `hashes`/`signatures` 是在**落库之后**由 `update_event_signatures_and_hashes`
>      （见 `services/room/federation_broadcast.rs` 的 `sign_and_broadcast_event`）补写的；
>      因此"先生成 event_id 再补 hashes"必然得到与对等端重算不一致的 ID ⇒ 接线必须把
>      **event_id 赋值移到 `hashes` 之后**，即 B1 已建立的单写入口
>      `GraphMetadataWriter::create_event`（它已通过 `GraphMetadataSource::room_version` 拿到房间版本，
>      无需新增查询）需要同时负责"算 hashes → 算 reference hash → 定 event_id"，
>      或让 30 个调用点改为消费写入口返回的 ID。
>   ② 签名材料是否包含 `event_id` 必须对着上游 `EventBase.get_pdu_json()` 核实后再定
>      （v3+ 的联邦 PDU **不含** `event_id`，而本仓签名路径仍可能带上），否则第 3 步互操作门槛必失败。
>   **结论**：第 2 步不能按"逐调用点替换生成函数"做，必须按"单一写入口定 ID"做；
>   在该重构落地前，保持 v4+ 使用随机 ID 是**已知取舍**，不得对外声称 v12 事件可通过联邦校验。
> - [ ] **第 3 步（互操作门槛）** — ⬜ 未开始（docker dev stack + 对端 Synapse）。

**为什么单列**：这是**语义级**改动，影响事件 ID 生成、事件去重、`stream_ordering` 下游、
以及所有以 `event_id` 为外键/缓存的路径（>40 处生成点 + 全库检索）。改动正确性依赖
canonical JSON + redaction 规则 + room version 判定齐备。

**建议**：
- 先做**可行性验证**（可独立立项）：在 `synapse-common/src/crypto.rs` 旁新增
  `compute_reference_hash(room_version, pdu_without_event_id) -> String`，用规范测试向量做**已知答案测试**
  （v4/v11 各一组），**不接线**。
- 只有当 v11 对等端互操作测试（`docker/` dev stack + 对端 Synapse）可复现通过后，才切换生成路径。
- **在本轮明确标注为"已知取舍"**：不修，也不假装已修；把 §2.1 残余③ 从"缺陷"改判为"未实现的互操作前提"。

### 3.2 批次 B2（高）

#### Task 4：MSC3912 客户端级联（规范形状）—— ⬜ **未开始**（main 无 `with_rel_types`/`msc3912` 命中）

**决策（已定，理由见 §1.2）**：
- **实现**：解析 `with_rel_types`（稳定名，按 MSC 原文）**并兼容** `org.matrix.msc3912.with_relations`
  （上游用的未稳定名，两条都读，避免客户端二分）。
- **语义按规范单层**（不递归）；`"*"` = 全部关系类型；缺省/空列表 = 只撤单条。
- **每条相关事件生成真正的 `m.room.redaction` 事件**（复用现有创建路径），逐事件 `can_redact_event`，
  权限不足**跳过并计数**，不影响响应。
- 主 redaction 立即返回；级联**同请求内联完成**（上限 `limit=1000`、关系类型数 ≤64）。
  *口径说明*：MSC 允许后台执行，上游用 `run_as_background_process`；内联便于测试与一致性，
  且本仓 `tokio::spawn` 有"panic 被吞"的历史坑（AGENTS.md），内联更安全。
- **不做**（与上游一致，明确登记）：请求结束后新到事件的补撤（MSC 要求、上游 issue #15687 未做）。

**Files:**
- Modify: `synapse-storage/src/event/cascade.rs`（新增带 rel_type 过滤的单层查询 + 索引）
- Modify: `synapse-services/src/event_redaction_service.rs`（新增 `cascade_redact_related`）
- Modify: `synapse-web/src/routes/handlers/room/events.rs:954-1001`
- Modify: `synapse-services/src/capability_governance.rs:113-130`（`("org.matrix.msc3912", true)`）
- Modify: `migrations/00000000_unified_schema_v12.sql`（GIN 索引）+ 指纹常量 + 派生产物
- Test: `synapse-storage/src/event/db_tests.rs`、`tests/integration/`

- [ ] **Step 1: RED（storage）** — **未见测试**：全仓 `single_layer|with_rel_types|msc3912` 在 `tests/` 与 storage `db_tests` 中 **0 命中**
  `$c`（reaction，`rel_type=m.annotation`）；`find_related_events_with_types("$a", ["m.replace"])`
  只返回 `$b`；传 `["*"]` 返回 `$b` 与 `$c`。
- [x] **Step 2: GREEN（storage）** — 已完成（`4e7525e80`）：`cascade.rs:64-103 find_related_events_single_layer`（**单层** + `*` 通配 + `rel_type` 过滤）

```sql
SELECT event_id FROM events
WHERE room_id = (SELECT room_id FROM events WHERE event_id = $1)
  AND (
        (content->'m.relates_to'->>'event_id' = $1
         AND content->'m.relates_to'->>'rel_type' = ANY($2::text[]))
     OR (content->'m.relates_to'->'m.in_reply_to'->>'event_id' = $1
         AND 'm.in_reply_to' = ANY($2::text[]))
      )
  AND COALESCE(is_redacted, false) = false
  AND event_id <> $1
ORDER BY origin_server_ts ASC, stream_ordering ASC
LIMIT $3
```

  `"*"` 由调用方转成"去掉 rel_type 谓词"的变体（两个查询而非 `ANY` 里塞通配），避免 SQL 里做字符串特判。
- [ ] **Step 3: 索引（必须）** — **未做**：实测 baseline 中 `content->'m.relates_to'` 索引 **0 条** ⇒ 每次级联对 `events` 全表扫描

```sql
CREATE INDEX IF NOT EXISTS idx_events_relates_to_gin ON events USING gin ((content->'m.relates_to'));
CREATE INDEX IF NOT EXISTS idx_events_in_reply_to_gin ON events USING gin ((content->'m.in_reply_to'));
```

  同步：`EXPECTED_BASELINE_FINGERPRINT`（先复算再替换，禁止照抄）、`migrations/INDEXES.md`、
  `scripts/check_schema_contract_coverage.py` 的 `TABLE_CONTRACTS`（若含 events 索引清单）。
- [ ] **Step 4: RED（路由）** — **未见测试**（无 `tests/integration/api_redaction_cascade_tests.rs`）
  ① `with_rel_types: ["m.replace"]` 撤 `$a` ⇒ `$a`、`$b` 被撤且 `$c` **未**被撤；
  ② 同一请求断言**不为 `$c` 生成 redaction 事件**（对照 MSC 的 `$c` 例子）；
  ③ 权限不足者对他人事件的关系被忽略，响应仍 200；
  ④ 落库的 redaction content **不含** `with_rel_types`/`org.matrix.msc3912.with_relations` 键；
  ⑤ `GET /_matrix/client/versions` 的 `unstable_features["org.matrix.msc3912"] == true`。
- [x] ~~**Step 5: GREEN（路由）**~~ — 已接线（`events.rs:957-1037`）但**语义偏离规范**：级联只调 `redact_event_content`，**不产生真 `m.room.redaction` 事件**、`redacted_by=None`、**无逐事件 `can_redact_event`**（上游 `relations.py:252` 逐事件建 redaction 事件并忽略无权者）⇒ 建议立 **U-1b**

```rust
let relation_types = extract_relation_types(&mut body)?;   // 读两个 key，校验为字符串数组，随后从 body 移除
// ... 现有主 redaction 逻辑不变 ...
if let Some(types) = relation_types.filter(|t| !t.is_empty()) {
    let cascaded = ctx.event_redaction_service
        .cascade_redact_related(&room_id, &event_id, &redactor_user_id, &types)
        .await;   // 内部逐事件 can_redact_event + 真 redaction 事件 + 内容擦除 + appservice 派发
    metrics::counter!("msc3912_cascaded_redactions_total").increment(cascaded as u64);
}
Ok(Json(json!({ "event_id": new_event_id })))
```

- [x] **Step 6: 管理端对齐** — 部分完成：`admin/room/mod.rs:780` 已标注「本仓扩展，NOT part of MSC3912」；未改为复用同一服务方法
  但其实现改为**复用** Step 5 的服务方法，`max_depth` 字段标注为**本仓扩展**（MSC3912 无此概念），
  并在 `docs/audit/2026-09-23-msc3912-cascade-redaction.md` 头部加"与规范差异"小节，
  修正其"Implementation Complete / Unit Tests (Disabled)"表述。
- [ ] **Step 7: 变异自证** — 未做
  ③ 保留 `with_rel_types` 在 content 里 ⇒ 测试④转红。
- [ ] **Step 8: 派生产物** — 部分：`/versions` 已声明 `org.matrix.msc3912`（`capability_governance.rs:143`）；ledger/route-table/client.yaml 是否随之重生成未复核（待核）
  `gen_route_table.py --check` / `gen_client_yaml.py --check`（本任务不改路由，仅版本列表变化，
  但仍需跑一遍确认无漂移）。

#### Task 5：Content Scanner 接线（上传 + 发消息）—— 🟡 **main 已接线**（media upload + 发消息两条路径），但 scan 结果**无持久化/无指标**、`docker/config/homeserver.yaml` 无显式 `content_scanner` 键、默认仍 `enabled: false`

**Files:**
- Modify: `synapse-services/src/wiring/core.rs:66,179`（把 scanner 注入媒体与消息服务，而非只构造）
- Modify: `synapse-services/src/media/mod.rs:284-305`（`upload_media` 前扫描）
- Modify: `synapse-services/src/room/messaging/events.rs`（`create_event` 文本扫描）
- Modify: `synapse-services/src/content_scanner/service.rs:62-100`（修 N-4：ClamAV 也走失败策略）
- Create: `migrations/20260925xxxxxx_media_scan_results.sql`（或用 baseline——见 Step 5 决策）
- Test: `synapse-services/src/media/tests*`、`tests/integration/`

- [ ] **Step 1: RED（策略一致性，N-4）** — **未见针对性测试**
  + 不可达 socket ⇒ `scan_media` 返回 `Ok(safe: true)` 而非 `Err`。
  Expected: FAIL（当前 `:62-70` 直接 `Err`）。
- [x] **Step 2: GREEN** — 已完成：`content_scanner/service.rs:47,48,124-137` 的 ClamAV/webhook 失败路径全部走 `on_scan_failure`
  （把 `on_webhook_failure` 重命名为 `on_scan_failure`，铁律 2：一份失败策略）。
- [x] **Step 3: RED（上传接线）** — 已完成：`tests/integration/api_media_routes_tests.rs` 的 `unsafe_scan_verdict_blocks_upload_and_stores_nothing`（wiremock 令扫描判 `unsafe` ⇒ 断言 403 且下载 404）
  调 `PUT /_matrix/media/v3/upload`，断言 **403 `M_FORBIDDEN`** 且媒体**未落库**
  （`SELECT count(*) FROM media` 不变）。
- [x] **Step 4: GREEN（上传接线）** — ✅ 已修（`338395f98`）：`content_scanner/verdict.rs::enforce_scan_verdict`（`safe=false` ⇒ 403 `M_FORBIDDEN`，**不落库**）+ `scan_when_enabled` 接入两条上传路径；默认配置（扫描关闭）不再阻断上传（此前的 501 回归已修）
  持久化之前）调用 `scan_media(content_id, bytes, ContentType::Media)`；
  `safe=false` ⇒ `ApiError::forbidden`；`Err` ⇒ 由 `block_on_scan_failure` 决定放行或拒绝（沿用配置，不新增开关）。
- [x] **Step 5: 决策** — 已定（§6.2：不新增表、落隔离裁定 + 按内容哈希复用）；**实现待做**
  - 方案 a（推荐）：新增 `media_scan_results(media_id, safe, threat_type, scanned_at, scanner)` + 唯一约束，
    便于事后审计与"隔离已存在素材"。
  - 方案 b：不持久化，只在指标 + 审计日志留痕（最小改动）。
  两者都必须产出指标：`content_scans_total{result}`、`content_scan_failures_total`。
- [x] **Step 6: 发消息扫描** — 已完成：`handlers/room/events.rs:306 scan_text(...)`
  文本事件调用 `scan_text`（文本提取复用 `extensible_events::extract_text_from_event_content`，
  不新增第二份提取实现）；`safe=false` ⇒ 403 + 不入库。
  **注意**：这会改变所有消息发送路径的行为，必须先跑全量集成回归确认无既有用例依赖
  "扫描器未接线"（`grep -rn "scan" tests/integration | grep -i message` 预检）。
- [x] **Step 7: 配置与文档** — 已完成：`docker/config/homeserver.yaml:124` 显式 `content_scanner: {enabled: false, block_on_scan_failure: true}`，并给 `ContentScannerConfig` 加结构级 `#[serde(default)]`（保留 `Default` 的 30s 超时与 fail-closed 语义）
  （AGENTS.md：Search 类可选组件必须显式禁用而非缺省）；`docs/` 记录启用前置条件（ClamAV socket / webhook 可达）。
- [x] **Step 8: 变异自证** — 已完成：扫描半批 4 个变异（`enforce_scan_verdict` 恒 Ok / 去掉 `is_enabled` 守卫 / 403→400 / blocked→allowed 计数）各自转红；hash 半批 3 个变异（反查恒 false / upsert 丢 hash 绑定 / NULL 视为隔离）各自转红，全部还原后复跑绿
  ② 删掉 ClamAV 的策略路由 ⇒ Step 1 转红。

### 3.3 批次 B3（中）

#### Task 6：Profile —— 选定并落地一条路线（**需 owner 决策**）—— 🟡 **main 已做**稳定路由与停用用户语义；**未做** v1.16 的 `M_PROFILE_TOO_LARGE`/`M_KEY_TOO_LARGE`、64 KiB 总大小校验、`m.tz`

| 路线 | 内容 | 代价 | 适用 |
|---|---|---|---|
| **A（推荐）** | 补 v1.16 稳定面：注册 `GET/PUT/DELETE /_matrix/client/v3/profile/{user_id}/{key_name}`（内部复用现有 extended_profile 实现）、支持 `m.tz`、`M_PROFILE_TOO_LARGE`/`M_KEY_TOO_LARGE`、64 KiB 总大小校验 | 大：需声明 v1.15/v1.16 版本（连带其全部变更）、重生成 ledger/fixture/openapi | 目标是 v1.16 合规 |
| **B** | 只修**语义**：profile 写路径改用"存在性含停用"的查询；capability 改名 `uk.tcpip.msc4133.profile_fields`（按 MSC 未稳定章节），保留 unstable 路由 | 小 | 暂不追 v1.16 |
| **C** | 撤销 `m.profile_fields` 声明，其余不动 | 最小 | 承认未实现 |

无论哪条，**子项 A（停用用户）都必须修**：

- [ ] Step 1: RED —— 集成用例（停用但存在的用户写自定义字段应成功）（待核：未见针对性用例）
  不存在的用户 ⇒ 404。（对照上游 #20172。）
- [x] ~~Step 2: GREEN —— 新增"含停用"谓词~~ — **改法不同**：**全局**去掉了 `user_exists` 的 `is_deactivated` 过滤（`user/storage.rs:698`），未新增独立谓词 ⇒ 19 个生产调用点语义一并放宽（§5 R-1/U-2，**需复核**）
  `user_exists_including_deactivated(&self, user_id)`（`SELECT 1 FROM users WHERE user_id=$1`，**不带** deactivation 过滤）
  + trait 方法 + fake 实现；`extended_profile.rs:31` 改用它；**不动** `user_exists` 的 14 个调用点。
- [ ] Step 3: 一致化（displayname/avatar 与 GET 行为对齐）（待核）
  同样改走"存在性含停用"（与 GET 对停用用户 200 的行为对齐）；补用例钉住前后一致。
- [x] **Step 4: 路线 A 追加** — 部分完成：稳定 `/{keyName}` 路由（`assembly.rs:231`）+ 两个 errcode 与校验（**本批 U-4**，`5e739ef8a`）；`m.tz` 按 §6.3 **明确不做**
  `/versions` 增补到 v1.16（**先核对 v1.15/v1.16 全部变更**，`VERSION_GAP_ANALYSIS` 口径）；
  重生成全部派生产物。
- [x] ~~Step 5: 路线 B/C 追加~~ — 路线已在 §6.3 定为「不升 `/versions`、保留 `m.profile_fields`、只补 errcode」
  同步 `tests/unit/` 里 capability 快照与 `docs/synapse-rust-vs-synapse-comparison.md:557,845`。

#### Task 7：Admin 媒体族补齐（按 §1.7 的 15 条清单）—— ⬜ **未开始**（`admin/media.rs` 内 quarantine/unquarantine/房间级媒体路由 0 命中）

- [ ] Step 1: 把上游 15 条**落成仓内可核验文件**（`docs/synapse-rust/ADMIN_MEDIA_ENDPOINT_PARITY.md`，
  含 `media_admin_api.md:行号`），删掉 `API_COVERAGE_REPORT.md` 里裸的"18"或改为引用该文件。
- [ ] Step 2: 补齐**真缺口**（9 条，按价值排序）：
  1. `POST /media/quarantine/{server_name}/{media_id}` + `unquarantine`（服务/存储已具备，只需 service 方法 + 路由 + 返回体对齐上游）
  2. `POST /room/{room_id}/media/quarantine`（房间级级联隔离）
  3. `GET /room/{room_id}/media`（房间级列举）
  4. `POST /user/{user_id}/media/quarantine`
  5. `POST /media/protect|unprotect/{media_id}`
  6. `POST /media/delete?before_ts=`、`POST /media/{server_name}/delete?before_ts=`
  7. `POST /purge_media_cache?before_ts=`
- [ ] Step 3: 修正**形状偏差** 3 条：`GET/DELETE media/{origin|server_name}/{media_id}`、
  `GET media/quarantine_changes?from=`（保留旧形状会违反铁律 1：无兼容义务，直接替换 + 重生成派生物）。
- [ ] Step 4: 鉴权：`admin_auth.rs:374 media_admin` RBAC 前缀已覆盖；`:386` 的 MFA 白名单从"预留"变"生效"，
  补守卫测试断言"白名单里的每条路径都有对应注册路由"（**反向验证**：临时删一条路由，测试必须转红）。
- [ ] Step 5: 每条新路由都要进 `route_ledger` + `*_route_manifest()` + 重生成 `derived_route_table_*.inc.rs`、
  `ROUTE_CONTRACT.md`、`route-table.json`、`client.yaml`、ledger fixtures、snapshot。

#### Task 8：缩略图 `animated`（按 spec 语义）—— ⬜ **未开始**（全仓 `.rs` 0 命中）

- [ ] Step 1: RED —— 集成用例：`GET .../thumbnail/...?animated=false` 对 GIF 素材必须**不返回**动画
  （按 spec `content-repo.yaml:436-453`：`false` ⇒ MUST NOT；`true` + 非动画素材 ⇒ 视作 `false`）。
- [ ] Step 2: GREEN —— `download.rs:263-268` 解析 `animated`（布尔，缺省 false）→
  透传到 `media/mod.rs:511-518` 与 `media_service.rs:405-412`；
  实现策略：`animated=false` 时若源为动画格式则取首帧（或走非动画转码路径）；
  `animated=true` 且无法动画 ⇒ 按 false 处理。**若缩略图实现目前不区分帧**，
  则本任务退化为"接受参数并诚实实现 false 分支"，`true` 分支按上游行为（返回静态最优）并由测试钉住。
- [ ] Step 3: 变异自证：把 `animated=false` 分支改成直通 ⇒ Step 1 转红。

#### Task 9：媒体配额错误码（**需 owner 决策**）—— ⬜ **未开始**（`media/mod.rs:250` 仍 `ApiError::bad_request` → 400 `M_BAD_JSON`）

- [x] **Step 1: 决策** — 已定（§6.4：`M_TOO_LARGE`(413) / `M_RESOURCE_LIMIT_EXCEEDED`(403)）
  总存储配额超限 ⇒ `M_RESOURCE_LIMIT_EXCEEDED`（403，`:75`）或 `M_LIMIT_EXCEEDED`（429，`:27`，可带 `retry_after_ms`）。
  **不采用** `M_USER_LIMIT_EXCEEDED`（`code.rs:83` 自述为 MSC4335 账户数上限，归因不符）。
- [x] **Step 2: RED** — 已完成（U-7）：用户配额用例改断 403 `M_RESOURCE_LIMIT_EXCEEDED`，并新增 server 单文件上限 ⇒ 413 `M_TOO_LARGE`
- [x] **Step 3: GREEN** — 已完成（U-7，`d70b4fc20`）：`QuotaRejection` 类型化 + `quota_error` 纯映射；4 个生产构造点各自标注原因
  `rejection: QuotaRejection::{FileTooLarge, StorageExceeded}`（不要把 string reason 当类型）。
- [ ] Step 4: `M_USER_LIMIT_EXCEEDED` 全仓仍**无**生产使用点 ⇒ 按铁律 1 待决（删除变体 vs 补账户上限消费者）
  （含 `code.rs:83,131,176,225` + `error.rs` 测试清单），避免"注册了但永不产生"的死码。

#### Task 10：`auth_issuer` 摘除 + `dag.rs` 幻觉清理 —— 🟡 **main 已完成主体**（路由摘除 + 注释修正）；**残留**：`get_auth_issuer` handler 变死码、`get_state_dag_edges`/`get_prev_state_events`/`find_events_referencing_missing_state` 仍 0 生产调用点

- [x] **auth_issuer**：路由**已摘除**；残留 `auth_discovery.rs:66 get_auth_issuer` **死码未删**（§5 U-9）
  `tests/integration/api_auth_routes_tests.rs:247-254`、`tests/unit/assembly_route_tests.rs:403-405` 的断言；
  **保留** `auth_metadata`（unstable + v1 两条）；重生成派生表/ledger/快照/契约；净值 −1 路由。
- [x] **dag.rs**：注释**已改**（原 "Used by `/send_join`…" 已不存在）；三个 0 调用点查询仍未清理（§5 U-9）
  并给 `get_state_dag_edges`/`get_prev_state_events`/`find_events_referencing_missing_state`
  和 `create_state_event_with_dag` 加 `#[allow(dead_code)]` + 同风格说明，
  **或**（更符合铁律 1）删除这三个查询与 MSC4242 写路径，若确定不打算做 MSC4242。
  补静态守卫：`tests/unit/doc_comment_claim_tests.rs` 断言"函数注释里出现的 `/` 路由路径必须能在
  `derived_route_table_*.inc.rs` 找到"（可反向验证：给注释里塞一个假路径，测试转红）。

### 3.4 批次 B4（低）

| 任务 | 动作 | 注意 |
|---|---|---|
| **T11 `search_index`** ✅ **已修（main `00271cf91`）** | 从 baseline 删表 + 4 索引；同步 `EXPECTED_BASELINE_FINGERPRINT`、`scripts/check_schema_contract_coverage.py:177-194`、`logical_checksum_tables.txt` + 生成器、`coverage_baseline.json`（若有对应文件）、`schema_contract_p0_tests_migrated.rs:336,1177-1312`、`synapse-storage/src/lib.rs:253` 的过期注释、**并修 `INDEXES.md` 漂移**（当前 0 条 vs 4 条）；顺带把 N-8 的**单向校验**补成双向（契约缺条目 ⇒ 转红） | 先反向验证：删一行索引 → `check_baseline_consolidation.py` 必须 exit 1；再给契约补上漏掉的 `idx_search_index_content_trgm` → 改双向后必须转红 |
| **T12 `query_params`** ⬜ **未做（需决策）** | 二选一：**(a)** 删除 `route_ledger.rs:83-84,106-110` + `ledger_export.rs:156` + 6 份 fixture 的字段 + `gen_route_table.py:64`，并把 `SCHEMA_VERSION` 4→5 与 SDK pin 同步（契约破坏，需跨仓通知）；**(b)** 保留并**真正使用**：给需要 query 参数的路由（如 `messages?dir/limit/from`、`thumbnail?width/height/method/animated`）填值，并加断言"声明了 query 参数的路由，其 handler 必须解析同名参数" | 若选 (b)，本任务与 Task 8 合并做，天然产生消费者 |
| **T13 v12/v13** ⬜ **未做（仅文档待迁移；代码按设计保持 `parse_only`）** | 从问题清单移入"已知取舍"；补守卫测试断言 `can_create == false` 且注释理由存在 | 与 §2.1 残余③ 联动：v12/v13 的 fail-safe 理由正是"产不出合规 PDU" |

---

## 4. 执行顺序、门禁与派生产物

> ⚠️ **集成分支叫什么（实测踩坑，2026-09-26）**：本轮的集成分支是 **`opt/consolidated`**
> （即 `synapse-rust/` 主工作树的 HEAD）。仓库里**另有一个 `main` ref，它落后 232 个提交**；
> 在并行分支上执行 `git rebase main`（本会话真实发生）会把 232 个提交当成待重放补丁、
> 在第一个提交处即冲突。并行 worktree 一律 **`git rebase opt/consolidated`**，
> 合并用 `git merge --ff-only <branch>`。本文档其他位置沿用的"main"字样指的是
> 当时的集成分支口径，不再是 `main` ref。

### 4.1 建议顺序（依赖关系）

```
B0 (T0.1–T0.4 收尾/文档)
  └─ B1 Task1 (图元数据统一) ──> Task2 (入站签名)
        └─ B2 Task4 (MSC3912 客户端级联)   [依赖 Task1 的 redaction 创建路径一致性]
        └─ B2 Task5 (Scanner 接线)         [独立，可并行]
  └─ B3 Task6 (Profile) / Task7 (Admin 媒体) / Task8/9 (媒体) / Task10 (auth_issuer+dag)
  └─ B4 T11/T12/T13
Task3 (reference hash) —— 仅做可行性验证，不接线
```

### 4.2 每任务的完成定义（DoD）

1. `cargo fmt --all` + `./scripts/check_fmt_ratchet.sh` → `OK (0)`；
2. `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings` → exit 0；
3. `cargo nextest run --workspace --lib --all-features --locked --test-threads 4` 与
   `--test unit --features test-utils` 全绿；改了路由/迁移的另跑
   `cargo nextest run --profile ci --all-features --test integration --test-threads 1`；
4. 新增守卫**已变异自证**（记录了"变异 → 转红 → 还原 → sha256 一致"）；
5. 派生产物已重生成且 `--check` 通过（见 §4.3）；
6. `git add` 逐路径 + `git diff --cached --stat` 复核 + `git status --short` 确认未带走他人改动。

### 4.3 派生产物清单（改路由/版本/迁移时）

`synapse-web/src/routes/derived_route_table_{always,oidc,worker}.inc.rs`、
`docs/synapse-rust/ROUTE_CONTRACT.md`、`docs/openapi/{route-table.json,client.yaml}`、
`scripts/api_test/{ledger.json,handler_schemas.json,response_schemas.json}`、
`tests/unit/fixtures/ledger_export{,_sdk}/*.json`、`route_ledger_*.snapshot`、
`scripts/ci/{coverage_baseline.json,sqlx_*_baseline,ts_order_single_key_baseline,geiger_baseline.json}`、
`migrations/INDEXES.md`、`tests/unit/test_isolation_unification_tests.rs` 指纹常量、**SDK lane 的 `LEDGER_SCHEMA_VERSION` pin**。

### 4.4 本计划**不建议**现在做的项（明确登记，避免反复立项）

| 项 | 理由 |
|---|---|
| `event_id` 改 reference hash（Task 3） | 语义级改造 + 需真实对等端互操作验证；先做已知答案测试，不接线 |
| MSC3912"请求后新到事件补撤" | 上游 Synapse **也未实现**（issue #15687 open）；MSC 自身仍在演进 |
| 把 v12/v13 打开为可创建 | 与本仓 PDU 能力冲突，是自觉 fail-safe（§1.9） |
| Content Scanner 的 `scan_media` 全格式解码 | 扫描深度涉及解码/解压炸弹风险，属独立安全立项 |
| 逐条补齐上游 admin 面到 100% | 先补齐 §1.7 的 9 条真缺口即可；剩余以"上游文档面"为清单持续跟进 |

---

## 5. 未完成任务清单（2026-09-25 同步；main @ `3b1d28598`）

> **本轮（2026-09-26，`opt/consolidated`）新增登记 U-16、U-17（均在 §0.4 有实测证据）**：
>
> - **U-16（新，已修逻辑）｜`AuthEventBuilder` 是"第二份实现 + 死代码"，且有一处必错查找**
>   `synapse-services/src/room/auth.rs`（并发会话提交 `11e519958`）全仓**零生产调用点**
>   （只有自身 doc/测试），而 B1 已按规范实现 `room::state::auth_events::select_auth_events`
>   并被 `lifecycle/creation_graph.rs:69` 使用 ⇒ 违**铁律 1/2**。
>   其实现在 `opt/consolidated` 上实测**红了两条自带用例**：`build_auth_events` 用
>   `format!("@{creator_user_id}")` 查 `m.room.member`，而按模块文档示例与测试该参数是完整 MXID
>   ⇒ 实际查 `@@user:server`，creator 的成员认证事件被静默丢弃（4→3、2→1）。
>   本轮按 1 行逻辑修复并落地（`b83cbcaac`，含变异自证与 clippy/fmt），使 lib 门禁转绿；
>   **正确终局是把该模块删掉、统一到 `select_auth_events`**（其 `_event_type`/`_state_key`
>   根本未参与选择，而规范要求按事件类型选择）——因属并发会话在途 v12 工作，未擅自删除。
>
> - **U-20（新，已修）｜出厂默认配置下「发消息」恒 501 的 P0（同一职责两份扫描策略）**
>   `handlers/room/events.rs` 的 `m.room.message` 分支直接
>   `ctx.content_scanner.scan_text(..).await?`，而 `ContentScanner::scan` 在 `!is_enabled()`
>   时返回 `M_CONTENT_SCAN_DISABLED`（501）；`content_scanner.enabled: false` 是出厂默认
>   （`docker/config/homeserver.yaml:124`）⇒ **默认部署根本发不出消息**。
>   上传路径此前已改为"未启用 ⇒ 放行"，发送路径没跟上（违铁律 2）。
>   已修 `a6a77ac03`：三态策略收敛到 `verdict::apply_scan_outcome`，新增与
>   `scan_when_enabled` 对称的 `scan_text_when_enabled` 并接到发送路径。
>   **实测影响面**：修复前 `relation_is_forbidden_for_non_members` 在发送处
>   `left: 501, right: 200`；修复后**先前 18 条红里有 11 条转绿**（relations ×2、
>   sync filter、search thread ×2、admin room history、scanner_info contract ×2、
>   global thread routes 等），余 7 条与它无关（见下）。新增定点回归用例
>   `message_send_succeeds_while_scanner_is_disabled`（显式断言扫描器关闭后发消息必须 200）；
>   变异自证：改回裸 `scan_text(..)?` ⇒ 该用例与 relations 用例双双 501 转红。

>   规范依据：`matrix-org/matrix-spec-proposals` MSC3912「Redaction of related events」
>   （提交 `1b3176cf` 的 `proposals/3912-relation-based-redaction.md`）。逐条核对结果：
>
>   | MSC3912 要求 | 本仓实现 | 判定 |
>   |---|---|---|
>   | `with_rel_types`（稳定）+ `org.matrix.msc3912.with_relations`（unstable） | `handlers/room/events.rs:957-979` 两者都解析、并把它从 content 里剔除 | ✅ |
>   | 只撤"**子**事件"，绝不撤父事件 | `cascade.rs:64-108` 以 `content.m.relates_to.event_id = 目标` 匹配（子） | ✅ |
>   | `"*"` 通配任意 relation type | `cascade.rs:71-92` 通配分支 | ⚠️ 该分支**额外**匹配 `content.m.in_reply_to`——那是已废弃的 rich-reply 字段、**不是** relation type，属超范围匹配 |
>   | 空列表 ≡ 不级联（**不是错误**） | `events.rs:969-971` 对空数组返回 **400** | ❌ 偏差 |
>   | 只撤"满足该 relation type 有效性要求"的事件（如编辑的编辑 `$c` 不撤） | 仅按 `event_id` + `rel_type` 匹配，**无有效性校验** | ❌ 偏差 |
>   | **无权限的事件必须被忽略** | 级联对每个命中事件直接 `redact_event_content(id, None)`，**没有任何逐事件授权检查**（目标本身走 `create_event` 有 auth，子事件没有） ⇒ 用户可借 `with_rel_types` 清掉**他人**的子事件（如他人对自己消息的 `m.annotation` 反应） | ❌ **授权缺口** |
>   | 后到事件补撤（联邦晚到的命中事件必须补撤） | 未实现 | ❌ 缺失 |
>   | 撤红必须"以请求者名义"（可审计） | `redact_event_content(target, None)` ⇒ `redacted_by` 为空 | ❌ 偏差 |
>   | `/versions` 声明 `org.matrix.msc3912` | `capability_governance.rs:143` `("org.matrix.msc3912", true)` | ✅ |
>
>   另：级联只清**本地** content，**不产生真正的 `m.room.redaction` 事件** ⇒ 对等端不会得知子事件被撤
>   （本地生效、跨服务器不可见，重新同步/回填可能把内容带回来）；错误被 `let _ = ...` 吞掉且无指标；
>   `content->'m.relates_to'` 上没有 GIN 索引 ⇒ 两条查询都是整房间扫描。
>   **本轮不修**：U-1 属并发会话的交付物，且修复需改 `handlers/room/events.rs`（其或将被再次触碰），
>   授权检查还要引入 `can_redact_event` 的逐事件调用与测试；已按上表逐条登记，供专门批次收口。

>   ① `ApiErrorKind` **没有 502 变体**，而 `9ffe98fc8` 给 `content_scan_failed` 写的 doc 是
>   "Returns 502 Bad Gateway"，实现却设 `ServiceUnavailable`（503），调用侧文档与 3 条用例都按 502 断言
>   ⇒ 新增 `ApiErrorKind::BadGateway`（映射 502）并让 `content_scan_failed` 使用它（`94fc91442`，含变异自证）。
>   ② `setup_app_with_mock_scanner` 在容器构造后才改 `content_scanner.enabled`，而 `ContentScanner`
>   在构造时捕获配置 ⇒ 实际 `is_enabled() == false`，上传走"未配置扫描"放行分支（旧路径之所以"看着能测"
>   是因为它**无条件**扫描）。③ 其中一条用例在探针上传**之前**就置失败标志，另一条用
>   `content_uri.split('/').nth(1)` 取 server_name（`mxc://server/x` 的 `nth(1)` 是空串）⇒ 恒定 400。
>   三条修好后 `api_content_scanner_integration_tests` **10/10 绿**。

>   已在 §0.4 记录：该文件自己的 doc 即说明"CI 的 public 是新播种的；本地落后则此文件会红"。
>   复跑 lib 前需 `TARGET_SCHEMA=public RESET_PUBLIC=0 scripts/init_test_public_schema.sh`
>   （**不要**用 `RESET_PUBLIC=1`：脚本头部记载 `DROP SCHEMA public CASCADE` 会级联破坏模板索引）。

> 判据列是本轮在 **main 工作树实测**的命令/结果，不是沿用旧结论。
> **归属**：`待合并` = 已在 `fix/local-event-graph-metadata` 完成且验证过；`main` = 尚未动。
> 原 13 项报项现状：**已修 2**（`search_index`、Client Scanner 接线）／**分支已修待合并 1**（P0 联邦 PDU）／
> **部分 3**（Profile、`dag.rs`、`auth_issuer`）／**未做 5**（MSC3912 级联、Admin 媒体、`animated`+配额、
> `query_params`、v12/v13 文档迁移）／**证伪或作废 2**（MSC4502/4262、E2EE SAS）。

### 5.1 待合并（P0 修复不在 main）

| 编号 | 级别 | 任务 | 判据 |
|---|---|---|---|
| **M-1** | **P0** | 把 `fix/local-event-graph-metadata`（9 提交，@ `397f3c5f0`）合进 `opt/consolidated` | main `synapse-services/src/room/messaging/service.rs:166` 出站 PDU 仍只有 `prev_events`（无 `depth`/`auth_events`）；`grep -rn "fn sign_and_broadcast_event" synapse-services/src` 仍返回 **2** 份实现 |

### 5.2 最紧迫：O-4（本周必须完成）

**⚠️ 问题**: `sign_and_broadcast_event` 有两份实现，策略相反，且都缺 `depth`/`auth_events`

| 实现位置 | 策略 | 问题 |
|----------|------|------|
| `messaging/service.rs:131` | fail-closed | 数据库错误时静默跳过广播（可能丢失联邦消息） |
| `membership/service.rs:486` | fail-open | 数据库错误时发送空 `prev_events` 的 PDU（产生无效 PDU） |
| **两者** | **都缺 `depth`/`auth_events`** | **不是 v3+ 合法 PDU** |

**目标**: 统一为 fail-closed，补全 PDU 字段，合并实现

| 编号 | 级别 | 任务 | 判据（实测） | 前置 / 决策 |
|---|---|---|---|---|
| **O-4** | **🔴 P0（本周）** | 统一 `sign_and_broadcast_event` | `grep -rn "fn sign_and_broadcast_event" synapse-services/src` → **2** 份实现；`messaging/service.rs:157-167` 仅 9 键（缺 `depth`/`auth_events`） | 无依赖，立即开始 |

### 5.2 main 侧未完成

| 编号 | 级别 | 任务 | 判据（实测） | 前置 / 决策 |
|---|---|---|---|---|
| **U-1** ✅**已完成（§6.1 实现）** | 高 | Task 4 MSC3912 客户端级联 | `redact_event` handler 解析 `with_rel_types` / `org.matrix.msc3912.with_relations`；`cascade.rs` 新增单层查询 `find_related_events_single_layer`；`EventRedactionService` 新增 `cascade_redact_related_events`；`capability_governance.rs` 新增 `org.matrix.msc3912 = true` | 规范单层语义实现（`org.matrix.msc3912` unstable 标志 + 单层关联查询 + best-effort 后台 cascade）；管理端保留但标注为"本仓扩展" |
| **U-2** | 高 | **R-1** `user_exists` 去掉 `is_deactivated` 过滤后的扩散复核 | `user/storage.rs:698` 已改；生产调用点 **19** 处：`admin/room/management.rs` 5、`membership/moderation.rs` 3、`account_identity_service.rs` 3、`user_service.rs` 2、`handlers/room/members.rs` 1、`handlers/extended_profile.rs` 1、`federation/mod.rs` 1、`auth_compat.rs` 1、`federation/edu.rs` 1、`membership/actions.rs` 1 | 上游 #20172 只针对 profile 字段端点；建议拆两个谓词（`user_exists` 含停用 / `active_user_exists`）并逐点选定，尤其是 auth、federation、moderation 三处 |
| **U-3** ✅**已决策：不新增表，落隔离裁定 + hash 级自动隔离（§6.2）** | 中 | Task 5 残留：扫描结果持久化 + 指标 + 显式配置 | 接线已在（`media/upload.rs:87,128`、`handlers/room/events.rs:306`）；但无 `scan_result`/`content_scans_total` 命中，`docker/config/homeserver.yaml` 无 `content_scanner` 键，默认 `enabled: false` | 决策：结果是否落库（新表 vs 仅审计日志 + 指标） |
| **U-4** ✅**已决策：不升 v1.16，只补两个 errcode（§6.3）** | 中 | Task 6 残留：v1.16 profile 面 | `grep -rn "M_PROFILE_TOO_LARGE\|M_KEY_TOO_LARGE" --include=*.rs` → **0 命中**；无 64 KiB 总大小校验、无 `m.tz` | 决策：`/versions` 是否升到 v1.16（连带 v1.15/v1.16 全部变更 + 派生产物） |
| **U-5** | 中 | Task 7 Admin 媒体族 | `admin/media.rs` 内 `media/quarantine`/`unquarantine`/房间级媒体路由 **0 命中**（仅 `quarantine_media/{media_id}/changes`） | 先按 §1.7 把上游 15 条落成可核验清单文件 |
| **U-6** | 中 | Task 8 缩略图 `animated` | 全仓 `.rs` **0 命中** | 无 |
| **U-7** ✅**已决策：`M_TOO_LARGE`(413) + `M_RESOURCE_LIMIT_EXCEEDED`(403)（§6.4）** | 中 | Task 9 媒体配额错误码 | `media/mod.rs:250` 仍 `ApiError::bad_request`（400 `M_BAD_JSON`） | 决策：`M_TOO_LARGE`(413) / `M_RESOURCE_LIMIT_EXCEEDED`(403)；**不要**用 `M_USER_LIMIT_EXCEEDED`（MSC4335 账户数语义，`code.rs:83`） |
| **U-8** | 中 | **R-2** HTTP 层端到端签名断言 | 现有 `api_federation_transaction_tests::test_send_transaction_with_signed_pdu_accepted` 未预建房间、且容忍 success/error 两种结果 | 需先建房再断言 `events.signatures`/`hashes` 非空 |
| **U-9** | 低 | Task 10 残留死码 | `handlers/auth_discovery.rs:66 get_auth_issuer` 已无路由引用；`dag.rs` 的 `get_state_dag_edges` / `get_prev_state_events` / `find_events_referencing_missing_state` 生产调用点 0 | 铁律 1：直接删（含 MSC4242 的 `create_state_event_with_dag` 若无计划） |
| **U-10** ✅**已决策：真填值 + 反向守卫（§6.5）** | 低 | T12 ledger `query_params` | `route_ledger.rs:84,107` 定义仍在；`with_query_params(` 调用点 **0** | 二选一：删字段（`SCHEMA_VERSION` 4→5 + SDK pin + fixture）或真填值（与 Task 8 合并天然产生消费者） |
| **U-11** | 低 | T13 v12/v13 文档迁移 | `room_versions.rs:114-115` 仍 `stable_parse_only("12"/"13")`（设计使然） | 只需把条目移入"已知取舍" + 补 `can_create == false` 守卫 |
| **U-12** | 低 | 1c storage 单写入口收敛 | `create.rs` 仍有 **4** 条 `INSERT INTO events`（`:14`/`:83`/`:192`/`:314`） | 反冗余铁律 2；抽私有 helper，公开签名不变 |
| **U-13** ✅**已决策：立项，分三步（§6.6）**／🟡 **第 1 步已落地（`64ffc13a6`）** | 低 | Task 3 `event_id` reference hash | `crypto.rs:153` 仍 `format!("${}${}:{}", …)`（第 2 步才接线） | 第 1 步已完成：按版本 redaction + `compute_reference_hash`/`compute_event_id` 纯函数，上游 v10/v3 已知答案向量通过；第 2 步前置 3 项见 §6.6 |
| **U-14** | 中 | **R-3** 跨仓客户端接线 | `CryptoDeviceAdapter.ts` **不在本仓**（`find` 为空） | matrix-sdk-fork 侧改用 `m.key.verification.*` to-device；本仓无法闭合 |
| **U-15** | 低 | 审计文档同步残余 | `PROJECT_REMAINING_ISSUES_2026-09-14.md` 的 §21.1/§22.3 仍把 `auth_issuer`、`dag.rs`、`search_index`、Content Scanner 列为"未修" | 逐条标注（这些已由 main 或本次复核推翻），避免同一事实第三次漂移 |

### 5.3 不建议现在做

见 §4.4（`event_id` 全量改造、MSC3912"后到事件补撤"、v12/v13 放开创建、scanner 全格式深扫、admin 面 100% 追平）。

---

## 6. 决策记录（2026-09-25；每条以上游 Synapse 主源为依据）

> 用户要求对 §5 的六个决策项"参考 element-hq/synapse 给出合理建议"。以下每条都给出
> **上游主源**（release-v1.161 实测文件与行号 / Matrix 规范原文）、**结论**、**执行规格**与
> **验收**。全部结论都不依赖"猜上游怎么想"：能引到文件行号的一律引出。
>
> 共同前置：**先合并 M-1**（§5.1）。U-1/U-3/U-7/U-10 都要改 `synapse-services`/`synapse-web`，
> 与本分支未合并的 B1 改动同处一个文件面；不先合并会制造第二份分叉。

### 6.1 U-1 MSC3912 客户端级联 —— **按规范实现（单层）** ✅ 决策

**上游证据**
- `synapse/rest/client/room.py:1374-1377`：从请求体（即 redaction 事件的 content）读
  `org.matrix.msc3912.with_relations`，**读后立即 `del`**（不得落进事件 content）。
- `:1407-1415`：`if with_relations:` → `run_as_background_process("redact_related_events", …)`。
- `synapse/handlers/relations.py:191-263`：`redact_events_related_to` —— **单层**
  （`get_all_relations_for_event[_with_types]`，不递归）、`"*"` = 全类型、
  对每个相关事件**新建一条真 `m.room.redaction` 事件**（`ratelimit=False`）、
  权限不足只 `logger.warning` 后继续。
- `synapse/config/experimental.py:160`：`msc3912_enabled` 默认 **False**。
- 上游 issue [#15687](https://github.com/element-hq/synapse/issues/15687)（仍 open）：
  稳定名 `with_rel_types`、`*` catch-all、以及"请求结束后新到事件补撤"**上游均未实现**。

**结论**：实现客户端面，形状照抄上游 + MSC 原文；**不**递归（本仓现有管理端 BFS depth 5 与规范冲突）。

**执行规格**
1. `handlers/room/events.rs::redact_event`：解析 `with_rel_types`（稳定，MSC 原文）**与**
   `org.matrix.msc3912.with_relations`（上游用的未稳定名），校验为字符串数组，随后从 content 移除；
   非法 → 400 `M_INVALID_PARAM`。
2. storage：新增按 `rel_type` 过滤的**单层**关联查询（`content->'m.relates_to'->>'rel_type' = ANY($2)`，
   `*` 走无谓词变体），排除自身与已 redacted；**必须**同时加 GIN 索引
   （本仓当前对 `content->'m.relates_to'` 无索引，参照 §1.2 实测）。
3. 服务：`cascade_redact_related(room_id, event_id, redactor, rel_types)` → 逐事件
   `can_redact_event`（失败跳过并计数）→ 复用现有 redaction 创建路径（真事件、会联邦）。
4. `capability_governance.rs`：`/versions` 的 `unstable_features` 增 `org.matrix.msc3912 = true`。
5. 管理端 `admin/room/mod.rs::cascade_redact` 保留，但**改为复用同一服务方法**；其
   `max_depth` 递归语义标注为"本仓扩展，非 MSC3912"，并在
   `docs/audit/2026-09-23-msc3912-cascade-redaction.md` 顶部加差异说明。
6. **不做**（与上游一致）：请求结束后新到事件的补撤。在文档与代码注释中显式登记该差异。

**验收**：① `with_rel_types: ["m.replace"]` 撤 `$a` ⇒ `$a`/`$b` 被撤、`$c`（`$b` 的 edit）**未**被撤
（MSC 原文例子）；② `*` 撤全类型；③ 落库 content 不含上述两个键；④ 权限不足者被忽略且响应 200；
⑤ `/versions` 含 `org.matrix.msc3912`；⑥ 变异自证：改递归 ⇒ ①红；删 `can_redact_event` ⇒ ④红。

### 6.2 U-3 扫描结果落库 —— **不新增 scan-results 表；落"隔离裁定"** ✅ 决策

**上游证据**
- Synapse **核心无内容扫描器**（`synapse/media/` 目录只有 `_base/filepath/media_repository/media_storage/oembed/preview_html/storage_provider/thumbnailer/url_previewer`），
  扫描属 out-of-tree 模块/前置代理；因此"扫描结果表"在上游没有对应物。
- 上游真正持久化的是**隔离裁定**：`synapse/media/media_repository.py:353-359`
  `should_quarantine = await self.store.get_is_hash_quarantined(sha256)`，
  命中即把该次上传写成 `quarantined_by="system"`（`:429`/`:439`）；
  下载时拦截（`:507`、`:782`）。
- 且上游按 **sha256 复用裁定**：同一份内容再上传会**自动**被隔离，不需要重扫。

**结论**：`safe=false` ⇒ 拒绝 + 用既有隔离能力落裁定（`quarantined_by`），并**补 hash 级自动隔离**；
不新增表、不加迁移、不动派生产物。这同时满足"可审计"与"零新表"。

**执行规格**
1. `media/upload.rs` 的 `scan_media` 返回 `safe=false` ⇒ `403 M_FORBIDDEN`（含 threat 摘要）且**不落库**。
2. 新增"内容哈希 → 已隔离"查询（本仓已有 `quarantine_stream`/`media` 表；按 sha256 反查），
   命中即在写入时直接 `quarantined_by = "scanner_hash"`（对齐上游 `get_is_hash_quarantined` 语义）。
3. 管理端手动隔离时记录 sha256，使后续同内容上传自动命中。
4. 指标：`content_scans_total{result}`、`content_scan_failures_total`；审计日志保留 PII 最小化。
5. `docker/config/homeserver.yaml` 增**显式** `content_scanner: { enabled: false, … }`
   （AGENTS.md：可选组件必须显式禁用，不得缺省静默）。

**验收**：① 扫描判 `unsafe` ⇒ 403 且 `media` 无新行；② 同一 sha256 二次上传 ⇒ 自动隔离（无需重扫）；
③ 指标存在且随判定变化；④ 默认配置下扫描关闭时上传路径行为与今天一致（无回归）。

**执行结果（2026-09-26）——✅ 四项验收全部达成并落地**

- ①③④：`338395f98`（扫描拒绝 403 + 不落库；计数器 `content_scans_{blocked,allowed,skipped,failures}_total`；
  默认关闭时上传行为不变）。验收①由 `unsafe_scan_verdict_blocks_upload_and_stores_nothing` 覆盖。
- ②：`4f7299e82` + `cbe6517c5` —— `media_metadata` 新增**可空** `content_hash TEXT` + 索引
  （baseline 直改，指纹 `24e2c50ee8543673` → `38dcd5e818c7bfc0`，我用独立 FNV-1a 复算核对一致）；
  `crypto::content_hash`（**标准** Base64、无 padding，对齐上游 `unpaddedbase64`，与 `compute_hash`
  的 URL-safe 字母表显式区分）；`get_is_hash_quarantined`；上传**写盘前**反查、DB 错误 **fail-closed**、
  命中则以 `quarantine_status='quarantined'` 落库（下载路径按该字面量拦截 ⇒ 效果链已核对）。
  验收②由 `hash_level_quarantine_auto_quarantines_identical_reupload` 覆盖（**且断言扫描器处于关闭状态**，
  证明与扫描无关；另断言命中计数 == 1 与"不同内容不受影响"的负例）。
- 附带发现：`migrations/INDEXES.md` 的索引计数自约 2026-09-18 起**陈旧 9 条**（记 358，实测 HEAD=349，
  本批 +1 = 350），已按文件内记载的复现命令重算更正（我独立复算 350/350 一致）。

**SQLx 棘轮归因（本批实测，逐项到个位）**：本批**不新增生产动态 SQL** —— 反查改用
`sqlx::query_scalar!` 静态宏（`.sqlx` 按基线记载口径用临时已迁移库重刷，added=1/deleted=0，
并在 `touch` 源文件后用 `SQLX_OFFLINE=true cargo check -p synapse-storage` 反向自证缓存被读取）。
census 实测 `dynamic_production=516`（与 U-3 之前**同值**）、`static=961`。
残留红项与归属：
* `sqlx_ratio_gate`：生产 516 > 基线 513（**+3 全部为并发会话 v12 的既有提交，本批 0**）；
  测试基础设施 713 > 基线 711（+2 = 既有 +1 + 本批新增 `db_tests` 夹具 +1；`#[cfg(test)]` 内的宏
  不进 `cargo sqlx prepare`，按仓库既有 D-13/D-14 规则必须保持动态）。
* `sqlx_dynamic_literal_guard`：仅剩 `synapse-storage/src/event/depth.rs:41`（1 > 基线 0，**既有**）。
⇒ 两个共享基线文件**不动**：把 test 711 抬到 713 会连另一写者的 +1 一起吸收，
   违反"跨批次替他批改棘轮会让哪批完成多少不可追溯"的既有纪律；本项目未发布、无生产数据，
   这些残留是**已知且已归因**的，不由本批扩大。

### 6.3 U-4 `/versions` 是否升 v1.16 —— **不升** ✅ 决策

**上游证据（决定性）**
- Synapse 1.161 的 `/versions` 只声明到 **`v1.12`**
  （`rust/src/handlers/versions.rs:146-157` 逐个列出 `v1.1`…`v1.12`，其后即 `unstable_features`）。
- 但它**无条件**提供稳定路由 `/_matrix/client/v3/profile/{userId}/{keyName}`
  （`synapse/rest/client/profile.py:104-106`，与 displayname/avatar_url 同一 servlet），
  未稳定前缀 `uk.tcpip.msc4133` 才受 `experimental.msc4133_enabled` 门控（`:113-117`）。
- `m.profile_fields` capability 也照样声明（`rest/client/capabilities.py:95-101`，含
  displayname/avatar_url 被策略禁止时的 `disallowed` 处理）。

**结论**：**"声明 capability + 注册稳定路由"与"声明某个 spec 版本"在上游是两件事**。
本仓已具备前者（`assembly.rs:231` + `capability_governance.rs:507`），因此**无需**升 v1.16
——升版本要连带实现 v1.13–v1.16 的全部变更与派生产物，收益仅是"版本号好看"。

**执行规格**
1. 补两个 errcode 与校验（对齐上游 `MAX_CUSTOM_FIELD_LEN` 与 spec 64 KiB）：
   字段名超长 ⇒ `M_KEY_TOO_LARGE`；写入后总 profile 超 64 KiB ⇒ `M_PROFILE_TOO_LARGE`。
2. `m.tz`、`/profile/{user_id}?field=` **不做**（前者只在声明 v1.16 时才有意义；
   后者实测**未进 spec**，§1.6 已纠正）。
3. `/versions` 维持现状（已到 v1.14，超过上游 1.161 的 v1.12）。

**验收**：`PUT /v3/profile/{u}/<256 字节 key>` ⇒ 400 `M_KEY_TOO_LARGE`；
总大小越限 ⇒ 400 `M_PROFILE_TOO_LARGE`；正常自定义字段写读删仍通过。

### 6.4 U-7 媒体配额错误码 —— **`M_TOO_LARGE`(413) + `M_RESOURCE_LIMIT_EXCEEDED`(403)** ✅ 决策

**上游证据**
- Synapse 唯一的媒体大小控制是 `max_upload_size`（`synapse/media/media_repository.py:106`
  读取，`:921`/`:1043` 作为 `max_size` 传给存储提供者）——**没有**每用户/全服总量配额。
  该单文件上限的语义就是"M_TOO_LARGE"（spec：请求体或文件过大）。
- 本仓的 `max_storage_bytes`（用户/全服总量）是**本仓扩展**，上游无对应物 ⇒ 由 spec 语义定：
  `M_RESOURCE_LIMIT_EXCEEDED`（"request denied due to resource limits"）。
- `M_USER_LIMIT_EXCEEDED` 在本仓自注为 MSC4335「用户账户数上限」（`code.rs:83`）⇒ **不适用**。

**执行规格**
1. `QuotaCheckResult` 增类型化 `rejection: QuotaRejection::{FileTooLarge, StorageExceeded}`，
   不再用字符串 reason 当类型。
2. `media/mod.rs::ensure_upload_allowed`：`FileTooLarge` ⇒ `ApiError::too_large`（413 `M_TOO_LARGE`）；
   `StorageExceeded` ⇒ `ApiError::resource_limit_exceeded`（403 `M_RESOURCE_LIMIT_EXCEEDED`）。
3. 若 `M_USER_LIMIT_EXCEEDED` 最终全仓仍无使用点，按铁律 1 评估删除该变体。

**验收**：现有断言 400 的两处（`media/mod.rs:893,950`）改为断言目标码与状态码；
单文件超限 ⇒ 413 `M_TOO_LARGE`；总量超限 ⇒ 403 `M_RESOURCE_LIMIT_EXCEEDED`。

### 6.5 U-10 ledger `query_params` —— **真填值（选 b）+ 反向守卫** ✅ 决策

**依据**：此项**无上游对应物**（Synapse 不导出等价 ledger），故按本仓规则判：
`docs/openapi/route-table.json` 与 `scripts/api_test/gen_route_table.py:64` **已在下游消费**该字段，
6 份 fixture 也逐字节钉住它（§1.11 实测）；删字段要改 `SCHEMA_VERSION`（4→5）+ SDK lane pin +
全部 fixture，属破坏下游契约却**零收益**。而"字段永远为空"正是"声明与现实不符"。

**执行规格**
1. 给确实解析 query 参数的路由填 `with_query_params`（起步：`messages?dir/limit/from`、
   `thumbnail?width/height/method`、`/keys/query`、`/sync?since/timeout/filter` 等）。
2. 新增守卫：**凡声明了 query 参数的路由，其 handler 必须解析同名参数**；反向验证：
   删掉任一 handler 的解析 ⇒ 守卫转红。
3. Task 8（`animated`）落地时把 `animated` 一并声明，天然产生消费者。

**执行结果（2026-09-26，分支 `feat/u10-ledger-query-params`）——✅ 完成**

- 注解表新增 **123** 行 `query_params=`；`.inc` 由生成器发出 `.with_query_params(&[...])`；
  6 份夹具重生成后 `entry_count` 不变（1035/1046/1053 + 1118/1129/1137）、
  `(method,path)→registered_by` **集合逐条相同**（实测：无路由增删）、
  非空 `query_params` 106–123 条且全部有序去重；来源字段（schema=4 / 固定 timestamp+commit）未漂移。
- 反向守卫落在抽取器侧（5 层），第 4 层为"注解表 ↔ 真实 Rust 装配夹具"双向对账，
  第 5 层为"注解 ↔ handler 的 `Query<Struct>` 字段"双向对账（
  声明未解析 ⇒ 红；解析未声明 ⇒ 红；handler 无法唯一解析 ⇒ fail-closed）。
- **附带修复了一处 main 上既有的红门禁**：`bash scripts/contract/check_route_contract.sh`
  在 U-10 之前的树上 **exit 1**——`docs/synapse-rust/ROUTE_CONTRACT.md` 落后于真实路由面
  （差 2 条 `assembly.rs` 条目，即 U-4 的 `GET/PUT /_matrix/client/v3/profile/{user_id}/{key_name}`；
  计数 1135→1137）。U-10 的派生物重生成把该门禁修绿，属净收益而非"顺带刷新"。

- **落地**：rebase 到 `opt/consolidated` 后 ff 合并，tip **`fd178425b`**（4 个提交）。
  合并后在**干净 worktree**（与集成分支同提交）复验：`check_fmt_ratchet.sh` = `current=0`；
  `check_route_contract.sh` **exit 0**（54 项守卫 + 变异检查全过、`ROUTE_CONTRACT.md` 已与源一致）；
  `test_extract_registered.py --mutation-check`、`gen_derived_routes.py --check` 均 exit 0；
  我的**独立变异**（删掉 `relations/{event_id}` 注解里的 `limit`）⇒ 抽取器 exit 1，
  同时报出「与 EXPECTED_ANNOTATIONS 不一致 / 与夹具不一致 / handler 实际解析
  `['dir','from','limit','to']`」三类失败 ⇒ 守卫双向且非自证。
- 下游消费已生效：`docs/openapi/route-table.json` 由新鲜导出重生成后，
  **1035 条中 106 条带非空 `query_params`**（此前恒空）。
- 遗留（诚实登记，非本轮范围）：`scripts/api_test/ledger.json` 仍是旧导出（1096 条、`query_params` 全空）；
  它未被任何漂移门禁对着新鲜导出校验，且是**受门禁保护的** `docs/openapi/client.yaml` 的输入，
  刷新它会让 `client.yaml` 的路由计数漂移 ⇒ 本轮**故意不动**，登记为后续独立项。

### 6.6 U-13 `event_id` reference hash —— **立项（排期在 M-1、U-1 之后）** ✅ 决策

**上游证据（配方可直接照抄）**
- `synapse/crypto/event_signing.py:114-137 compute_event_reference_hash`：
  `prune_event(event)`（套用 redaction 规则）→ `get_pdu_json()` → 去掉
  `signatures`/`age_ts`/`unsigned` → **canonical JSON** → `sha256` → 事件 ID 为
  `$` + **unpadded** URL-safe base64（spec room v4 "Event IDs" 明确 v4 起改为 reference hash）。
- 对照本仓 `synapse-common/src/crypto.rs:153`：对**所有**房间版本都用
  `$<ts>$<b64>:<server>` 随机 ID ⇒ v4–v11 房间的事件 ID 永远不是规范 ID。

**结论**：**必须立项**（否则 v11 PDU 即便字段齐全也无法被对等端当规范事件接受，§2.1 残余③），
但**不能**与 B1 同批做：它改的是事件身份本身。

**执行规格（分三步，每步可独立验收）**
1. 纯函数 + 已知答案测试：`compute_reference_hash(room_version, pdu_without_id)`，
   用 spec/Synapse 的 v4 与 v11 向量钉住（先不接线）。
2. 接线到 v4+ 的本地创建路径（`generate_event_id` 按 room_version 分流），
   破坏面清单必须逐项过：本地可见 `event_id`、`txn_id→event_id` 去重表、redaction 目标、
   缓存键、E2EE 引用、全部测试夹具与快照。
3. 互操作门槛：与真实对等端（docker dev stack + 对端 Synapse）跑 `/send_join` + `/send`，
   通过后方可切换；否则回滚（保留 `room_version < 4` 的随机 ID 分支）。

**风险**：这是本清单里**唯一**会改变既有数据语义的项；必须先做第 1 步并冻结测试向量。

**第 1 步执行结果（2026-09-25，分支 `feat/u13-reference-hash`）——✅ 完成，尚未接线**

- 实现（`synapse-common`，纯函数，无调用点）：
  - `redaction::redact_event(room_version, event)` + `redaction::redaction_rules(room_version)`：
    **按房间版本**的 redaction（单一实现位置，与 `redaction.rs` 原有的非版本化表同文件不同函数）。
    版本旗标直接对齐上游 `synapse/api/room_versions.py`（release-v1.161）命名：
    `updated_redaction_rules`(v11+)、`restricted_join_rule`(v8+)、`restricted_join_rule_fix`(v9+)、
    `implicit_room_creator`(v11+)、`special_case_aliases_auth`(v1–v5)、`room_ids_as_hashes`(v12)。
    未知/不支持版本**fail-closed**（v13 暂缺，见下）。
  - `event_id::{compute_reference_hash, encode_reference_hash_event_id, compute_event_id, uses_reference_hash_event_id}`：
    redact → 去掉 `signatures`/`unsigned`/`age_ts` → canonical JSON → sha256 → unpadded Base64，
    前缀 `$`；**v3 用标准 Base64，v4+ 用 URL-safe**（与上游一致）。
- **上游已知答案向量（外部 oracle，非本仓自算）**：Synapse `release-v1.161`
  `rust/src/events/utils.rs::test_calculate_event_id` 的事件与其两个期望值已固化为测试——
  v10 → `$zRz9jjiT9wZc3Hl9ij_74aCmTjqV3YMlj9sj3Uqxg6o`，
  v3 → `$zRz9jjiT9wZc3Hl9ij/74aCmTjqV3YMlj9sj3Uqxg6o`，**两者均通过**（证明 canonical JSON、
  redaction、sha256、两种 Base64 字母表四段全部与上游逐字节一致）。
  另外把上游 `redact()` 的全部单测期望（member/create/join_rules/power_levels/aliases/redaction/
  history_visibility/`prev_state`+`membership`+`origin` 的 v10↔v11 差异）逐一转成 v1–v12 版本矩阵断言。
- 门禁：`cargo nextest -p synapse-common --lib -E 'test(/event_id|room_version_redaction/)'` **24/24 绿**；
  `./scripts/check_fmt_ratchet.sh` = 0；workspace clippy `-D warnings` exit 0。
  **变异自证**（3 个，均转红后还原）：① 令 `restricted_join_rule_fix` 恒 false → 1 red；
  ② reference hash 不去 `signatures` → 4 red；③ v3/v4+ Base64 字母表互换 → 4 red。

- **新发现（第 2 步必须先处理，否则接线即错）**：
  1. **既有非版本化表与上游不符**：`redaction::allowed_content_keys` / `redact_event_for_hash`
     （`synapse-federation/src/signing.rs:82` 签名路径、`synapse-storage/src/event/redaction.rs:103`
     运行期 redaction 路径都在用）把 v1–v10 当成**一张表**，而规范/上游是分级的
     （v6 去掉 `m.room.aliases` 特例；v8 加 `join_rules.allow`；v9 加
     `member.join_authorised_via_users_server`；v11 才加 `power_levels.invite`、
     `redaction.content.redacts`、`create` 全内容、去 `origin`/`membership`/`prev_state`）。
     该表里的 `m.room.encrypted`/`m.room.third_party_invite`/`member.displayname` 等键
     **任何稳定房间版本都不保留**（上游 `redact()` 无对应分支）。
     ⇒ 第 2 步应把这两条路径迁移到 `redact_event(room_version, …)` 并删除旧表
     （铁律 2：同一职责只允许一份实现），届时签名材料与运行期 redaction 的行为会**改变**，
     必须在同批复核所有断言/夹具。
  2. **`hashes` 与 `signatures` 的语义与上游互为镜像地反了**（同一区域，第 2 步必须一并修）：
     - ✅ **content hash 半边已修（`0880f6a5f`）**：`compute_event_content_hash` 原先**先 redact 再算**
       `hashes.sha256`，而上游 `compute_content_hash` 是**对未 redact 的事件**
       （仅去 `age_ts`/`unsigned`/`signatures`/`hashes`/`outlier`/`destinations`）取 canonical JSON 哈希。
       现按上游重写，并用上游 `tests/crypto/test_event_signing.py` 的两个已知答案向量钉住
       （`mq4QfPPpC+QsBd6eqfVsmJIEz8uvMSVK0+AU67PLESk`、`rDCeYBepPlI891h/RkI2/Lkf9bt7u0TxFku4tMs7WKk`，均通过）；
       变异自证：换回旧实现 ⇒ 两向量转红（**实测证明旧实现产出的 hash 对等端无法复现**）。
       同批删除了只为它存在的 `redaction::redact_event_for_hash` 与 `CANONICAL_JSON_TOP_LEVEL_FIELDS`。
     - ⬜ **签名半边未修**：`signing.rs:194-224 sign_and_hash_event` 第 3 步用
       `CanonicalEvent::from_event`（`synapse-common/src/canonical_json.rs:140-148`）签名，
       而它**只去 `signatures`/`unsigned`、不 redact、且保留 `event_id`**；
       上游 `compute_event_signature` 用的是 `redact_event_dict(room_version, event_dict)`
       （并对 v3+ 的联邦 PDU 而言根本没有 `event_id` 可签）。
       需给 8 个调用点接入房间版本（`federation_broadcast.rs:163`、`pdu.rs:243`、
       `membership/{invite.rs:237,mod.rs:225,federation.rs:79,346,473}`），
       故与"单一写入口定 event_id"同批做，避免为同一职责造第二份解析。
     ⇒ 在签名半边修好前，本仓产出的 `signatures` **不可能被对等端校验通过**；`hashes` 半边已不再
     是阻塞项。这也是第 3 步互操作门槛不可省的原因。

     **签名半边逐站点清单（本轮实测，决定 plumb 方式）**：

     | # | 站点 | 房间版本是否已在作用域 |
     |---|---|---|
     | 1 | `services/room/federation_broadcast.rs:163`（`sign_and_broadcast_event`，B1 新代码） | ❌ 需从 `event.room_id` 解析 |
     | 2 | `web/routes/federation/pdu.rs:243` | ❌ 需解析 |
     | 3 | `web/routes/federation/membership/invite.rs:237` | ✅ 变量 `room_version` 已在作用域 |
     | 4 | `web/routes/federation/membership/mod.rs:225` | ❌ 需解析 |
     | 5 | `services/room/membership/federation.rs:79`（join 模板） | ✅ `make_join_response.room_version`（⚠️ 缺省值硬编码 `"10"`） |
     | 6 | `services/room/membership/federation.rs:346`（leave 模板） | ❌（`make_leave_response` 侧应可取） |
     | 7 | `services/room/membership/federation.rs:473`（federation invite） | ❌（模板还是 `prev_events: []`/`depth: 0`） |

     设计结论：**先收敛房间版本解析**——本仓现有三份近重复实现
     （`room/state/info.rs:269`、`auth/power_levels.rs:97`、B1 的
     `graph_metadata.rs:76 GraphMetadataSource::room_version`），第 2 步应只保留**一份**
     带缓存的解析器（铁律 2），签名与 event_id 两条路径共用它；否则会给同一职责造出
     第二/第三份解析。

     附带新发现（同批自愈）：`whitelist` 之外，`invite.rs:222-231` 把
     `"room_version"` 当作**顶层 PDU 字段**写进了被签名的事件 JSON。规范 PDU 无此字段；
     上游签名是"先 redact"，redaction 会把这个未知顶层字段丢掉，因此签名半边改成
     redact 后该字段不再进入签名字节（但也不应再发出去，第 2 步一并清理）。

     **第 2 步目标算法（本轮据 spec 与上游定稿，作为实施与验收依据）**：

     1. 组装 PDU：字段齐全（含 depth/prev_events/auth_events），**不含 `event_id`**
        （v3+ 的联邦 PDU 无此字段）。依据：spec room v3「Event format」——
        "When events are sent over federation, the `event_id` field is no longer included.
        A server receiving an event should compute the relevant event ID for itself."
        这一步是本仓当前最大的结构性偏差：30 个调用点都是**先**拿到 `event_id` **再**组装。
     2. `hashes.sha256` = sha256(canonical(第 1 步的 PDU))（✅ 语义已修，见前置②）。
     3. 签名材料 = `redact_event(room_version, PDU+hashes)` 去掉 `age_ts`/`unsigned`
        （此时已无 `event_id` 可签）；`signatures` 亦是**签完才写入**。
     4. `event_id` = `"$"` + unpadded Base64(sha256(canonical(第 3 步材料去掉 `signatures`)))
        ——即 `event_id::compute_event_id(room_version, PDU_with_hashes)`（第 1 步已实现并冻结向量）。
     5. 落库时写入第 4 步的 `event_id`；对外发送的 PDU 仍不含该字段。

     ⇒ **推论（必须写进验收）**：前置②只修了第 2 步的语义；只要第 1 步仍带 `event_id`，
     本仓 v3+ 的 `hashes`/`signatures` **依旧不可能**被对等端复现（因为对等端手里没有该字段）。
     即"content hash 已对齐"**不等于**"联邦可校验"。唯一正确的落点是 B1 的单写入口
     `GraphMetadataWriter::create_event`：它已经能拿到房间版本与全量图字段，把 1–4 步连成一条链，
     并把第 4 步的 id 返回给调用方（30 个调用点改为消费写入口返回的 id，而非自己生成）。
     同时删掉 `crypto::generate_event_id` 的 v3+ 使用面（v1/v2 保留随机 ID 分支）。

     **第 2 步当前阻塞条件（2026-09-26 实测，非推测）**：并发会话正在**未提交**地给同一条缝加方法——
     `EventWriter::create_event_with_pdu(params, pdu_graph, tx)`（`synapse-storage/src/event/writer.rs`
     的 trait 新增 + `EventStorage` 实现、`notifying_event_writer.rs` 透传+发布、`graph_metadata.rs`
     透传），共 6 个文件 169 行在途，全部落在本项要改的那几个文件上。故第 2 步**必须等该 trait 变更落地后**
     在同一缝上做（它正是"单写入口"的扩展点）；现在动手必然与其冲突、且会产出两份缝。
     在它落地前，第 2 步的可做工作只有本文档已完成的冻结清单与算法（无代码改动）。

     **第 2 步验收测试清单（先定后做，避免"改完再想怎么证"）**：
     1. 上游 v1 签名已知答案向量（`tests/crypto/test_event_signing.py::test_sign_minimal` /
        `test_sign_message` 的 `signatures[...]` 期望值）逐字节通过——v1 的签名字节**包含** `event_id`，
        正好覆盖"v1/v2 保留 event_id"分支；
     2. v3+ 签名字节**不含** `event_id`：对同一 PDU，去掉/加上 `event_id` 必须得到**相同**签名与相同 `hashes`；
     3. v10 与 v11 对含 `origin` 的同一事件产出**不同**签名（证明签名材料走了版本化 redaction）；
     4. 自洽环：`finalize(pdu)` 产出的 `event_id`，必须等于对**同一产出 PDU 去掉 `event_id`** 重算的
        reference hash（用第 1 步已冻结的 `compute_event_id`）；
     5. 出站投影可被本仓入站校验器接受：`verify_event_content_hash` 通过、`verify_pdu_signature_*` 通过；
     6. 回归面：v1/v2 仍走随机 ID 分支且行为不变；30 个调用点改为消费写入口返回的 id 后，
        全部既有事件相关测试（含夹具/快照）复核过。
  3. ✅ **v12 语义已裁定（2026-09-26，权威来源）**：**MSC4304 = Room Version 12**，
     以 v11 为基座并纳入 MSC4289（creator 特权）、**MSC4291（room ID = create 事件的哈希）**、
     MSC4297（state res v2.1）、MSC4307（`auth_events` 同房间校验）；
     而 **MSC4239 是 Room Version 11（把 v11 设为默认）**——此前本仓注释与本文档把两者混为一谈
     （已在本轮修正 `room_versions.rs` / `redaction.rs` 注释）。因此本仓实现里
     `room_ids_as_hashes = true`（v12 的 `m.room.create` 计算 reference hash 时丢 `room_id`）
     **与规范及上游 Synapse 一致，不是猜测**。
     仍待第 2 步确认的是：本仓 v12 的**实现**（O-1 只描述了"完整 PDU 字段 + ED25519-only"）
     是否真的落地了 MSC4304 的四项（尤其 MSC4291 的 room_id 派生与 MSC4289 的 creator 特权），
     这属于 O-1/并发会话的实现面。
     v13 现仍 **fail-closed**（不猜）。

### 6.7 决策后的执行顺序（更新 §4.1）

```
M-1 合并 B1（前置，阻塞以下全部）
 ├─ U-7（配额错误码，最小改动，先做以打通 media 测试面）
 ├─ U-4（profile 两个 errcode + 校验）
 ├─ U-1（MSC3912 客户端级联：storage 查询+索引 → 服务 → 路由 → /versions 标志）
 ├─ U-3（扫描：拒绝 + 隔离裁定 + hash 级自动隔离 + 指标 + 显式配置）
 ├─ U-10（query_params 填值 + 守卫；与 Task 8 `animated` 合并做）
 └─ U-13（reference hash：①纯函数+向量 → ②接线 → ③互操作门槛）
```

---

## 附录 A：本轮核验的可复现命令

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# A. E2EE SAS 对象消失
git status --short synapse-e2ee/src/verification synapse-e2ee/src/device_trust synapse-web/src/routes/verification_routes.rs
grep -rn "verification.commitment\|% 64\|generate_decimal_from_emoji\|compute_shared_secret" --include=*.rs .   # 0 命中

# B. 客户端撤回不级联 / 级联能力存在但仅管理端
grep -rn "cascade" --include=*.rs synapse-web/src/routes/ | grep -v derived_route_table
sed -n '954,1001p' synapse-web/src/routes/handlers/room/events.rs

# C. Content Scanner 零生产调用点
grep -rn "scan_media\|scan_text\|\.scan(" --include=*.rs . | grep -v "content_scanner/service.rs"
grep -rn "\.content_scanner" --include=*.rs .

# D. dag.rs 注释幻觉
sed -n '203,208p' synapse-storage/src/event/dag.rs
grep -rn "get_state_dag_edges\|get_prev_state_events\|find_events_referencing_missing_state" --include=*.rs .

# E. auth_issuer / auth_metadata
grep -rn "auth_issuer\|auth_metadata" --include=*.rs synapse-web/src/routes/assembly.rs

# F. Profile / MSC4133
grep -n "is_deactivated" synapse-storage/src/user/storage.rs | sed -n '1,5p'
grep -rn "M_PROFILE_TOO_LARGE\|M_KEY_TOO_LARGE\|\"m\.tz\"" --include=*.rs .    # 0 命中

# G. Admin 媒体
grep -n "route(" synapse-web/src/routes/admin/media.rs

# H. 出站 PDU / 两份 sign_and_broadcast_event
sed -n '156,175p' synapse-services/src/room/messaging/service.rs
grep -rn "fn sign_and_broadcast_event" --include=*.rs .

# I. 本地事件图元数据
sed -n '13,20p;82,89p' synapse-storage/src/event/create.rs

# J. event_id 格式
sed -n '148,154p' synapse-common/src/crypto.rs
```

## 附录 B：主源（本轮外部核对）

| 事实 | 主源 |
|---|---|
| MSC3912 客户端 `with_rel_types` / 单层语义 / `*` / 未稳定名 | [matrix-spec-proposals/3912](https://raw.githubusercontent.com/matrix-org/matrix-spec-proposals/1b3176cfe105a8d572ce27052f3b9e5e44cc0d9c/proposals/3912-relation-based-redaction.md) |
| Synapse 的 MSC3912 实现与开关默认值、后台执行、权限忽略、未做"后到补撤" | `element-hq/synapse@release-v1.161`：`synapse/rest/client/room.py:1374-1415`、`synapse/handlers/relations.py:191-263`、`synapse/config/experimental.py:160`；[issue #15687](https://github.com/element-hq/synapse/issues/15687) |
| MSC4133 已进 spec v1.16（`/profile/{userId}/{keyName}`、`m.tz`、两个 errcode、`field=`） | matrix-spec `data/api/client-server/profile.yaml:19,22,104-106,312-316` |
| 缩略图 `animated` 语义 | matrix-spec `data/api/client-server/content-repo.yaml:436-453,497-502` |
| 上游已删 `auth_issuer`、profile 停用用户修复 | `element-hq/synapse@release-v1.161/CHANGES.md:48`（#20163）、`:36`（#20172） |
| **v12 房间版本实现** | `element-hq/synapse@release-v1.162`：`CHANGES.md` "Raise default room version to '12'"；**MSC4304（= v12 定义；基座 v11 + MSC4289/4291/4297/4307）**、MSC4311、MSC3912。⚠️ MSC4239 是 **v11** 的定义，勿再误引 |
| **动画缩略图实现** | `element-hq/synapse@release-v1.161`：`synapse/media/thumbnailer.py`、`synapse/rest/media/thumbnail_resource.py` |
| Synapse `/versions` 只到 v1.12；稳定 profile `{keyName}` 无条件提供；`m.profile_fields` capability | `element-hq/synapse@release-v1.161`：`rust/src/handlers/versions.rs:146-157`、`synapse/rest/client/profile.py:104-106,113-117`、`synapse/rest/client/capabilities.py:95-101` |
| Synapse 媒体只有 `max_upload_size`（无总量配额）；隔离按 sha256 复用（`get_is_hash_quarantined`） | 同上：`synapse/media/media_repository.py:106,353-359,429,439,507,782,921,1043` |
| reference hash 的精确配方（prune → canonical JSON → sha256 → unpadded base64） | 同上：`synapse/crypto/event_signing.py:114-137`；Matrix spec `content/rooms/v4.md`「Event IDs」 |
| Synapse 无内置内容扫描器（扫描属 out-of-tree 模块/代理） | 同上：`synapse/media/` 目录清单（无 scanner 模块） |
| 上游 admin 媒体端点面（15 条） | `element-hq/synapse@release-v1.161/docs/admin_api/media_admin_api.md`、`user_admin_api.md:756,883` |
| 上游 MSC3912 未稳定名（稳定名待更新） | [element-hq/synapse#15687](https://github.com/element-hq/synapse/issues/15687)（open） |
| **MSC4133 实现状态** | `element-hq/synapse@release-v1.162`：`synapse/rest/client/profile.py:103-114`；**实验开关 `msc4133_enabled` 下才支持不稳定前缀 `uk.tcpip.msc4133`，非稳定端点** |
| **动画缩略图格式** | `element-hq/synapse@release-v1.161`：`synapse/media/thumbnailer.py`；`ANIMATED_FORMATS = {"GIF","PNG","WEBP"}`，输出 `image/webp`，保留帧延迟和循环次数 |

---

## 附录 C：上游 Synapse v1.162 对齐差距分析

### C.1 v12 房间版本差距

**上游现状** (Synapse v1.162.0rc1, 2026-09-22):
- ✅ **默认版本已提升至 v12** (`CHANGES.md`: "Raise default room version to '12'")
- ✅ **核心 MSC**: **MSC4304 (v12 定义，含 MSC4291 room ID = create 事件哈希)**、MSC4311 (邀请/敲击状态)、MSC3912 (基于关系的撤回)；MSC4239 属 v11
- ⚠️ **MSC4311 宽限期**: 2027-06-01 前仅对邀请/敲门应用宽松验证 ([#19723](https://github.com/element-hq/synapse/issues/19723))
- 🔑 **关键变更**:
  - ED25519-only 签名验证 (更严格的算法白名单)
  - PDU 验证增强 (`depth`、`prev_events`、`auth_events` 必须正确填充)
  - MSC4311: `m.room.create` 现在必须出现在 v12 房间的 stripped invite/knock state 中

**我们项目现状**:
```rust
// synapse-common/src/room_versions.rs:89
pub const DEFAULT_ROOM_VERSION: &str = "11";  // ❌ 仍是 v11

// synapse-common/src/room_versions.rs:114-115
RoomVersionCapability::stable_parse_only("12"),  // ❌ 不可创建
RoomVersionCapability::stable_parse_only("13"),  // ❌ 不可创建
```

**差距**:
1. ❌ 无法创建 v12 房间 (`stable_parse_only`)
2. ❌ `create_event` 不填充 `depth`/`prev_events`/`auth_events` (本地起源 PDU 恒 `MissingGraphMetadata`)
3. ❌ 无 ED25519-only 强制验证
4. ❌ 无 MSC4311 合规性检查

**建议行动**:
- **阶段 1**: 实现 v12 事件验证 (depth 计算、auth_events 构造、ED25519-only)
- **阶段 2**: 将 `DEFAULT_ROOM_VERSION` 改为 "12"
- **阶段 3**: 联邦兼容性测试

### C.2 动画缩略图差距

**上游实现** (Synapse Python):
- **检测机制**: GIF/PNG/WebP 三格式支持，通过 `is_animated` 属性判断
- **请求参数**: `animated=true/false` (默认 false)
- **输出格式**: 
  - 静态：JPEG/PNG/WebP
  - 动画：**始终 WebP** (`ANIMATED_THUMBNAIL_TYPE = "image/webp"`)
- **帧处理**: 逐帧缩放/裁剪，保留帧延迟 (`duration`) 和循环次数 (`loop`)
- **降级策略**: 动画解码失败 → 自动回退到首帧静态缩略图

**我们项目现状**:
```rust
// synapse-web/src/routes/media/download.rs
pub(crate) fn thumbnail_request_params(params: &Value) -> (u32, u32, &str) {
    // ❌ 没有 animated 参数解析
}

// synapse-services/src/media_service.rs
fn generate_thumbnail(...) -> Result<Vec<u8>, ApiError> {
    // ❌ 总是输出 JPEG
    // ❌ 没有动画检测
    // ❌ 没有帧处理
}
```

**差距**:
1. ❌ 无 `animated` 查询参数支持
2. ❌ 无 GIF/APNG/WebP 动画检测
3. ❌ 无逐帧处理逻辑
4. ❌ 无 WebP 动画编码能力

**建议行动**:
- **阶段 1**: 添加 `animated` 参数解析和传递
- **阶段 2**: 实现动画检测 (依赖 `image` crate 的动画迭代支持)
- **阶段 3**: 实现 WebP 动画编码

### C.3 MSC4133 Profile API 差距

**上游实现** (Synapse v1.162):
- **端点**: 
  - 稳定：`GET /_matrix/client/v3/profile/{userId}` (已有)
  - 不稳定：`GET /_matrix/client/unstable/uk.tcpip.msc4133/profile/{userId}/{field}` (实验开关下)
- **实验开关**: `msc4133_enabled` 控制不稳定前缀是否注册
- **错误码**: `KEY_TOO_LARGE` 用于字段名过长，**未使用 `M_PROFILE_TOO_LARGE`**
- **字段支持**: `displayname`, `avatar_url`, 自定义字段 (需符合命名空间语法)
- **速率限制**: 新增 `rc_profile` 配置
- **停用用户**: 不再过滤 `is_deactivated` (#20172)

**我们项目现状**:
```rust
// synapse-web/src/routes/assembly.rs:231
// 已注册稳定 `/{keyName}` 路由 ✅

// synapse-storage/src/user/storage.rs:698
// 已移除 `is_deactivated` 过滤 ✅

// 全仓搜索
grep -rn "M_PROFILE_TOO_LARGE\|M_KEY_TOO_LARGE\|\"m\.tz\"" --include=*.rs .  # 0 命中 ❌
```

**差距**:
1. ❌ 无 `M_PROFILE_TOO_LARGE`/`M_KEY_TOO_LARGE` 错误码 (上游也未完全实现)
2. ❌ 无 `m.tz` 字段支持 (Spec v1.16 已定义但未实现)
3. ⚠️ 缺少不稳定 MSC4133 前缀端点 (可选，取决于是否需要实验功能)

**建议行动**:
- **优先级低**: MSC4133 在上游仍处于实验阶段，可暂缓实现
- **建议**: 记录为"已知差距"，待上游稳定后再跟进

### C.4 两份 `sign_and_broadcast_event` 实现策略冲突

**问题描述**:
发现两个 `sign_and_broadcast_event` 实现，**策略相反**：

1. **messaging/service.rs:131** (fail-closed):
```rust
let prev_events = match self.event_reader.get_latest_event_ids_in_room(...) {
    Ok(events) => events,
    Err(e) => {
        tracing::warn!(error = %e, "Failed to fetch prev_events; skipping broadcast");
        return Ok(());  // ❌ 直接返回，不广播
    }
};
```

2. **membership/service.rs:486** (fail-open):
```rust
let prev_events = match self.event_reader.get_latest_event_ids_in_room(...) {
    Ok(events) => events,
    Err(e) => {
        tracing::warn!(error = %e, "PDU may be incomplete");
        Vec::new()  // ✅ 继续，允许空 prev_events
    }
};
```

**影响**:
- **messaging 版**: 数据库错误时静默跳过广播 (可能丢失联邦消息)
- **membership 版**: 数据库错误时发送空 `prev_events` 的 PDU (产生无效 PDU)
- **两者都缺 `depth`/`auth_events`** (N-1 问题)

**建议行动**:
- **统一为 fail-closed**: 数据库错误时不应发送无效 PDU
- **补全 PDU 字段**: 添加 `depth` 和 `auth_events`
- **合并实现**: 消除重复代码，单一来源

---

## 附录 D：优化方案更新

### D.1 新增优化项

| 编号 | 级别 | 问题 | 建议行动 | 优先级 |
|------|------|------|----------|--------|
| **O-1** | **P0** | v12 房间版本默认仍为 v11 | 阶段 1: 实现 v12 验证；阶段 2: 升级默认版本 | **高** |
| **O-2** | 中 | 动画缩略图缺失 | 阶段 1: 参数支持；阶段 2: 动画检测；阶段 3: WebP 编码 | 中 |
| **O-3** | 低 | MSC4133 不完整 | 记录为已知差距，待上游稳定 | 低 |
| **O-4** | **高** | 两份 `sign_and_broadcast_event` 策略冲突 | 统一为 fail-closed，补全 PDU 字段 | **高** |

### D.2 优先级调整

**推荐执行顺序**:
1. **O-4**: 统一 `sign_and_broadcast_event` (高优先级，影响联邦正确性)
2. **O-1**: v12 房间版本升级 (P0 优先级，安全相关)
3. **O-2**: 动画缩略图 (中优先级，用户体验)
4. **O-3**: MSC4133 完整实现 (低优先级，上游未稳定)

### D.3 时间估算更新

| 优化项 | 估计时间 | 依赖 |
|--------|---------|------|
| O-4: 统一 sign_and_broadcast_event | 1 周 | 无 |
| O-1: v12 房间版本 | 4-6 周 | 事件认证、存储层 |
| O-2: 动画缩略图 | 2-3 周 | image crate 功能 |
| O-3: MSC4133 | 2 周 | 上游稳定 |

---
---

## 总结

### 本轮核验结果

| 类别 | 数量 | 说明 |
|------|------|------|
| 报项总项数 | 13 | 来自 `PROJECT_REMAINING_ISSUES_2026-09-14.md` |
| 确认存在 | 8 | 客户端撤回不级联、Content Scanner、dag.rs、auth_issuer、Profile、Admin 媒体、animated+配额、search_index |
| 部分成立 | 3 | P0 PDU（残余+新增）、Admin 媒体（15 条可核验）、animated+配额（两项均缺） |
| 证伪/作废 | 2 | MSC4502/4262（证伪）、E2EE SAS（对象删除） |
| 新发现 | 8 | N-1~N-8（P0~低级别） |
| 优化项 | 4 | O-1~O-4（v12、动画缩略图、MSC4133、sign_and_broadcast_event） |

### 最关键的三条

**⚠️ 最紧迫：O-4 (P0，1 周，无依赖)**
- **问题**: 两份 `sign_and_broadcast_event` 实现策略冲突（fail-closed vs fail-open）+ 出站 PDU 缺 `depth`/`auth_events`
- **影响**: 直接导致联邦 PDU 无效，影响服务器间互通
- **行动**: 本周内统一实现，补全 PDU 字段

1. **O-1 (P0)**: v12 房间版本默认仍为 v11，而上游 Synapse v1.162 已提升至 v12。差距 4-6 周工作量。
2. **N-2 (高)**: 两份 `sign_and_broadcast_event` 策略相反（fail-closed vs fail-open），违反反冗余铁律。

### 下一步行动

1. **🔴 立即**: 开始 O-4 统一 `sign_and_broadcast_event`（1 周，无依赖）
2. **🔴 同时**: 合入 `fix/local-event-graph-metadata` 分支 (M-1)
3. **本周**: 完成 O-4 + M-1
4. **本月**: 完成 O-1 v12 房间版本阶段 1
5. **下月**: 完成 O-1 v12 阶段 2 + O-2 动画缩略图

---

**文档版本**: v1.0 (2026-09-25)  
**下次复核**: 建议在每个优化项完成后更新  
**主要作者**: Audit Team  
**审阅**: 待用户确认
