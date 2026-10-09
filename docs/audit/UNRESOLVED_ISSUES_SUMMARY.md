# Synapse-Rust 当前仍存问题总结

> 权威来源：本文件为「当前仍存问题」的权威清单。  
> **最后更新**: 2026-10-09（增量见 §8）  
> **基准**: Synapse v1.162.0 / Matrix Spec v1.15（v12 房间版本）  
> **对应全面报告**: [`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md)  
> **排查手段**: 静态代码审查 + 现有门禁/扫描（clippy、cargo deny/audit、machete、ci 脚本）；未执行全量测试与性能基准  
> **定级总览**: **0 个 P0**、**0 个 P1**（原 3 条已于 2026-10-06 全部闭环，见 §2）、**26 个 P2**（**已于 2026-10-06 全部闭环**，最后一项 `COMPAT-05` 经报告 §8.11 复核闭环，见 §3.4）、**27 个 P3**（**已于 2026-10-06 全部处置**：21 条闭环 + 6 条 by-design 正式登记，见 §4 与报告 §8.11）；本轮复核推翻 **9 条**历史结论（见 §5）。**2026-10-09 增量复核新增登记 2 条 P3（`DOC-12` / `DOC-13`，见 §8.2），未新增 P0/P1/P2。**

---

## 1. P0 级问题

**无。** 上一版登记的 P0（RBAC `login_as_user` 冒充超管）已于本轮回复核为**已修复**（见 §5.1）。

---

## 2. P1 级问题（原 3 条，已于 2026-10-06 全部闭环）

### 2.1 联邦 `/send_join` 响应缺失规范必需的 `event` 字段

**编号**: `FED-01`　**状态**: ✅ **已闭环（2026-10-06）**（同时闭环权威清单 P0-1）

**问题描述**:

- `/send_join` 的 v1/v2 响应体仅含 `origin` / `room_id` / `event_id` / `state` / `auth_chain`，**缺少规范要求的 `event`**（已签名的 `m.room.member` join PDU 正文）。
- 代码已通过 `re_sign_pdu_locally` 完成本地签名，却未将签好的事件放入响应。

**影响范围**: 远端发起入群的服务器无法取得/校验 join 事件 PDU，可能拒绝完成入群或导致 DAG 断裂 → **跨服入群互操作失败**。

**修复**:

- `join.rs` v1/v2 均改用 [`project_and_sign_pdu_locally`](../../synapse-web/src/routes/federation/membership/mod.rs#L230)（返回签名后 PDU），并在响应对象追加 `"event": signed_pdu`；签名缺失即 `ApiError::internal` 硬失败（与 invite 端点既有约定一致）。
- 集成测试 [`federation_existence_leak_tests.rs`](../../tests/integration/federation_existence_leak_tests.rs) 断言 `event` 存在，且为 join `m.room.member`、带非空 `hashes`/`signatures`。
- 门禁验证通过：`cargo clippy -p synapse-web --all-targets --all-features --locked -- -D warnings`、`cargo clippy --test integration --all-features --locked -- -D warnings` 均无告警。

---

### 2.2 `.trae/rules/project_rules.md` 目录结构与迁移计数严重失实

**编号**: `DOC-01`　**状态**: ✅ **已闭环（2026-10-06）**

**问题描述**: 该文件为每次会话加载的 workspace 常驻规则，但其 §1.3 目录结构严重偏离现实：

- 称 `migrations/`「约 50 个」迁移文件（**实际仅 1 个 SQL**：`00000000_unified_schema_v12.sql` 及 2 个 md）。
- 列出 `src/storage/`、`src/services/`、`src/web/routes/` 等**实际不存在的路径**（代码已迁至 workspace crate）。
- §7.0 链接 `src/services/container.rs` 不存在（实为 `synapse-services/src/container.rs`，另见 `DOC-06`）。

**影响范围**: 误导所有后续 agent/开发者，与「文档即事实来源」的项目自述冲突，削弱整个文档体系可信度。

**修复**: `.trae/rules/project_rules.md` 升至 v2.5.0——§1.3 按真实 workspace 布局重写（8 个成员 crate + 根薄装配层；`migrations/` 注明「1 个统一基线 SQL」）；§7.0 链接修正为 [`synapse-services/src/container.rs`](../../synapse-services/src/container.rs)；版本历史追加 2.5.0 行。

---

### 2.3 `UNRESOLVED_ISSUES_SUMMARY.md`（本文）曾含过时结论与破路径

**编号**: `DOC-02`　**状态**: ✅ **本次已刷新**（登记为流程缺陷，防漂移门禁见 `DOC-11`）

**问题描述**: 作为权威问题清单，此前多条标「仍存」的条目经复核实为**已修复**，且引用的代码路径已不存在（`synapse-web/src/routes/room/messaging/messages.rs` 整个目录已不存在）。

**建议方案**: 本次已同步刷新（移除 5 条已解决/过时项、修正破路径）；后续将其纳入文档防漂移门禁，在 CI 中断言引用的 `file:line` 可达。

---

## 3. P2 级问题（26 条，分域摘要）

> 完整描述/影响/证据/建议见全面报告 §2.B。

### 3.1 安全（1，`SEC-01` 已闭环）

| 编号           | 问题                                                                                                                                                                             | 证据                                                           |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------ |
| ~~`SEC-01`~~ | ✅ **已闭环（2026-10-06）**：`upsert` 前新增 32 字节 ed25519 公钥校验（[`edu.rs:625`](../../synapse-web/src/federation/edu.rs#L625) `decode_base64_32`，失败 `dropped += 1` + `warn!`），拒绝畸形/超长载荷落库 | [`edu.rs:625`](../../synapse-web/src/federation/edu.rs#L625) |

### 3.2 功能完整性与正确性（1，`FED-02` 已闭环）

| 编号           | 问题                                                                                                                                                                                                                                                                                                                                                                                             | 证据                                                                                                                                                                         |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ~~`FED-02`~~ | ✅ **已闭环（2026-10-06）**：新增共享 [`verify_inbound_join_pdu_integrity`](../../synapse-web/src/routes/federation/membership/mod.rs#L157) 并接入 `send_join` v1/v2（[`join.rs:155`](../../synapse-web/src/routes/federation/membership/join.rs#L155)、[:307](../../synapse-web/src/routes/federation/membership/join.rs#L307)）；采「有则校验/无则放行」——入站带 `hashes` 校内容哈希、`signatures` 覆盖 sender 服务器才校发送者签名，兼容规范的模板请求体 | [`membership/mod.rs:157`](../../synapse-web/src/routes/federation/membership/mod.rs#L157)、[`join.rs:155`](../../synapse-web/src/routes/federation/membership/join.rs#L155) |

### 3.3 性能与稳定性（6，已全部闭环）

| 编号            | 问题                                                                                                                                                                                                                                                                                                                                                                  | 证据                                                                                                                                            |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| ~~`PERF-01`~~ | ✅ **已闭环（2026-10-06）**：合并为单查询 `WHERE token_hash IN ($1, $2) AND is_revoked = FALSE ORDER BY (token_hash = $1) DESC LIMIT 1`（当前哈希优先），热路径 DB 往返 **2 → 1**；`.sqlx` 已重生成、门禁通过                                                                                                                                                                                            | [`token.rs:141`](../../synapse-storage/src/token.rs#L141)                                                                                     |
| ~~`PERF-02`~~ | ✅ **已闭环（2026-10-06）**：[`00000000_unified_schema_v12.sql`](../../migrations/00000000_unified_schema_v12.sql#L3264) 新增复合索引 `idx_events_type_origin_ts(event_type, origin_server_ts DESC)`，覆盖「`event_type` 等值 + 时间范围」双谓词；[`INDEXES.md`](../../migrations/INDEXES.md) 计数 350/140 → **351/141**                                                                          | [`00000000_unified_schema_v12.sql#L3264`](../../migrations/00000000_unified_schema_v12.sql#L3264)、[`INDEXES.md`](../../migrations/INDEXES.md) |
| ~~`PERF-03`~~ | ✅ **已闭环（2026-10-06）**：[`token.rs:171`](../../synapse-storage/src/token.rs#L171) `get_user_tokens` 加 `ORDER BY id ASC LIMIT $2`（`MAX_USER_TOKENS=10_000`）；[`background_update.rs:394`](../../synapse-storage/src/background_update.rs#L394) `get_updates_by_status` 加 `LIMIT $2`（上限 10_000）；命中即 `warn`。同批消除 `#[cfg(test)] get_total_message_count` 的静态宏 `.sqlx` 悬空隐患 | [`token.rs:171`](../../synapse-storage/src/token.rs#L171)、[`background_update.rs:394`](../../synapse-storage/src/background_update.rs#L394)   |
| ~~`PERF-04`~~ | ✅ **已闭环（2026-10-06）**：连接预算门禁改为解析真实运行配置（`database.rs` 默认 / `homeserver.yaml` / `docker-compose.yml` 三源一致性 + `postgres.conf` 的 `max_connections`），断言「文档 == 代码 == 运行」；同轮修掉真实漂移 `postgres.conf` 200→250                                                                                                                                                                 | [`check_connection_budget.py`](../../scripts/ci/check_connection_budget.py)、[`postgres.conf`](../../docker/config/postgres.conf)              |
| ~~`PERF-05`~~ | ✅ **已闭环（2026-10-06）**：基线参数（`8`/`4`/`20`）抽到 `scripts/ci/memory_budget_baseline`（缺失/缺项 fail-closed），`high` 风险改为阻断（exit 1）                                                                                                                                                                                                                                             | [`check_memory_budget.py`](../../scripts/ci/check_memory_budget.py)、[`memory_budget_baseline`](../../scripts/ci/memory_budget_baseline)       |
| ~~`PERF-06`~~ | ✅ **已闭环（2026-10-06）**：[`PERFORMANCE_BASELINE.md:58`](../../docs/PERFORMANCE_BASELINE.md#L58) 升 v1.1，更正 `5/100 (5%)` → **`5/50 (10%)`**（分母取 `DatabaseConfig::max_size` 默认 50），加注依据与部署不变式                                                                                                                                                                             | [`PERFORMANCE_BASELINE.md:58`](../../docs/PERFORMANCE_BASELINE.md#L58)                                                                        |

### 3.4 兼容性与可访问性（6，其中 `COMPAT-01`/`COMPAT-02`/`COMPAT-03`/`COMPAT-04`/`COMPAT-06` 已闭环）

| 编号              | 问题                                                                                                                                                                                                                                                                                                                                                                                                                                   | 证据                                                                                                                                                              |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ~~`COMPAT-01`~~ | ✅ **已闭环（2026-10-06）**：复核为**误报**——MSC4155 官方语义（邀请过滤）**已实现**（[`account_data_service.rs:246`](../../synapse-services/src/account_data_service.rs#L246) 写入校验 + [`invite_blocklist_service.rs:13`](../../synapse-services/src/invite_blocklist_service.rs#L13) / [`membership/service.rs:86`](../../synapse-services/src/room/membership/service.rs#L86) 消费）；**保留能力广告**，订正 [`MSC_SEMANTICS.md`](../../docs/synapse-rust/MSC_SEMANTICS.md) | [`MSC_SEMANTICS.md`](../../docs/synapse-rust/MSC_SEMANTICS.md)                                                                                                  |
| ~~`COMPAT-02`~~ | ✅ **已闭环（2026-10-06）**：[`config/server.rs:155`](../../synapse-common/src/config/server.rs#L155) 新增 `support_url: Option<String>`；[`versions.rs:173`](../../synapse-web/src/routes/handlers/versions.rs#L173) `get_well_known_support` 由硬编码 `https://matrix.org` 改为读配置（未配置返回 `{}`），补 2 单测                                                                                                                                              | [`config/server.rs:155`](../../synapse-common/src/config/server.rs#L155)、[`versions.rs:173`](../../synapse-web/src/routes/handlers/versions.rs#L173)            |
| ~~`COMPAT-03`~~ | ✅ **已闭环（2026-10-06）**：`M_UNRECOGNIZED`/`Unimplemented` 归一为 **404**（规范无 501 errcode，405 由 method-not-allowed 中间件直接发出）；[`error.rs:269`](../../synapse-common/src/error.rs#L269) 构造函数同步                                                                                                                                                                                                                                                 | [`error/code.rs:166`](../../synapse-common/src/error/code.rs#L166)、[`error.rs:269`](../../synapse-common/src/error.rs#L269)                                     |
| ~~`COMPAT-04`~~ | ✅ **已闭环（2026-10-06）**：删除非规范 errcode `M_UNSUPPORTED` 全链（变体/构造函数/serde/测试）；唯一调用点 presence 改 [`ApiError::forbidden`](../../synapse-web/src/routes/handlers/presence.rs#L29)（**403 `M_FORBIDDEN`**）                                                                                                                                                                                                                                      | [`presence.rs:29`](../../synapse-web/src/routes/handlers/presence.rs#L29)、[`error/code.rs`](../../synapse-common/src/error/code.rs)                             |
| ~~`COMPAT-05`~~ | ✅ **已闭环（2026-10-06）**：两条建议路径均已落地——① 迁移内 [`:70-71`](../../migrations/00000000_unified_schema_v12.sql#L70-L71) 已是 `CREATE EXTENSION IF NOT EXISTS pgcrypto;` / `pg_trgm;`（幂等）；② [`migrations/README.md:52-71`](../../migrations/README.md#L52) 文档化前置要求（必需扩展、`postgresql-contrib` 安装包、建库角色 `CREATE` 权限、受限角色由 DBA 预装）；复核 `gen_random_uuid()` 调用点（`:709/724/2195`）均由该前置保障                                                                 | [`00000000_unified_schema_v12.sql:70-71`](../../migrations/00000000_unified_schema_v12.sql#L70-L71)、[`migrations/README.md:52`](../../migrations/README.md#L52) |
| ~~`COMPAT-06`~~ | ✅ **已闭环（2026-10-06）**：`--static` 新增「源码查询指纹 vs 缓存」新鲜度断言（复用 census 词法扫描器，1360 处字面量全量核对，非常量实参跳过）；CI 仅跑 `--static` 的缺口已补齐                                                                                                                                                                                                                                                                                                                | [`check_sqlx_cache_fresh.sh`](../../scripts/ci/check_sqlx_cache_fresh.sh)、[`sqlx_query_census.py`](../../scripts/ci/sqlx_query_census.py)                       |

### 3.5 代码质量与技术债务（5，已全部闭环）

| 编号          | 问题                                                                                                                                                                                                                                                                                                                                                                                                                                                  | 证据                                                                                                                                                                                  |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ~~`CQ-01`~~ | ✅ **已闭环（2026-10-06）**：建立「allow 白名单 + 理由必填」ratchet——新建 [`check_clippy_allow_ratchet.py`](../../scripts/ci/check_clippy_allow_ratchet.py) + 白名单 [`clippy_allow_allowlist`](../../scripts/ci/clippy_allow_allowlist)（13 条均带理由）+ 逐文件 grandfather 基线 [`clippy_allow_baseline`](../../scripts/ci/clippy_allow_baseline)（**176 unjustified / 108 文件**，只降不升）；group lint 硬禁 + 扫描 0 处 fail-closed；接入 [`ci.yml`](../../.github/workflows/ci.yml) `repo-sanity` | [`check_clippy_allow_ratchet.py`](../../scripts/ci/check_clippy_allow_ratchet.py)、[`clippy_allow_allowlist`](../../scripts/ci/clippy_allow_allowlist)                               |
| ~~`CQ-02`~~ | ✅ **已闭环（2026-10-06）**：11 处生产 `expect_used` 补 `reason = "…"` 证明性注释、删 1 处冗余 allow；allow-ratchet 基线 **176 → 164** unjustified / **108 → 99** 文件，门禁 `OK`（见报告 §8.9）                                                                                                                                                                                                                                                                                      | [`crypto.rs:276`](../../synapse-common/src/crypto.rs#L276)、[`loader.rs:204`](../../synapse-common/src/config/loader.rs#L204)                                                        |
| ~~`CQ-03`~~ | ✅ **已闭环（2026-10-06）**：`get_room_members_paginated` 两处 `map_err` 改为保留 cause（`Failed to check room existence: {e}` / `Failed to check membership: {e}`）（见报告 §8.9）                                                                                                                                                                                                                                                                                     | [`membership/service.rs:781`](../../synapse-services/src/room/membership/service.rs#L781)                                                                                           |
| ~~`CQ-04`~~ | ✅ **已闭环（2026-10-06）**：超大文件（`synapse-storage/src/room/mod.rs` 2474 行）改走 **CI 阈值门禁**（[`check_file_size_ratchet.py`](../../scripts/ci/check_file_size_ratchet.py) + 基线，500 行 + 257 条 grandfather），未拆分文件                                                                                                                                                                                                                                                | [`check_file_size_ratchet.py`](../../scripts/ci/check_file_size_ratchet.py)、[`.clippy.toml`](../../.clippy.toml)                                                                    |
| ~~`CQ-05`~~ | ✅ **已闭环（2026-10-06）**（最小方案：保留 `String` 存储、显式化错误路径）：[`membership/service.rs:559`](../../synapse-services/src/room/membership/service.rs#L559) 不可解析值 → `ApiError::internal`（fail-closed）；[`state_map_auth.rs:131`](../../synapse-federation/src/event_auth/state_map_auth.rs#L131) 不可解析值 → `tracing::warn!` 后返回 `None`（降级可见）（见报告 §8.9）                                                                                                                | [`membership/service.rs:559`](../../synapse-services/src/room/membership/service.rs#L559)、[`state_map_auth.rs:131`](../../synapse-federation/src/event_auth/state_map_auth.rs#L131) |

### 3.6 冗余代码与过时内容（4，其中 `RED-01` 已闭环见报告 §8.8、`RED-02`/`RED-03`/`RED-04` 已闭环见报告 §8.7）

| 编号           | 问题                                                                                                                                                                                                                                                                                                                | 证据                                                                                                                                            |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| ~~`RED-01`~~ | ✅ **已闭环（2026-10-06）**：[`event/create.rs`](../../synapse-storage/src/event/create.rs) 中 `create_event_with_pdu` 与 `create_outlier_event` 的逐字节重复 INSERT 抽为私有 [`insert_event_row_with_graph`](../../synapse-storage/src/event/create.rs#L64)；`INSERT INTO events` 字面量 **5 → 4 处**                                    | [`event/create.rs`](../../synapse-storage/src/event/create.rs)、[`insert_event_row_with_graph`](../../synapse-storage/src/event/create.rs#L64) |
| ~~`RED-02`~~ | ✅ **已闭环（2026-10-06）**：`search_index` 三处遗留引用全部清理——① [`lib.rs:261`](../../synapse-storage/src/lib.rs#L261) 注释移除 `search_index`（`grep` 零命中）；② `coverage_baseline.json` 幽灵条目（并入 `RED-03`）清零；③ [`docs/trigram-audit.md`](../../docs/trigram-audit.md) 中关于已删模块 `search_index.rs` 的「已采纳 / [DONE]」失效结论就地订正（`~~…~~ → 已作废`） | [`lib.rs:261`](../../synapse-storage/src/lib.rs#L261)、[`trigram-audit.md`](../../docs/trigram-audit.md)                                       |
| ~~`RED-03`~~ | ✅ **已闭环（2026-10-06）**：[`coverage_baseline.json`](../../scripts/ci/coverage_baseline.json) 移除 8 条指向不存在文件的幽灵条目；复核 **609 条 / 0 幽灵**                                                                                                                                                                                  | [`coverage_baseline.json`](../../scripts/ci/coverage_baseline.json)                                                                           |
| ~~`RED-04`~~ | ✅ **已闭环（2026-10-06）**：[`.gitignore`](../../.gitignore#L136) 新增 `.worktrees/` 忽略规则，并清理 `.worktrees/` 残留目录（`DEAD-03`）                                                                                                                                                                                               | [`\.gitignore`](../../.gitignore#L136)                                                                                                        |

### 3.7 文档与知识库（3 + 依赖卫生 1，其中 `DOC-03`/`DOC-04`/`DOC-05` 与 `DEAD-01` 已闭环）

| 编号            | 问题                                                                                                                                                                                                                                                                                                                                                              | 证据                                                                                                                                                      |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ~~`DOC-03`~~  | ✅ **已闭环（2026-10-06）**：[`API_COVERAGE_REPORT.md`](../../docs/synapse-rust/API_COVERAGE_REPORT.md) 三口径重算至与 [`ROUTE_CONTRACT.md`](../../docs/synapse-rust/ROUTE_CONTRACT.md#L11)（2026-10-06 生成）同一 HEAD：注册条目 **1159**、唯一路径 **921**、逻辑端点 **813**（旧值 1153/915/807 作废），消除「人工文档落后机器权威 6 条」矛盾                                                                            | [`API_COVERAGE_REPORT.md`](../../docs/synapse-rust/API_COVERAGE_REPORT.md)                                                                              |
| ~~`DOC-04`~~  | ✅ **已闭环（2026-10-06）**：[`synapse-rust-vs-synapse-comparison.md`](../../docs/synapse-rust-vs-synapse-comparison.md) 就地订正三类 ground truth（默认房间版本 **12**、Content Scanner **已接线**、`validate_id_token_claims` **已删**）；守卫 [`doc_credibility_guard_tests.rs`](../../tests/unit/doc_credibility_guard_tests.rs) 新增 `stale_claim_violations`（剥离 `~~…~~` 后判陈旧声明），**8/8 通过** | [`comparison.md`](../../docs/synapse-rust-vs-synapse-comparison.md)、[`doc_credibility_guard_tests.rs`](../../tests/unit/doc_credibility_guard_tests.rs) |
| ~~`DOC-05`~~  | ✅ **已闭环（2026-10-06）**：[`CHECKLIST.md`](../../CHECKLIST.md) 三处订正：:45「10 个 delta 迁移」→ 实为 **0**（已折入统一基线）；:55/:79 的依赖结论注明「曾失真（实测 10 个未使用）→ 已于本轮删除并接入 `cargo machete` pre-push 阻断门禁，复验 **0 unused**」                                                                                                                                                                 | [`CHECKLIST.md`](../../CHECKLIST.md)                                                                                                                    |
| ~~`DEAD-01`~~ | ✅ **已闭环（2026-10-06）**：原报 10 个 machete 未使用依赖（根 8 + `synapse-web` 2）已全部删除并复验为 0；`cargo machete` 已接入阻断级 pre-push 门禁                                                                                                                                                                                                                                                | [根 Cargo.toml](../../Cargo.toml)、[synapse-web/Cargo.toml](../../synapse-web/Cargo.toml)、[.githooks/pre-push](../../.githooks/pre-push)                  |

---

## 4. P3 级问题（27 条，加固/决策/细节）

> 完整清单见全面报告 §2.C。**27 条已于 2026-10-06 全部处置**（21 条闭环 + 6 条 by-design 正式登记，详见报告 §8.11）。摘要：
>
> - 安全加固：~~`SEC-02`~~/~~`SEC-03`~~/~~`SEC-06`~~（✅ 见 §8.10）、~~`SEC-04`~~（✅ 撤销写入路径主动失效已覆盖 30s 缓存窗口，见 §8.11）、~~`SEC-05`~~（✅ per-IP 双桶节流 `LOGIN_MAX_IP_ATTEMPTS=30`，见 §8.11）。
> - 功能桩/借用：~~`FN-03`~~（✅ `get_cross_signing_keys` 签名由恒空修正为从 `signatures` 提取 + 4 单测，见 §8.11）、~~`FN-04`~~（✅ URL 预览桩补文档、行为不变，见 §8.11）；`FN-05`~`FN-08`（**by-design 登记**：voice 501 桩、备份 410、legacy keys `M_UNRECOGNIZED`、MSC 编号借用，见 §8.11）。
> - 性能细节：~~`PERF-07`~~（✅ 谓词改 sargable `sender = $1`，附不变式说明，见 §8.11）、~~`PERF-08`~~（✅ 构造函数改 async + `tokio::fs`，见 §8.10）、~~`PERF-09`~~（✅ 模拟测试补诚实头注释并指向真实门禁，见 §8.11）。
> - 兼容/CI：~~`COMPAT-07`~~（✅ `.well-known` 逐端点复核即闭环，见 §8.11）、~~`COMPAT-08`~~（✅ feature matrix 超时 300→900 且可 env 覆盖，见 §8.11）、`COMPAT-09`（**by-design**：稳定/unstable 双前缀并存服务旧客户端，见 §8.11）。
> - 质量/冗余/文档：~~`CQ-06`~~（✅ 补 allow 理由，见 §8.10）、~~`CQ-07`~~（✅ 无理由 allow 三分类处理，见 §8.11）、~~`CQ-08`~~（✅ 计数口径订正，见 §8.10）、~~`DEAD-02/03`~~（✅ 已闭环 2026-10-06，见报告 §8.7）、~~`DOC-06`~~/~~`DOC-07`~~/~~`DOC-08`~~/~~`DOC-09`~~/~~`DOC-10`~~（✅ 已闭环 2026-10-06，见报告 §8.10；其中 `DOC-09` 仅归档 5 份、v12 系列被生产代码引用故保留）、~~`DOC-11`~~（✅ 已闭环 2026-10-06，见报告 §8.8）。
> - 门禁存量（非 §2.C 条目）：`fmt` 棘轮基线 [`scripts/.fmt-baseline`](../../scripts/.fmt-baseline) 曾因更早轮次遗留回退至 **51 处**（本轮触碰文件仅占 2 处，已就地修正）；本轮门禁终验经用户决策执行 `cargo fmt --all` 清空全部存量并固化基线为 **0**，`./scripts/check_fmt_ratchet.sh` 复跑 `OK`（详见报告 §8.10 门禁终验补充）。

---

## 5. 本轮复核纠正（证伪 / 已修复，9 条）

> 以下条目经**直接读码复核**确认已不成立，须从跟踪清单移除或降级，避免重复排查。

### 5.1 上一版 RBAC P0/P1 已修复（3）

| 原编号                            | 结论        | 复核证据                                                                                    |
| ------------------------------ | --------- | --------------------------------------------------------------------------------------- |
| P0-1 `login_as_user` 越权冒充超管    | ✅ **已修复** | `synapse-web/src/routes/admin/user.rs:597-599` 新增超管校验                                   |
| P1-1 `/devices/delete` 字符串匹配错位 | ✅ **已修复** | `synapse-web/src/utils/admin_auth.rs:217` 改为 `ends_with("/devices/delete")` 匹配；单测改用真实路径 |
| P1-2 客户端审计 result 恒 success    | ✅ **已修复** | 全仓已无 `audit_user_action` 定义/调用                                                          |

### 5.2 权威清单「仍存」实为已解决（5）

| 原条目                               | 结论         | 复核证据                                                                                   |
| --------------------------------- | ---------- | -------------------------------------------------------------------------------------- |
| 事务去重标记位置（原 1.2）                   | ✅ **已修复**  | `synapse-services/src/room/messaging/messages.rs:52-55`：dedup marker 与事件同事务写入；原引用路径不存在 |
| Profile `/{keyName}` 未注册（原 3.2）   | ✅ **已修复**  | `synapse-web/src/routes/assembly.rs:239` 已注册                                           |
| Admin 媒体端点族不完整（原 3.3）             | ✅ **已对齐**  | `synapse-web/src/routes/admin/media.rs:40-69`                                          |
| ledger `query_params` 无消费方（原 3.4） | ✅ **已有守卫** | `synapse-web/src/routes/route_ledger.rs:53-58`                                         |
| v12/v13 房间不可创建（原 4.1）             | ✅ **已更新**  | `synapse-common/src/room_versions.rs:94` `DEFAULT_ROOM_VERSION="12"`；v13 已移除           |

### 5.3 其余证伪（1）

| 原结论                | 结论                | 复核证据                                                                                                                                          |
| ------------------ | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| 交叉签名上传无验签 = 可利用 P1 | ⚠️ **降级为死代码（P3）** | `/keys/signatures/upload` → `device_keys_service.upload_signatures`（`device-keys/service.rs:651`，**有 ownership 校验**）；`cross_signing` 同名方法无调用点 |

---

## 6. 已解决的问题（供参考）

| 问题                                                     | 修复依据              | 描述                                                                                                                         |
| ------------------------------------------------------ | ----------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 联邦 `/send_join` PDU 语义                                 | 2026-09-28        | GraphMetadataWriter 接入 reference hash                                                                                      |
| Event ID 语义不匹配                                         | 2026-09-28        | Placeholder + legacy ID 再生成                                                                                                |
| soft_failed 读路径过滤                                      | 2026-09-28        | 全面覆盖客户端读取面                                                                                                                 |
| 客户端撤回级联                                                | 2026-09-28        | 完整实现 MSC3912                                                                                                               |
| Animated 缩略图支持                                         | 2026-09-28        | Phase 2 全功能实现                                                                                                              |
| Content Scanner 误判                                     | 2026-10-04        | 验证为已集成，原审计误判                                                                                                               |
| Admin 媒体端点族补齐                                          | 2026-10-04        | `purge_media_cache` 路由已添加                                                                                                  |
| U-19-R4 `redacted_by=None` 审计丢失                        | `5d1bfdc3f`       | 传递 `redaction_event_id`                                                                                                    |
| U-13-R9 v≤11 写路径不持久化图字段                                | `11bf5455d`       | 走 `create_event_with_pdu`                                                                                                  |
| OIDC 回调提权                                              | `fe35fb0a`        | 按 issuer+subject 绑定                                                                                                        |
| SSSS 对齐 aes-hmac-sha2                                  | `a2375743`        | 含 NIST 已知向量                                                                                                                |
| Dehydrated `/events` 仅 POST                            | Phase 2           | 改为 GET + query 参数                                                                                                          |
| rc_reports 专项限流                                        | Phase 2           | 桶函数 + 可配置规则                                                                                                                |
| AS 登录 `m.login.application_service`                    | Phase 2           | 完整实现                                                                                                                       |
| RBAC P0-1 / P1-1 / P1-2                                | 2026-10-05 修复     | 见 §5.1                                                                                                                     |
| FED-01 `/send_join` 缺 `event` 字段                       | 2026-10-06 修复     | v1/v2 响应回显签名 join PDU；补集成测试（见 §2.1）                                                                                        |
| DOC-01 `project_rules.md` 失实                           | 2026-10-06 修复     | §1.3 重写、§7.0 路径修正、版本升至 v2.5.0（见 §2.2）                                                                                      |
| DOC-02 权威清单过时结论/破路径                                    | 2026-10-06 刷新     | 移除 5 条已解决项、修正引用路径（见 §2.3）                                                                                                  |
| DEAD-01 未使用依赖 + machete 未入门禁                           | 2026-10-06 修复     | 删除 10 个未使用依赖（复验 0）；`cargo machete` 入 pre-push 阻断阶段（见 §3.7）                                                                 |
| COMPAT-06 `.sqlx` 缓存新鲜度未校验                             | 2026-10-06 修复     | `--static` 新增「源码查询指纹 vs 缓存」断言（1360 处字面量全量核对）；守卫测试 17/17 通过（见 §3.4）                                                         |
| PERF-04 连接预算门禁只校验文档、不读运行配置                             | 2026-10-06 修复     | 门禁解析三源池上限 + `postgres.conf`，断言「文档 == 代码 == 运行」并在真实值上校验不变式；`postgres.conf` 200→250（见 §3.3）                                  |
| PERF-05 内存预算基线硬编码 + `high` 不失败                         | 2026-10-06 修复     | 基线参数抽到 `scripts/ci/memory_budget_baseline`（fail-closed）；`high` 风险改为 `exit 1`（见 §3.3）                                       |
| PERF-02 `get_daily_message_count` 缺复合索引                | 2026-10-06 修复     | 统一基线新增 `idx_events_type_origin_ts(event_type, origin_server_ts DESC)`；索引计数 350/140 → 351/141（见 §3.3）                       |
| PERF-03 `get_user_tokens` / `get_updates_by_status` 无界 | 2026-10-06 修复     | 两查询各加 `LIMIT`（上限 10_000）+ 命中 `warn`；同批消除 `#[cfg(test)]` 静态宏 `.sqlx` 悬空隐患（见 §3.3）                                           |
| COMPAT-01 MSC4155 广告与语义不符                              | 2026-10-06 复核（误报） | 官方语义已实现（account data 写入校验 + 邀请门禁消费）→ 保留广告，订正 `MSC_SEMANTICS.md`（见 §3.4）                                                    |
| COMPAT-02 `.well-known/matrix/support` 硬编码             | 2026-10-06 修复     | 新增 `support_url` 配置项；未配置返回 `{}`，不再伪造 `https://matrix.org`；补 2 单测（见 §3.4）                                                   |
| COMPAT-03 `M_UNRECOGNIZED` 状态不一致                       | 2026-10-06 修复     | `Unrecognized`/`Unimplemented` 归一为 **404**（规范无 501 errcode）；构造函数同步（见 §3.4）                                                 |
| COMPAT-04 非规范 errcode `M_UNSUPPORTED`                  | 2026-10-06 修复     | 全链删除；唯一调用点 presence 改 `ApiError::forbidden`（**403 `M_FORBIDDEN`**）（见 §3.4）                                                 |
| DOC-03 路由计数文档矛盾                                        | 2026-10-06 修复     | `API_COVERAGE_REPORT.md` 三口径重算至与 `ROUTE_CONTRACT.md` 同 HEAD：1159/921/813（见 §3.7）                                           |
| DOC-04 comparison 多处 stale                             | 2026-10-06 修复     | 订正三类 ground truth；守卫新增 `stale_claim_violations`（剥离删除线后判陈旧），8/8 通过（见 §3.7）                                                  |
| DOC-05 `CHECKLIST.md` 结论过时                             | 2026-10-06 修复     | :45「10 个 delta 迁移」→ 0；:55/:79 依赖结论同步 `DEAD-01` 闭环（见 §3.7）                                                                  |
| COMPAT-05 迁移扩展前置依赖                                     | 2026-10-06 复核闭环   | 迁移内已 `CREATE EXTENSION IF NOT EXISTS`（幂等）；`migrations/README.md` 文档化前置要求（见 §3.4）                                           |
| SEC-04 撤销检查 30s 缓存窗口                                   | 2026-10-06 复核闭环   | 所有撤销写入路径主动失效对应标记，TTL 仅异常兜底（见报告 §8.11）                                                                                      |
| SEC-05 登录锁定无 per-IP 节流                                 | 2026-10-06 复核闭环   | `LOGIN_MAX_IP_ATTEMPTS=30` + 双桶 `check_login_lockout`；Redis 不可用 fail-closed（见报告 §8.11）                                     |
| PERF-07 `COALESCE(user_id, sender)` 非 sargable         | 2026-10-06 复核闭环   | 谓词改 `sender = $1`，附写路径不变式说明，可用 `idx_events_sender_time`（见报告 §8.11）                                                         |
| PERF-09 性能测试模拟无断言                                      | 2026-10-06 复核闭环   | 补诚实头注释（明示 SIMULATED/不断言）并指向真实门禁（见报告 §8.11）                                                                                 |
| COMPAT-08 feature matrix `all-extensions` 超时 300s      | 2026-10-06 复核闭环   | 默认超时 300→**900**，支持 `FEATURE_MATRIX_TIMEOUT_SECS` 覆盖（见报告 §8.11）                                                            |
| COMPAT-07 `.well-known` 其余字段/边界                        | 2026-10-06 复核闭环   | 逐端点复核无残留缺陷（support_page 标准、capabilities 非标准块已删）（见报告 §8.11）                                                                 |
| CQ-07 无理由 `dead_code`/`unused_*` allow                 | 2026-10-06 修复     | 8 处三分类：真死代码 1 删、feature-gated 2 补理由、test-only 5 补理由；均 rustc lint，ratchet 基线不变 163/98（见报告 §8.11）                            |
| FN-03 `get_cross_signing_keys` 签名恒空                    | 2026-10-06 修复     | 新增 `master_key_id`/`extract_signature_for` 从 `signatures` 提取；`verify_cross_key_signature` 复用去重；补 4 单测；该函数无生产调用点（见报告 §8.11） |
| FN-04 URL 预览硬编码桩                                       | 2026-10-06 修复     | `preview_url` 补 Stub 文档注释（不改行为），受默认关闭的 `msc4452_enabled` 门控（见报告 §8.11）                                                     |
| FN-05~FN-08 / COMPAT-09 by-design                      | 2026-10-06 正式登记   | 501/410 桩、legacy keys `M_UNRECOGNIZED`、MSC 编号借用、稳定/unstable 双前缀（见报告 §8.11）                                                 |

---

## 7. 需要跟踪的文档

1. **`docs/audit/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`** — 当前权威全面报告
2. **`docs/audit/INDEX.md`** — 审计文档索引
3. **`docs/synapse-rust/ROUTE_CONTRACT.md`** — 路由契约（机器权威，2026-10-09 生成，**1030** 条）
4. **`docs/synapse-rust/MSC_SEMANTICS.md`** — MSC 语义对齐表
5. **`docs/audit/PERMISSION_RBAC_AUDIT_2026-10-05.md`** — 权限专项（P0/P1 已修复）

---

**生成时间**: 2026-10-06  
**下次更新**: 2026-10-13

## 8. 2026-10-09 增量复核（前缀命名空间治理收尾）

> 判据一律取**提交对象**与本仓门禁实跑；基线 `main @ f6cdd5c60`。定级沿用本文件口径。

### 8.1 本轮闭环（既有红）

| 项 | 结论 | 证据 |
| --- | --- | --- |
| `cas.rs` 未使用 `HeaderValue` 导入 | ✅ 已修复（`8d0839fc8`） | C6 第四批删 legacy CAS 中间件后的死导入。`cargo check` 只给 warning，但 CI 权威门禁 `clippy --workspace --all-targets --features test-utils -- -D warnings` 把它升级为 error ⇒ 提交前一直是既有红 |
| `rooms/{room_id}/sync` 的 `query_params` 被静默丢弃 | ✅ 已修复（`ebe4a3db6`） | `scripts/contract/ledger_annotations.txt` 是 `query_params` 的唯一真源；M3 把该路由迁 vendor 时注解键没跟 ⇒ `full_state,since,timeout` 丢失，`EXTRACT_STRICT` 的 parser 自检判红 |
| 恒 400 的拒绝型路由 `POST /_matrix/vendor/v1/rooms/{room_id}/widgets/{widget_id}/send` | ✅ 已删除（`ebe4a3db6`） | 零有效实现（handler 正文 `let _ = body;` 后恒返 400）；契约链 9 步重生成；注册条目 1031 → **1030** |
| `.githooks/pre-commit` 的暂存快照判据在多写者下误拦**所有**提交 | ✅ 已修复（`a95ba5ef0`） | 判据由「未暂存集合非空」收紧为「已暂存 ∩ 未暂存」；多写者窗口内可正常提交 |

### 8.2 本轮新登记（P3，文档 / 口径卫生）

| 编号 | 级别 | 问题 | 处置 |
| --- | --- | --- | --- |
| `DOC-12` | P3 | **SDK 仓** `docs/api-contract/*.md`（手维护模块页）系统性陈旧：「挂载版本」表停在 client 前缀；SDK Manager 方法名与实际不符（抽样 `widget.md` **5/18** 行） | 登记于 [`前缀命名空间治理方案-2026-10-08.md`](../前缀命名空间治理方案-2026-10-08.md) §13.22.6(c)；需独立刷新批次 |
| `DOC-13` | P3 | `scripts/api_test/ledger.json` 陈旧（停在 2026-08-12 的路由集），而 `gen_route_table.py` 的**默认** `--ledger` 正指向它 ⇒ 照默认跑会产出 3000+ 行伪改动、且**本地 `--check` 会通过**（同源自洽），只在 CI 判红 | 已踩 2 次（`a421e7641`、2026-10-09）。CI 口径见 `ci.yml` 的 openapi-artifact job（default-feature 新鲜导出 + `artifact_common.FIXED_TIMESTAMP`）；建议把 `--ledger` 改为**必填** |

### 8.3 口径订正（同一 HEAD）

- `API_COVERAGE_REPORT.md` §1.1 三口径的**分桶列**此前滞后一整批（M3 迁出 46 条后只更新了总数，旧值 446+290+297 = 1033 ≠ 总数 1031）⇒ 按该文 §8 配方复算为 注册条目 **1030**（395/290/345）、唯一路径 **817**（288/228/301）、逻辑端点 **749**（222/226/301）；`synapse-rust-vs-synapse-comparison.md` 的三口径引用同步。
- `docs/openapi/route-table.json` 按 CI 口径重生成：952 → **951** 条（删 widget send + 补回 `sync` 的 `query_params`）。

### 8.4 跨仓联动

- **SDK `release/contract-entrypoint` `2103d74e0`**：跟随后端删路由 —— 契约镜像 1015 → 1030、清 1 条陈旧键（`contract:codegen` 单调并集不会自动删）、`pin-docs` 重钉 46 页。
- **Tjg `4198b566`**：重打包 SDK tarball + `pnpm-lock.yaml` 校验和 + 重钉 `sdk-pin.json`（`sdk_commit 589bb9ca9 → 2103d74e0`、`synapse_rust_commit → f6cdd5c60`）。
- ⚠️ **develop 侧的真实漏项**：`src/widgets/index.ts` 的 `sendWidgetMessage()` 仍在调用本轮删除的路由
  （后端删路由时判「零消费者」有误 —— 只检索了 `src/widget/` 单数目录，且用了会被本机 CLI 吞掉的裸 `grep` 扩展正则）。
  已在 develop 按本仓既有形态登记 `by-design` 豁免 + 补 ⚠️ JSDoc；**`sendWidgetMessage()` 的存废 / 改接标准 send API 属产品决策**。
  详见 SDK 仓 `artifacts/remaining-issues-and-optimization-plan-2026-10-08.md` §7.5。
- develop 侧同轮另清掉 3 条既有红（`docs-counts` 计数陈旧、`public-jsdoc-examples` 缺 `@example`、路径契约不匹配 + 覆盖棘轮未收紧），`pnpm lint` 与 `pnpm quality:contracts` 已双绿；见同文件 §7.5。
