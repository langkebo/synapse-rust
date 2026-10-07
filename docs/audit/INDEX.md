# Synapse-Rust 审计文档索引

## 📋 活跃文档（当前有效）

### 核心问题追踪

| 文档 | 描述 | 更新日期 |
|------|------|----------|
| **[UNRESOLVED_ISSUES_SUMMARY.md](UNRESOLVED_ISSUES_SUMMARY.md)** | 当前仍存问题总结（权威来源） | 2026-10-06 |
| **[INDEX.md](INDEX.md)** | 本索引文档 | 2026-10-06 |
| **[COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md)** | **全面系统遗留问题排查报告（当前权威）** | 2026-10-06 |
| **[FINAL_AUDIT_SUMMARY_20261006.md](FINAL_AUDIT_SUMMARY_20261006.md)** | **遗留问题审计最终总结报告（结项收敛；明细见全面报告）** | 2026-10-06 |
| **[COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md](archive/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md)** | 全面系统遗留问题排查报告（上一版，已被取代，已归档） | 2026-10-04 |
| **[PERMISSION_RBAC_AUDIT_2026-10-05.md](PERMISSION_RBAC_AUDIT_2026-10-05.md)** | 权限/RBAC 专项审计（P0/P1 已修复） | 2026-10-05 |
| **[FULL_SYSTEM_TEST_REPORT_2026-10-05.md](FULL_SYSTEM_TEST_REPORT_2026-10-05.md)** | 全系统测试报告 | 2026-10-05 |

### Room v12 合规

| 文档 | 描述 | 状态 |
|------|------|------|
| **[ROOM_V12_UPSTREAM_ALIGNMENT_OPTIMIZATION_2026-09-28.md](ROOM_V12_UPSTREAM_ALIGNMENT_OPTIMIZATION_2026-09-28.md)** | Room v12 上游对齐优化方案 | ✅ Active |
| **[ROOM_V12_PLAN_STATUS_2026-09-27.md](ROOM_V12_PLAN_STATUS_2026-09-27.md)** | Room v12 实施计划状态 | ✅ Active |
| **[ROOM_V12_B2A_DELIVERY_2026-09-27.md](ROOM_V12_B2A_DELIVERY_2026-09-27.md)** | B2A 交付验证 | ✅ Active |
| **[ROOM_V12_COMPLETION_PLAN_2026-09-27.md](ROOM_V12_COMPLETION_PLAN_2026-09-27.md)** | 完成计划 | ✅ Active |
| **[O1_PHASE1_V12_IMPLEMENTATION_DETAILS.md](O1_PHASE1_V12_IMPLEMENTATION_DETAILS.md)** | v12 实施细节 | ✅ Active |

### 联邦与互操作

| 文档 | 描述 | 状态 |
|------|------|------|
| **[A5_LIVE_FEDERATION_INTEROP_TESTING.md](A5_LIVE_FEDERATION_INTEROP_TESTING.md)** | A5 联邦互操作测试 | ✅ Verified (2026-09-28) |
| **[v12-pdu-graph-fields-integration.md](v12-pdu-graph-fields-integration.md)** | PDU 图字段集成 | ✅ Active |
| **[MUTATION_6_FAILURE_ANALYSIS_2026-09-29.md](archive/MUTATION_6_FAILURE_ANALYSIS_2026-09-29.md)** | Mutation 测试失败分析 | 📦 已归档 |
| **[MUTATION_6_REDESIGN_SYNTHETIC_2026-09-29.md](archive/MUTATION_6_REDESIGN_SYNTHETIC_2026-09-29.md)** | Mutation 重新设计 | 📦 已归档 |

### MSC 与实验性功能

| 文档 | 描述 | 状态 |
|------|------|------|
| **[sdk-encapsulation-audit.md](sdk-encapsulation-audit.md)** | SDK 封装审计 | ✅ Active |
| **[CAS_ROUTER_PREFIX_MISSING_2026-09-29.md](CAS_ROUTER_PREFIX_MISSING_2026-09-29.md)** | CAS 路由器前缀缺失 | ✅ Active |

### 其他规划文档

| 文档 | 描述 | 状态 |
|------|------|------|
| **[REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md](REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md)** | 剩余问题验证与优化计划 | ✅ Active |
| **[ISSUE_REVIEW_2026-09-28.md](ISSUE_REVIEW_2026-09-28.md)** | 问题审查总结 | ✅ Active |
| **[SQLX_STATICIZATION_PLAN_2026-09-23.md](SQLX_STATICIZATION_PLAN_2026-09-23.md)** | SQLX 静态化计划 | ✅ Active |
| **[DB_REVIEW_2026-09-17.md](DB_REVIEW_2026-09-17.md)** | 数据库审查 | ✅ Active |
| **[D-12_EVENT_REPORT_HISTORY_STATS_FIX_PLAN.md](D-12_EVENT_REPORT_HISTORY_STATS_FIX_PLAN.md)** | 事件报告历史统计修复计划 | ✅ Active |

### 技术策略

| 文档 | 描述 | 状态 |
|------|------|------|
| **[A3_v1_deprecation_strategy.md](A3_v1_deprecation_strategy.md)** | v1 弃用策略 | ✅ Active |
| **[P5_engineering_2026-09-11.md](P5_engineering_2026-09-11.md)** | 工程问题分类 | ✅ Active |

---

## 📦 归档文档（已过时/已解决）

以下文档已移动至 `archive/` 子目录，供历史参考：

**归档数量**: 46 个文件

主要归档类别：
- P1-P5 性能基准与基础设施问题
- Gate 完整性验证系列
- 部署与验证流程
- 早期架构补救路线图
- 缓存读写审计
- SIGTERM 根因分析
- 测试隔离模板与设计
- 被取代的遗留问题报告（`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md`、`LEGACY_ISSUES_REPORT_20261004.md`、`LEGACY_ISSUES_REPORT_ROUND3_20261004.md`）与已结案的 Mutation 报告（`MUTATION_6_*`，2026-10-06 归档）

👉 **访问方式**: `docs/audit/archive/` 目录

> **不在本索引范围的计划文档**：[`docs/superpowers/plans/`](../superpowers/plans/) 下的
> 实施计划（约 36 个 `.md`）由 `gstack` 自动化工作流生成，已被 [`.gitignore:110`](../../.gitignore#L110)
> 忽略、**不入库**，故不登记为本目录的审计文档。本索引仅覆盖 `docs/audit/` 下入仓的
> 审计/报告/计划，以及 `archive/` 中的历史归档。

---

## 📊 问题状态摘要

### 已解决的问题（2026-10-06 更新）

| 问题 | 优先级 | 解决日期 | 解决方案 |
|------|--------|----------|----------|
| 联邦 `/send_join` 响应缺 `event` 字段（FED-01） | P1 | 2026-10-06 | v1/v2 回显签名 join PDU + 集成测试 |
| `project_rules.md` 目录/迁移计数失实（DOC-01） | P1 | 2026-10-06 | §1.3 重写、§7.0 路径修正，升至 v2.5.0 |
| `UNRESOLVED_ISSUES_SUMMARY.md` 过时结论（DOC-02） | P1 | 2026-10-06 | 移除 5 条已解决项、修正破路径 |
| 10 个 machete 未使用依赖 + 门禁缺失（DEAD-01） | P2 | 2026-10-06 | 删除依赖（复验 0），`cargo machete` 入 pre-push 阻断级 |
| `.sqlx` 缓存内容新鲜度未校验（COMPAT-06） | P2 | 2026-10-06 | `--static` 新增「源码查询指纹 vs 缓存」断言（1360 处字面量全量核对） |
| 连接预算门禁只校验文档、不读运行配置（PERF-04） | P2 | 2026-10-06 | 门禁解析三源池上限 + `postgres.conf`，断言「文档==代码==运行」；`postgres.conf` 200→250 |
| 内存预算门禁基线硬编码、`high` 不失败（PERF-05） | P2 | 2026-10-06 | 基线参数抽到 `scripts/ci/memory_budget_baseline`（fail-closed）；`high` 改为阻断 |
| `get_daily_message_count` 缺复合索引（PERF-02） | P2 | 2026-10-06 | 统一基线新增 `idx_events_type_origin_ts(event_type, origin_server_ts DESC)`；索引计数 350/140 → 351/141 |
| `get_user_tokens` / `get_updates_by_status` 无界查询（PERF-03） | P2 | 2026-10-06 | 两查询各加 `LIMIT`（上限 10_000）+ 命中 `warn`；同批消除 `#[cfg(test)]` 静态宏 `.sqlx` 悬空隐患 |
| MSC4155 广告与语义复核（COMPAT-01） | P2 | 2026-10-06 | 复核为误报：官方语义已实现（account data 写入校验 + 邀请门禁消费），保留广告并订正 `MSC_SEMANTICS.md` |
| `.well-known/matrix/support` 硬编码（COMPAT-02） | P2 | 2026-10-06 | 新增 `support_url` 配置项；未配置返回 `{}`，不再伪造 `https://matrix.org`；补 2 单测 |
| `M_UNRECOGNIZED` 状态不一致（COMPAT-03） | P2 | 2026-10-06 | `Unrecognized`/`Unimplemented` 归一为 **404**（规范无 501 errcode）；构造函数同步 |
| 非规范 errcode `M_UNSUPPORTED`（COMPAT-04） | P2 | 2026-10-06 | 全链删除；唯一调用点 presence 改 `ApiError::forbidden`（**403 `M_FORBIDDEN`**） |
| 路由计数文档矛盾（DOC-03） | P2 | 2026-10-06 | `API_COVERAGE_REPORT.md` 三口径重算至与 `ROUTE_CONTRACT.md` 同 HEAD：1159/921/813 |
| comparison 多处 stale（DOC-04） | P2 | 2026-10-06 | 订正三类 ground truth；守卫新增 `stale_claim_violations`（剥离删除线后判陈旧），8/8 通过 |
| `CHECKLIST.md` 结论过时（DOC-05） | P2 | 2026-10-06 | :45 迁移计数订正为 0；:55/:79 依赖结论同步 `DEAD-01` 闭环 |
| 联邦 `/send_join` PDU 语义 | P0 | 2026-09-28 | GraphMetadataWriter 接入 reference hash |
| Event ID 语义不匹配 | P0 | 2026-09-28 | Placeholder + legacy ID 再生成 |
| Soft_failed 读路径过滤 | P0 | 2026-09-28 | 全面覆盖客户端读取面 |
| 客户端撤回级联 | P2 | 2026-09-28 | 完整实现 MSC3912 |
| Animated 缩略图支持 | P2 | 2026-09-28 | Phase 2 全功能实现 |
| Content Scanner 误判 | P2 | 2026-10-04 | 验证为已集成，原审计误判 |
| Admin 媒体端点族补齐 | P3 | 2026-10-04 | `purge_media_cache` 路由已添加并完成契约更新 |
| 集成测试存量红：房间版本面断言过时（`api_auth_routes_tests`） | 测试 | 2026-10-06 | 判定为**测试过时**（根因 G-1：`7489b247f` 收窄创建面至 v12、`8687d8335` 漏改本文件），断言改为比对 `SUPPORTED_ROOM_VERSIONS`；复跑 1534/0/0 转绿（见全面报告 §8.13） |

### 当前待处理问题（2026-10-06 全面排查后）

| 问题 | 编号 | 优先级 | 状态 | 负责人 |
|------|------|--------|------|--------|
| 联邦 `/send_join` 响应缺 `event` 字段 | FED-01 | **P1** | ✅ 已闭环（2026-10-06） | - |
| `project_rules.md` 目录结构/迁移计数失实 | DOC-01 | **P1** | ✅ 已闭环（2026-10-06） | - |
| `UNRESOLVED_ISSUES_SUMMARY.md` 过时结论与破路径 | DOC-02 | **P1** | ✅ 本轮已刷新 | - |
| 10 个 machete 未使用依赖 + `cargo machete` 未入门禁 | DEAD-01 | P2 | ✅ 已闭环（2026-10-06） | - |
| 联邦 EDU `m.signing_key_update` 无独立验签 | SEC-01 | P2 | ✅ 已闭环（2026-10-06，见 §8.9） | - |
| `/send_join` 族未做 PDU 级哈希/签名校验 | FED-02 | P2 | ✅ 已闭环（2026-10-06，见 §8.9） | - |
| 性能：`get_token` 双查询回退、基线文档漂移 | PERF-01…06 | P2 | ✅ 已闭环（2026-10-06，见 §8.5/§8.6/§8.9） | - |
| 兼容：迁移扩展依赖 `pgcrypto`/`pg_trgm` | COMPAT-05 | P2 | ✅ 已闭环（2026-10-06，见报告 §8.11） | - |
| 质量：clippy allow 逸散、错误吞咽、membership 静默降级 | CQ-01…05 | P2 | ✅ 已闭环（2026-10-06，见 §8.7/§8.8/§8.9） | - |
| 冗余：重复 INSERT、幽灵覆盖率条目、worktree 残留 | RED-01…04 | P2 | ✅ 已闭环（2026-10-06，见 §8.7/§8.8） | - |
| 文档：路由计数/CHECKLIST/comparison stale | DOC-03…05 | P2 | ✅ 已闭环（2026-10-06，见 §8.6） | - |
| 加固/决策/细节项（含 `all-extensions` 超时、MSC 借用） | P3 全体 | P3 | ✅ 已全部处置（2026-10-06，见报告 §8.11）：21 闭环 + 6 by-design 登记 | - |

---

## 🔍 文档使用指南

### 快速查阅

1. **了解当前问题**: 阅读 [UNRESOLVED_ISSUES_SUMMARY.md](UNRESOLVED_ISSUES_SUMMARY.md)
2. **查看整体报告**: 阅读 [COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md)
3. **查看结项总结**: 阅读 [FINAL_AUDIT_SUMMARY_20261006.md](FINAL_AUDIT_SUMMARY_20261006.md)
4. **追踪 Room v12 进度**: 查看 `ROOM_V12_*` 系列文档
5. **联邦互操作验证**: 查看 `A5_LIVE_FEDERATION_INTEROP_TESTING.md`
6. **权限专项**: 查看 [PERMISSION_RBAC_AUDIT_2026-10-05.md](PERMISSION_RBAC_AUDIT_2026-10-05.md)

### 深入分析

- **Mutation 测试方法论**: `MUTATION_6_*` 系列
- **数据库 Schema 变更**: `DB_REVIEW_2026-09-17.md`, `SQLX_STATICIZATION_PLAN_2026-09-23.md`
- **实验性功能语义**: `sdk-encapsulation-audit.md`

### 历史记录

- 归档文档位于 `archive/` 目录
- 如需查找特定日期的审计报告，请查看归档目录

---

## 📅 更新日志

### 2026-10-06

- ✅ **全量测试与性能门禁运行验证（2026-10-06）**：四条 CI 测试车道 + 三个性能门禁全部 PASS——lib **6683/0**、unit **1824/0**（2 skipped）、integration **1534/0/0**（4121.50s）、e2e **20/0**（7 ignored）；计算性能门禁 `17/0/0`、Sliding Sync `33/33`（p95≈0.70–0.90ms）、分页 keyset 较 offset **10.54×**。运行中修复 1 处集成存量红（判定为**测试过时**，见下）。证据登记于 [FINAL_AUDIT_SUMMARY_20261006.md](FINAL_AUDIT_SUMMARY_20261006.md) §3.1 与全面报告 §1.5。
- ✅ 创建 [COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md)（当前权威全面报告）：
  - 基线门禁实测：clippy/deny/fmt/分层/契约/预算/ratchet 等全部通过
  - 定级：**0 个 P0、3 个 P1、26 个 P2、27 个 P3**
  - 新发现两处门禁盲区：`cargo machete` 未接入、`.sqlx` 新鲜度未校验
- ✅ 刷新 [UNRESOLVED_ISSUES_SUMMARY.md](UNRESOLVED_ISSUES_SUMMARY.md)：
  - 移除 5 条已解决/过时条目（事务去重、Profile、Admin 媒体、ledger、v12/v13）
  - 修正破路径；`FED-01`（send_join 缺 `event`）登记为 P1（已于同日闭环，见下）
  - 复核确认 RBAC P0-1 / P1-1 / P1-2 已修复
- ✅ **P1 全部闭环（2026-10-06）**：
  - `FED-01`：`/send_join` v1/v2 响应回显签名 join PDU（`event`），补联邦集成测试，clippy 门禁通过
  - `DOC-01`：`.trae/rules/project_rules.md` §1.3 重写、§7.0 路径修正，版本升至 v2.5.0
  - `DOC-02`：本清单过时结论与破路径已刷新
- ✅ **P2 首项 `DEAD-01` 闭环（2026-10-06）**：
  - 删除 10 个 machete 未使用依赖（根 crate 8 + `synapse-web` 2），`cargo machete` 复验 0
  - `cargo machete` 接入 `.githooks/pre-push` **Stage 3（阻断级）**
  - `SQLX_OFFLINE=true cargo check --workspace --all-features` 通过；`Cargo.lock` 已刷新
- ✅ **P2 门禁补强项 `COMPAT-06` 闭环（2026-10-06）**：
  - `check_sqlx_cache_fresh.sh --static` 新增「源码查询指纹 vs 缓存」新鲜度断言（缺条目即 fail）
  - 复用 `sqlx_query_census.py` 词法扫描器，纯静态核对 1360 处查询字面量（无需 DB/编译），非常量实参跳过
  - 断言仅在 `--static` 运行（`--full`/`--compile` 已有更权威内容核对）；`sqlx_cache_tooling_guard_tests` 17/17 通过
- ✅ **P2 门禁补强项 `PERF-04`/`PERF-05` 闭环（2026-10-06）**：
  - `PERF-04`：`check_connection_budget.py` 改为解析真实运行配置（`database.rs` 默认 / `homeserver.yaml` / `docker-compose.yml` 三源一致性 + `postgres.conf` 的 `max_connections`），断言「文档 == 代码 == 运行」并在真实值上校验不变式；同轮修掉真实漂移 `postgres.conf` `max_connections` 200→250
  - `PERF-05`：基线参数（`8`/`4`/`20`）抽到 `scripts/ci/memory_budget_baseline`（缺失/缺项 fail-closed），`high` 风险由警告改为阻断（`exit 1`）；`CLAUDE.md` 基线清单补列该文件
- ✅ **P2 协议/兼容/性能/文档批次闭环（2026-10-06，全面报告 §8.6）**：
  - `COMPAT-01`（误报复核）：MSC4155 官方语义已实现 → 保留能力广告，订正 `MSC_SEMANTICS.md`
  - `COMPAT-02`：`.well-known/matrix/support` 由硬编码 `https://matrix.org` 改为读 `support_url` 配置（未配置返回 `{}`）
  - `COMPAT-03`：`M_UNRECOGNIZED`/`Unimplemented` 归一为 **404**（规范无 501 errcode）
  - `COMPAT-04`：删除非规范 errcode `M_UNSUPPORTED` 全链；presence 改 `M_FORBIDDEN`（403）
  - `PERF-02`：统一基线新增复合索引 `idx_events_type_origin_ts(event_type, origin_server_ts DESC)`；`INDEXES.md` 350/140 → 351/141
  - `PERF-03`：`get_user_tokens`/`get_updates_by_status` 各加 `LIMIT`（上限 10_000）+ 命中 `warn`
  - `DOC-03`：`API_COVERAGE_REPORT.md` 三口径重算至 1159/921/813（与 `ROUTE_CONTRACT.md` 同 HEAD）
  - `DOC-04`：comparison 三类 ground truth 订正；守卫新增 `stale_claim_violations`，8/8 通过
  - `DOC-05`：`CHECKLIST.md` 三处结论订正（迁移计数、依赖状态）
  - **结果**：§6.2「短期（P2）」第 3–7 项全部闭环
- ✅ **P3 卫生批次闭环（2026-10-06，全面报告 §8.7）**：
  - `CQ-04`：改走 CI 阈值门禁 `check_file_size_ratchet.py`（500 行 + 257 条 grandfather 基线），未拆分文件
  - `RED-02`：`search_index` 三处遗留引用清理（`lib.rs` 注释、`coverage_baseline.json` 幽灵条目、`trigram-audit.md` 失效结论订正）
  - `RED-03`：`coverage_baseline.json` 移除 8 条幽灵条目（复核 609 条 / 0 幽灵）
  - `RED-04`：`.gitignore` 新增 `.worktrees/` 忽略并清理残留目录
  - `DEAD-02`/`DEAD-03`：删除负载测试产物残留与 `.worktrees/` 副本
- ✅ **P3 收尾批次闭环（2026-10-06，全面报告 §8.8）**：
  - `CQ-01`：新建 `check_clippy_allow_ratchet.py` + 白名单（13 条带理由）+ 逐文件 grandfather 基线（176/108 文件，只降不升）；group lint 硬禁 + 扫描 0 处 fail-closed，接入 `ci.yml` `repo-sanity`
  - `RED-01`：`event/create.rs` 两个调用方的逐字节重复 INSERT 抽为 `insert_event_row_with_graph`（5 → 4 处）
  - `DOC-11`：守卫新增 markdown 链接形态解析（红证明通过）、`docs-quality-gate.yml` 纳入 `migrations/*.md`、`.trae/rules/` 改由本地守卫覆盖（CI 缺席优雅跳过）
  - **结果**：§6.3「中期（P3 / 决策）」第 8/9/10 项全部闭环
- ✅ **P2 收尾批次闭环（2026-10-06，全面报告 §8.9）**：
  - `SEC-01`：联邦 `m.signing_key_update` EDU 落库前新增 32 字节 ed25519 公钥校验（`decode_base64_32`，失败 `dropped += 1` + `warn!`）
  - `FED-02`：新增共享 `verify_inbound_join_pdu_integrity` 并接入 `/send_join` v1/v2；「有则校验/无则放行」兼容规范模板请求体
  - `PERF-01`：`get_token` 双查询回退合并为单查询（`WHERE token_hash IN ($1,$2) … ORDER BY (token_hash=$1) DESC LIMIT 1`），`.sqlx` 已重生成
  - `PERF-06`：`PERFORMANCE_BASELINE.md` 升 v1.1，连接池由 `5/100` 更正为 `5/50 (10%)`（分母取 `DatabaseConfig::max_size`）
  - `CQ-02`：11 处 `expect_used` 补 `reason` 证明注释、删除 1 处冗余 allow；ratchet 基线 176/108 → **164/99**
  - `CQ-03`：`membership/service.rs` 两处 `map_err` 保留原始 cause
  - `CQ-05`（最小方案）：`resolve_membership_from` 不可解析值改 `ApiError::internal`（fail-closed）；`state_map_auth.rs` 的 `membership_of` 改 `warn!` + `None`（降级可见）
  - **结果**：§2.B「P2 级问题（26）」仅余 **`COMPAT-05`**（`pgcrypto`/`pg_trgm` 扩展前置）未闭环
- ✅ **P3 剩余批次闭环（2026-10-06，全面报告 §8.10）**：
  - **批 A 文档与链接卫生**：`DOC-06`（链接已在 §8.2 修正，复核确认）、`DOC-07`/`DOC-08`（`migrations/README.md` 两类断链就地修复）、`DOC-09`（归档 5 份重复报告至 `docs/audit/archive/`；v12 系列被生产代码引用故保留）、`DOC-10`（`docs/audit/INDEX.md` 补 `docs/superpowers/plans/` 说明）、`CQ-08`（迁移/索引计数口径订正：218 表 / 342 索引，`INDEXES.md` 升 v1.4.0）
  - **批 B 代码质量与低风险安全**：`CQ-06`（`voice.rs` 裸 allow 补 `reason`；ratchet 基线 164/99 → **163/98**）、`SEC-02`（删 `cross_signing` 两处死代码 `upload_key_signature`/`upload_signatures`）、`SEC-03`（`csrf.rs` `session_id` 改 `secure_compare` 恒时比较）、`SEC-06`（`verify-security.sh` 硬编码密码改 `${PRO_PASS:-…}`）、`PERF-08`（`media_service` 构造函数改 async + `tokio::fs`，全部调用点刷 `.await`）
  - **验证**：`cargo check`（三 crate）+ `cargo check --test unit --features test-utils` 均 exit 0；clippy（lib）无告警；`.sqlx` 静态新鲜度 1357/1357 命中；allow-ratchet `OK`
  - **门禁终验补充**：`fmt` 棘轮曾因更早轮次遗留回退至 **51 处**（本轮文件仅占 2 处，已就地修正）；经用户决策执行 `cargo fmt --all` 清空全部存量并固化 `scripts/.fmt-baseline` 为 **0**，`check_fmt_ratchet.sh`、allow-ratchet（163/98）、doc-credibility 守卫（10 passed）复跑均绿
  - **结果**：§2.C「P3 级问题（27）」余 by-design（`FN-05`~`FN-08`）与需决策项（`COMPAT-07/08/09`、`CQ-07`、`SEC-04/05`、`PERF-07/09`）
- ✅ **剩余收尾批次闭环（2026-10-06，全面报告 §8.11）**——按用户「需决策的给出合理建议，然后按建议处理」逐项给出建议并实施/复核/登记：
  - **A 代码改动（3 项）**：`CQ-07`（8 处无理由 `dead_code`/`unused_*` allow 三分类：真死代码 1 删 `mut`、feature-gated 2 补理由、test-only 5 补理由；均 rustc lint，ratchet 基线不变 163/98）；`FN-03`（`get_cross_signing_keys` 签名由恒空修正为从 `signatures` 提取——新增 `master_key_id`/`extract_signature_for`，`verify_cross_key_signature` 复用去重，补 4 单测；该函数无生产调用点）；`FN-04`（`preview_url` 补 Stub 文档注释，行为不变）
  - **B 复核闭环（6 项）**：`COMPAT-05`（迁移内 `CREATE EXTENSION IF NOT EXISTS` + README 文档化前置，见 §3.4）、`SEC-04`（撤销写入主动失效覆盖缓存窗口）、`SEC-05`（per-IP 双桶节流 `LOGIN_MAX_IP_ATTEMPTS=30`）、`PERF-07`（谓词改 sargable `sender = $1`）、`PERF-09`（模拟测试补诚实头注释）、`COMPAT-08`（超时 300→900 + env 可覆盖）
  - **C by-design 登记（6 项）**：`COMPAT-07`（`.well-known` 逐端点复核即闭环）、`COMPAT-09`（稳定/unstable 双前缀并存）、`FN-05`~`FN-08`（501/410 桩、legacy keys、MSC 编号借用）
  - **验证**：`cargo check`（三 crate）exit 0；clippy（lib）无告警；`.sqlx` 静态新鲜度 **1357/1357** 命中；allow-ratchet **163/98 OK**；`fmt` 棘轮 **0/0 OK**；`doc_credibility_guard` **10 passed**；`cross_signing` 单测 **44 passed**（含新增 4 项）
  - **结果**：**§2.B 26 条 P2 全部闭环**（末项 `COMPAT-05`）；**§2.C 27 条 P3 全部处置**（21 闭环 + 6 by-design）
- ✅ **既有编译错误排查（2026-10-06，全面报告 §8.12）**：`cargo check/clippy --all-targets` **不带** `--features test-utils` 时的 **312 errors**（`E0282×190`/`E0432×47`/`E0433×75`，`synapse-services`/`synapse-web` lib test）查明为**缺 feature 的调用假阳性**——`test_mocks` 以 `#[cfg(any(test, feature = "test-utils"))]` 门控，跨 crate 需 feature 传播。证据：带 `--features test-utils` → **EXIT=0 / 0 error**。**结论：by-design，非缺陷，不改代码**（CI/preflight/`.cargo/config.toml`/`.rust-analyzer.toml` 已一致落实该 flag）
- ✅ **创建 [FINAL_AUDIT_SUMMARY_20261006.md](FINAL_AUDIT_SUMMARY_20261006.md)（遗留问题审计最终总结报告）**：结项收敛——0 P0 / 3 P1 / 26 P2 / 27 P3 全部闭环或 by-design，9 条历史结论证伪；汇总基线门禁全绿终态、§8.1–§8.12 闭环批次与 6 项 by-design 残余；明细指向权威全面报告，不重复
- ✅ 更新本索引：新增报告条目、刷新待处理问题表与使用指南

### 2026-10-04

- ✅ 创建此索引文档
- ✅ 归档 41 个过时报告到 `archive/`
- ✅ 更新 `UNRESOLVED_ISSUES_SUMMARY.md`：
  - 客户端撤回级联验证为已实现
  - Animated 缩略图验证为已实现
  - Content Scanner 标记为误判
- ✅ 补齐 Admin 媒体端点族：
  - `purge_media_cache` 路由已添加
  - 契约产物同步更新

### 2026-09-28

- ✅ P0 问题全部解决（PDU 语义、Event ID、Soft_failed）
- ✅ A5 联邦互操作测试完成
- ✅ MSC3912 客户端撤回级联实现
- ✅ 动画缩略图 Phase 2 完成

---

## 📞 联系与反馈

如有问题或需要更新此索引，请联系项目维护团队。

**文档维护者**: Audit Team  
**最后审核**: 2026-10-06  
**下次计划更新**: 2026-10-13
