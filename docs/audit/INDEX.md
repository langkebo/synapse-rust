# Synapse-Rust 审计文档索引

## 📋 活跃文档（当前有效）

### 核心问题追踪

> **先看下面第一行**。`COMPREHENSIVE_AUDIT_2026-10-22.md` 是**逐轮更新的 living audit**
> （只写"还没解决"的），其余 2026-10-06 的报告是**历史快照**，其"当前权威"标注指的是当时。

| 文档 | 描述 | 更新日期 |
|------|------|----------|
| **[COMPREHENSIVE_AUDIT_2026-10-22.md](COMPREHENSIVE_AUDIT_2026-10-22.md)** | **未解决问题清单（living audit，逐轮更新；已关闭条目正文在 `archive/` 的 closed-detail）** | 2026-10-24（第七轮） |
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

### 上游对齐

| 文档 | 描述 | 状态 |
|------|------|------|
| **[UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md](UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md)** | 上游 `changelog.d`（24 条）逐条对照与对齐优化方案 | ✅ Active |
| **[../synapse-rust-vs-synapse-comparison.md](../synapse-rust-vs-synapse-comparison.md) §19** | 上游 **Synapse v1.163.0**（2026-10-14）对照 + 本仓现状重判（当前口径；含 P0/P1 差距与批次 A–E 方案） | ✅ Active（2026-10-16） |

### Worker 与水平扩展

| 文档 | 描述 | 状态 |
|------|------|------|
| **[WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md](WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md)** | Worker 拓扑验证现状审计与完善方案（W-1…W-9 缺口 + 批次 W-A…W-E；判定"静态校验已成型、运行时对账/强制力/参考部署未做"） | ✅ Active（2026-10-16） |

### 测试质量与门禁

| 文档 | 描述 | 状态 |
|------|------|------|
| **[CI_LANE_BLINDNESS_2026-10-27.md](CI_LANE_BLINDNESS_2026-10-27.md)** | CI 车道盲区（两条长期没读数的车道）：①`Repo Sanity` 的第一条检查因**夹具假阳性**（`UPDATE t`）恒红，同 job 后面 **22 个门禁全部 `skipped`** ⇒ 它们的沉默不能当作"已合规"证据；②`OpenAPI Artifact` 的 `client.yaml` 步用 `--skip-export` 读 `.gitignore:44` 的路径 ⇒ **干净 checkout 上永远失败**（从未成功过）。两条均已修；本地 27 个门禁实测 **26 过 / 1 假红**（gitignored 的联邦测试证书，CI 上不存在）。§7–§8 继续把"存量红"拆成四类成因：门禁清单没配齐（天生红）/ 被守对象真坏 / **门禁把绿判成红**（`require_tests_ran.sh` 只认复数）/ **基础设施抖动**（`exit 143` + `shutdown signal`） | ✅ Active（2026-10-28） |
| **[TEST_COMPROMISE_AUDIT_2026-10-25.md](TEST_COMPROMISE_AUDIT_2026-10-25.md)** | 测试降标审计：测试基础设施 fail-open 导致 9 条安全边界用例静默假绿；覆盖率门禁三缺口（含**基线可被一次不完整 lcov 清零**）；性能验收只有报告没有断言；mock 语义漂移。**§7 是"改了什么"的一页索引**（门禁 / 缺陷 / 可测性 / 复核命令 / 0% 分流（分母订正为 32：**10 陈旧**（全部已实测非零）/ 4 真 0 / 3 `feature` 关（已补 feature 集）/ **5 无可测行** / **2 跨 crate 转发层**（已补 `&dyn` 契约测试）/ **8 已消除**） / 覆盖率车道解耦（H）/ 地板抬升 20 条（I） / 途中被证伪的十四个判断；**§3.1.3 = P2「提升到 30%」的逐文件台账**（`scripts/ci/coverage_raise_queue.json` + `check_coverage_raise_queue.py`：`target` 不是 floor，达成即删，记账起点不得高于基线；**2026-10-28 已关账** —— 三条实测 82.9 / 68.54 / 48.24 存入 `closed.measurements`，判据 9–12 钉住"删空≠关账""测到了≠基线收了"）；**§3.1.7 = 「低于基线」的两种成因分不开**（基线加 `lines_found` 且**必须与 `line_pct` 成对**（回归分支只留旧分母、老条目保持缺失）；分类器新增 `DENOM`/`LOST-HITS`，未知打 `?→N` 不填 0；迁移 608 条：417 带分母、257 收紧、0 下降、**74 个新文件刻意不写入**（否则把 70 条 `[NEW]` 洗成"不回退即可"）；§3.1.7.6 把遗留 128 条压成人看 16 条：**20 条可证**（2 零改动 + 4 纯新增 + 14 内联测试外移）+ **92 条最简解释是分母增长**（净增长，非证明）+ **16 条必须人看**（core 4），并记了一个**试过并否掉**的行数外推估计器（拿 6 条已证样本校准就破，误差来自内联测试外移）；**§3.1.7.7 = 四条 core L4 的逐行裁决**（读 lcov `DA:` 而不是看比例：`auth_discovery.rs` 7/36=19.4% 是**成功路径零测试**的真缺口，已补两条集成用例 + 两处变异自证；`login.rs` 96.9% / `device_keys/models.rs` 98.9% 判定无需动作）；**§3.1.7.8 = `admin/register.rs` 的门禁分支**（60 条未命中行全在**非默认配置**：IP 白名单 / captcha / 人工审批 / 未启用 / 验签映射 —— 这些是**授权控制**，默认关着所以从没被测过；补 9 条集成用例 + 3 处变异自证，并纠正"既有 XFF 用例其实测的是 LocalhostGuard 而非 handler 白名单"））；**§3.1.4 = PR 覆盖率车道**（`pr-coverage` job + `affected_crates.py` / `run_pr_coverage.sh` / `check_changed_line_coverage.py`：只跑受影响 crate，判据是**变更行下界**而非比对基线 —— 实测 `src/bin/*` 合并 10.91–59.06% 对单腿 0.00%，比基线会常态假红）；**§3.1.3.1 = `logging.rs` 实施记录**（补测试时撞出两个真缺陷：二次安装是 panic 而非 `Err` ⇒ 调用方错误分支不可达、`RUST_LOG=` 空值让全进程静默连 `Logging initialized` 都不打）） | ✅ Active（2026-10-28 第 13 轮） |
| **[TEST_QUALITY_FINDINGS_2026-10-28.md](TEST_QUALITY_FINDINGS_2026-10-28.md)** | 测试代码质量整改的**问题总账**：把 A 类（判据把绿判成红，8 条假红，含 pre-push 把 advisory-db 拉取失败当漏洞）、B 类（判据结构上看不到，`skipped` ≠ 绿）、C 类（门禁修对后浮出的 7 条真实缺陷）逐条列状态，再单列 **10 条现存未解决问题**（覆盖率 134+6 待逐条定性 / 棘轮 `max` 只上不下 / 大盘近 12 次无绿 / 门禁拆 job / `always()` 依赖前置产出 / `ROUTE_CONTRACT` 跨午夜漂移 / 行数棘轮与 worktree 互斥 / 判据仍建在日志文本上）。**§6 是这 10 条的逐条处置记录**（2026-10-28 第二轮，提交 `21881328f`）：机制化判据 2 条、归因分离 2 条、棘轮出口 1 条、CI 结构 2 条，另有 **2 条推翻原诊断**（`No files` 实为 warning 不失败；行数棘轮与 worktree 的互斥 10-23 就已修好） | ✅ Active（2026-10-28） |

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

### 2026-10-27

- ✅ **[CI_LANE_BLINDNESS_2026-10-27.md](CI_LANE_BLINDNESS_2026-10-27.md)** 新建：**两条 CI 车道长期没有效读数**。
  - **①`Repo Sanity` 的 22 个门禁没跑**：`check_schema_table_coverage.py` 逐字扫文件原文，把
    `tests/unit/sqlx_ratio_gate_tests.rs` 内嵌源码夹具里的 `sqlx::query!("UPDATE t …")` 当成真表 ⇒ `t`
    报"未在迁移中定义"。引入点 `git log -S CONTINUATION_SOURCE` → `15efc8dc6`（"unit 车道转全绿"）
    **修绿一条车道、弄红另一条的第一步**；而 GitHub 步骤是顺序短路 ⇒ 该 job 实测
    `failure 1 / skipped 22 / success 8`，**22 个门禁的读数整体丢失**。
    修法遵既有约定（`sqlx_dynamic_literal_guard_tests.rs:474-478` 早写明"夹具表名必须用真实表"）
    ⇒ 三处 `t` → `events` + 把约定写进注释；变异自证：探针表 `definitely_missing_table_xyz` 仍被抓（exit 1），删除后 exit 0
  - **①的第二个阻塞**：`file_size_baseline` 落后 `f2031f559`（`synapse-test-utils/src/lib.rs` 2974 → 2981）⇒
    `--update` 后 diff 仅 2 行（另一行是 `b7ed3fc47` 把 `pruning.rs` 缩到 925 行后的**排序位移**，非删除），条目 222 → 222
  - **②`client.yaml` 步从未可能成功**：`gen_client_yaml.py --skip-export` 读硬编码的
    `scripts/api_test/reports/ledger_default.json`，而 `reports/` 在 `.gitignore:44` ⇒ 干净树里没有它。
    修法：脚本新增 `--ledger PATH`（隐含跳过 export），CI 指向刚导出的 `$RUNNER_TEMP/ledger_default_ci.json`；
    实跑三条路径（正常 / 负例 exit 1 / 旧 `--skip-export` 不变），产出与旧路径**逐字节相同**
  - **③`Test & Lint (--all-features)` 的 unit 车道红在一个 gitignored 负向样本上**：`24f123805` 起
    `code_referenced_docs_exist` 报 `docs/superpowers/plans/2026-09-22-protocol-correctness-phase1.md`
    不存在 —— 该目录在 `.gitignore:113`，开发机上有文件、CI 干净树上没有 ⇒ **本地永远绿、CI 永远红**。
    已按守卫自己给的处置方式把该路径补进 `INTENTIONALLY_ABSENT_DOCS`（5 → 6 条）
  - **复核**：把 ci.yml `repo-sanity` 的 27 个 `run:` 块抽出来逐个实跑 ⇒ **26 OK / 1 假红**
    （`No private keys (key files)` 只因本地存在 `.gitignore:153` 的联邦测试证书，CI 上是 `success`）
  - **仍未做（独立决策）**：门禁拆 job / `if: always()` 汇总，避免"一红全盲"再发生；
    `gen_client_yaml.py --check` 的 `--expected` 默认指向未入库的 `client.yaml`，该分支在任何机器上都会失败（不在 CI 路径上）
  - **推送后的真实复核（`ea9a57af1`）**：`Repo Sanity` 由 `failure 1 / skipped 22 / success 8` 变成
    **`success 26 / failure 1 / skipped 4`** —— 22 个门禁**首次真跑**。唯一红是 `File-size ratchet`：
    缩略图两笔（`1ab01c9a9`/`ea9a57af1`）让 `media_service.rs` 1462→1620、`admin_media.rs` 1300→1443，
    **不是假红**；抽文件回不到上限内（`media_service.rs` 有 34 行散在既有函数体内，抽走可移动的 124 行
    仍有 1496 > 1462）⇒ **经确认后 `--update` 让渡**，基线 222→222 条、按内容 `sort` 后差异仅这 2 个数值、
    实跑 `EXIT=0`。记录为**让渡而非修好**（两文件此后不得再涨）
  - **车道闭合**：让渡后 `d297b2937` ⇒ `Repo Sanity` **31/31 步全绿、0 skipped**
    （自引入点 `15efc8dc6` 起第一次真正全绿）
  - **同批复核作废一条登记**：`missing_docs` 棘轮实测 `EXIT=0`、`debt 0 = baseline 0` —— 它此前"红"
    只是排在 `Run unit test target` 之后被 `skipped`（一红全盲的又一个受害者），并非真红
  - **`gen_client_yaml.py --check` 的死默认 + 三处"CI 有这个门禁"的失实**：`--expected` 默认指向
    gitignored 的 `docs/openapi/client.yaml` ⇒ 该分支在任何机器上只能报 `reference artifact not found`；
    且 `scan_handler_schemas.py:17`、`scripts/api_test/README.md:170`、`artifact_common.py:25`
    都把 `--check` 写成已接线的 CI 门禁，而 CI 里带 `--check` 的只有 `gen_route_table.py`
    （对账**入库**的 `route-table.json`，ci.yml:362）。修法：去掉死默认、`--check` 必须显式给
    `--expected`（校验前移到 `parse_args` 之后，缺参 0.08 s 失败而不是先跑一遍 cargo export），
    三处声明改实情。三条路径实测：缺参 exit 1 / 一致 exit 0 / 漂移 exit 1
  - **覆盖率那次的真红因也在这里**：`Code Coverage` 首次真跑（103 min），红在
    `doc_credibility_guard_tests::code_referenced_docs_exist` = §③ 那个 gitignored 负向样本，
    已由本轮的 `INTENTIONALLY_ABSENT_DOCS` 修掉；干净树等价复现（移走该文件后 18 条守卫全绿）已做
  - **再一轮：缩略图让 `baseline_tables` 的硬编码例外过期** ⇒ `Test & Lint (×2, all-features)` 红在
    `Run library unit tests (--workspace --lib)`、`Code Coverage` 红在**步骤 1/3**
    （`cargo llvm-cov -p synapse-storage --lib`），**同一根因**且都排在 §4 那条守卫之前。
    `1ab01c9a9` 把基线 v11-10 的例外列表改成 `NOT IN ('uq_to_device_txn_msgid', 'uq_thumbnails_variant')`，
    而模型把这句当 marker、把例外集合硬编码 1 条 ⇒ 基线每加一个例外就假红，且方向错成"`uq_thumbnails_variant`
    会被清掉"（基线实际保留它）。修法：**从基线解析例外列表**（`v11_10_kept_indexes()`）、
    只留两个结构 marker、加 `V11_10_REQUIRED_KEEP` 兜底解析失效、新增测试钉 `uq_thumbnails_variant`。
    实测 `14 passed`；变异自证（锚点改 `uq_%ZZZ_MUTANT`）⇒ **6 条同时红**，还原后 0 残留
  - **"存量红"标签掩盖的两件不同的事**（§7）：
    - **`DB Migration Gate` 的红是门禁天生红**：七个子 job 只 `Schema Blind Guards` 红；判据是告警预算，
      `WARNING_BUDGET` 只登记 SBG-03 一条（`allowed: 1`）而实际 3 条 —— 另两条
      `scripts/dev-test-setup.sh:61`（`ff56f36384`，10-18）与 `scripts/run_full_integration.sh:136`
      （`94d9e78bb8`，10-22）都是**合法的测试库前置断言**，只是从未登记；门禁上线（10-19）时第一条已在树里
      ⇒ **从第一次运行起就红，从没有过绿的状态**。顺带查出 `SAFE_FILES` 是**整文件**豁免：`ci.yml`
      仅因一条 SBG-03 计数查询被豁免，连带对 `DROP SCHEMA public CASCADE`（SBG-01，本门禁唯一要拦的东西）也免疫。
      修法：补登记 + 豁免收窄为**逐规则**（新增 `rules` 字段）+ `validate_exemptions()` 启动自检
      （缺 `reason`/`rules`、规则编号写错 ⇒ 立即失败，否则"登记了但登记错"静默等于没做）。
      读数：命中 20→20 条不变、`(规则,文件)` 集合逐条相同、`INFO 17→19` / `ERROR 2→0` / `EXIT 1→0`。
      **A/B 自证**：同一份变异（往 `dev-test-setup.sh` 加真 `DROP SCHEMA public CASCADE`）下，
      旧脚本降级为 `INFO`、新脚本报 `ERROR`
    - **`Docker Security Scan` 的红是真 CVE，但落在没人构建的目标上**：从 `trivy-base-sarif`
      artifact 解析出唯一一条 —— `libssl3 3.0.20-1~deb12u2` / `CVE-2026-84782` / HIGH /
      修复版 `3.0.22-1~deb12u2`，在 `distroless/cc-debian12` 里。
      **换 pin 没用**：实测 `e5d81dd…` **就是** `cc-debian12:latest`（上游未重建，层内文件仍是
      2026-06-07 的 `3.0.20`）。而这条扫描针对的 `runtime-distroless` 目标**全仓无构建/部署入口**
      （compose 与 `deploy.sh` 都是 `--target tools`），真出货镜像的 `Trivy Image Scan` 一直是
      success（`tools` 阶段构建期 apt 装到 `3.0.22-1~deb12u2`，实测）。
      **裁决**：不用 `.trivyignore` 压制，删掉 `runtime-distroless` / `runtime-libs` 两个 stage
      （= 归档审计 DOCKER_REVIEW_2026-09-19 的 P0-1/O1-1「B. 废弃」），`Base Image Scan` 只留
      debian（真出货根，阻断）+ builder（report-only）。另补 `resolve-base-image-digests`
      （`workflow_dispatch` only）让"pin 落后"可被自动读出，并修正一条指向**不存在测试**的
      守卫注释（原文指 `ci_test_scope_tests.rs` 里没有的函名）。
  - **门禁把"绿"判成红**（§8.1）：`Appservice statistics cost gate` 自 `ea8276d8e` 引入起**从未绿过**。
    该步骤用 `-E 'test(statistics_per_service_cost)'` 只选中 **1** 条用例，而 nextest 的计数行是
    **单复数变化**的（`Starting 1 test` vs `Starting 3 tests`），`require_tests_ran.sh` 的正则只写复数
    ⇒ 两条 isolation 车道的日志里明明是 `PASS [3.331s] (1/1)` + `Summary 1 test run: 1 passed`，
    却都报 `this step ran ZERO tests`。这是同一脚本第三次"判据写死单一输出形态"
    （09-20 ANSI 颜色、09-30 `grep -q` + `pipefail` 的 SIGPIPE、10-27 单复数）。
    修法：正则改 `tests?` + 补单数正控与 `Starting 0 tests` 反控；A/B 实测旧脚本 `exit 1`、新脚本 `exit 0`
  - **两条真实供应链条目被 `skipped` 掩盖**（§8.2，`Security Audit` 是路径过滤触发，近 12 次 `ci.yml`
    里 5 次 `skipped`）：①`tokio 1.53.1` / `RUSTSEC-2026-0386`（`AsyncFd` I/O safety，`unsound`，
    补丁区间 `>=1.53.3`）⇒ 已升 `1.54.1`（全仓 `AsyncFd` 零引用，暴露面为零）；
    ②`xxhash-rust 0.8.19` 的 `BSL-1.0` 不在名单内（`022cbd67f` 把 redis 0.29 → 1.7 时的**传递依赖**）
    ⇒ 按 `webpki-roots` 同口径加 **crate 级例外**，刻意不塞全局 `allow`。
    本地 `cargo deny check licenses` ⇒ `licenses ok`；`advisories` 手动跑因
    advisory-db **git 协议被拦**（`error sending request ... rustsec/advisory-db`）失败，但 pre-push
    钩子用缓存副本报 `advisories ok`，且缓存 HEAD `99670cfc`（2026-10-27）里
    `crates/tokio/RUSTSEC-2026-0386.md` 在册 ⇒ **该条已清**（最终仍以 CI 为准）
  - **两红不是代码问题**（§8.3）：`Benchmark` 的 `Run benchmarks` 是 runner 被回收
    （`shutdown signal` / `exit 143`，日志停在 `Compiling synapse-rust`，两次同型）；
    `Assert no leftover test databases` 是 `out of shared memory` 打断用例后的**次生**残留。
    大盘事实：近 12 次 `ci.yml` **没有一次 `success`** ⇒ "某个 job 红了"在本仓已失去信息量
- ✅ **[TEST_COMPROMISE_AUDIT_2026-10-25.md](TEST_COMPROMISE_AUDIT_2026-10-25.md)** §7.7：**覆盖率车道第一次跑通，棘轮红在 134 个文件**（`0a5310346` / run `40469375466`）。
  - `Run coverage`（三条腿）**success**，约 75 min；`Code Coverage` 的红从"跑不到"变成"跑到了不达标"，
    失败步 `Per-file coverage ratchet (B.3)`
  - 两类信号：①**6 条 core-path** `was no baseline`（`credential_auth.rs` / `token_auth.rs` /
    `history_boundary.rs` 均为 **0.0%** < 70%，另 `cascade.rs` 48.6%、`msc4512_proxy.rs` 24.8%、
    `worker_topology.rs` 45.1%）；②**134 条** 已有地板但下降，幅度异常（`tracing.rs` 74.6%→19.4%、
    `federation/client.rs` 59.5%→36.8%、`admin_media_service.rs` 100%→72.8%）
  - **判定：性质未定，明确禁止 `--update`**。§7.7.1 补记给出第一条硬证据：`tracing.rs` 代码
    **零变更**（101 行，`090742266..HEAD` 对它是 0 个提交）却 74.59%→19.4% ⇒ 这 55 个点只能来自
    "跑的测试集变了"；且 per-file 棘轮此前**从未在 CI 跑过**（`c952ee2b0` 说明：最近 30 次
    `Code Coverage` 恒 skipped）⇒ 134 条实为"**首次差异**"，不是"回归"
  - 原候选"3 个 feature 改分母"**不足以解释主现象**：那三个只进 leg 1（storage `--lib`），
    `tracing.rs` 属 leg 2；`merge_lcov.py` 是按行取 max 的**并集**，也不制造下降
  - **§7.7.2 收口（2026-10-27）**：`tracing.rs` 的 55 个点是**基线跨文件重写**，非测试集变化。
    基线首现 `4565cecd5` 时该文件 248 行（含 `DistributedTracer` + 10 个内联测试）；
    `090742266` 删 149 行死代码后缩到 101 行、只剩 1 个只覆盖 `Display` 的测试 ⇒ 19.4% 与
    `cargo llvm-cov -p synapse-common --lib` 本地读数**逐位相同**。`save_baseline()` 取 `max`
    且 CI auto-ratchet 从未成功 ⇒ 合法重写被永久冻红。原"集成测试静默跳过"候选**已证伪**
    （layer 唯一注册点 `logging.rs:51`，测试代码零命中）。处置 = 补 2 个内联测试（`tracing.rs:135`
    / `:170`），实测 **84/85 = 98.82% > 74.59%**，棘轮自然转绿，基线未动
  - **§7.7.3**：`--exclude` → `--exclude-from-test` **本轮不采纳**（不解释本轮的红；§⑤/⑦ 已用
    补测试路线关掉；本地无法验证 + 两个未验证风险），重开条件见原文
  - **下一步**：6 条 core-path 按 §7.5 方法逐条定性；口径判定明确后才允许动 `coverage_baseline.json`
- ✅ **[TEST_COMPROMISE_AUDIT_2026-10-25.md](TEST_COMPROMISE_AUDIT_2026-10-25.md)** 第 9 轮：**最后一个开口（原 ⑦ 类 8 个 0%）关掉**，代价 6 分钟。
  - **改判**：`synapse-web/routes/**` 那 8 个不是"只能等 CI 的 integration 腿"，而是**陈旧（地板冻结）**。
    8 个文件都在 `synapse-web` crate、模块**无条件**编译、**每个都自带内联 `#[cfg(test)] mod tests`**
    ⇒ `cargo llvm-cov -p synapse-web --lib`（6m02s，**967 passed**）一次全出：
    `extractors/mod` **100%**、`federation/media` 71.0%、`sticky_event` 53.5%、`membership/query` 44.1%、
    `visibility` 43.8%、`upgrade` 32.1%、`admin/room/spaces` 20.3%、`transaction/edus` 15.2% —— **无一为 0**
  - **0.0 的真因**（三条独立证据）：①`git show 4565cecd5:`（2026-09-19 bootstrap）里这 8 个文件**一个测试都没有**；
    ②`save_baseline()` 只 `max(prev,cur)`，抬升通道只有 CI 的 auto-ratchet 步；
    ③该步**从未成功**：`git log --all --grep="auto-update per-file baseline"` = **0 条**
  - **顺带证伪"等 CI"整条路**：最近 30 次 CI 全 `failure`，抽查 6 次的 `Code Coverage` / `Integration Tests`
    结论**全是 `skipped`**（`coverage` 需 `needs: [integration-test]` → `integration-test` 需 `needs: [test]`，
    `test` 恒红 ⇒ 下游一律 skip）⇒ 基线里 32 个 0.0 是 bootstrap 的**冻结值**，不是当期读数。
    与 3 个 `feature` 关文件同性质：**11 个 0% 的"理由"其实是同一条永远不会兑现的等待**
  - **未改的东西（有意）**：~~`coverage_baseline.json` 一个字节没动~~ → **第 10 轮已改，见下**
- ✅ **同文档第 10 轮（2026-10-27 晚）**：把上一轮留下的三条"独立决策"全部做掉 + 关掉 §4.2.2 的"待查"。
  - **C（先做，因为原判定会导致误删生产代码）**：`room/api.rs` / `sliding_sync/api.rs` 的 0% **不是**
    "5 行死默认实现"。LLVM 实测可执行行是 **101 / 96**（机械口径漏计 ~95%，纯 `.await` 转发体一个关键字都没有）；
    生产装配把真实例装进 `Arc<dyn …>`（`wiring/rooms.rs:88` 等 20+ 处）⇒ **删掉 services 编译不过**。
    补 **12 条 `&dyn` 契约测试**后实测 **28.7% / 14.6%**（12 passed / 116 s）
  - **覆盖率车道解耦（§7.5 H）**：`coverage` 的 `needs: [integration-test, changes]` → `[changes]`。
    `run_coverage.sh` 自己跑两条腿、不消费 integration 的任何产物 ⇒ 那条依赖只带来"永远不发车"
  - **`STORAGE_FEATURES` 补 `friends,widgets,burn-after-read`（§7.5 F）**：三个 feature 门控的近 3800 行
    此前**根本不编译**；现在 **71 条用例全绿**（27 + 19 + 25），下次 coverage 即产出读数
  - **地板抬升 20 条（§7.5 I）**：608 → 608 条，**0 新增 / 0 降低**。8 个 web 路由 0.0 → 15.2%–100%，
    两个 `api.rs` 0.0 → 28.7%/14.6%，另 10 个 web 文件按 lib 腿下限上抬。刻意**不用** `--save-baseline` 写整份报告
    （那会把 17 个未入库文件按单腿值"转正"，等于提前豁免 ramp-up 门槛）
  - **§4.2.2 四处"待查"全部定级**：2 良性（`acquire_lock_with_retry` 的核心语义 mock 保住了且有断言依赖）/
    1 无断言依赖（`validate_proxy_ticket`）/ 1 走不到（`claim_to_device_events`）/ 1 已声明偏差
    （`get_device_list_changed_users` 缺逐设备流位置，得改数据模型）
  - **仍未闭合**：3 个 `feature` 关文件在基线里仍是 0.0（本地全量 storage 腿 **53 min 未跑完**，不手写编数）；
    `Test & Lint` 的四条独立红灯；`test` 绿之前 auto-ratchet 仍不会落地

### 2026-10-26

- ✅ **[TEST_COMPROMISE_AUDIT_2026-10-25.md](TEST_COMPROMISE_AUDIT_2026-10-25.md)** 收敛（39 笔提交）。要点：
  - **修掉一个活缺陷**：`setup_fresh_test_app_with_config` fail-open ⇒ 9 条安全边界用例在 DB 不可达时
    **静默假绿**（且比真跑更慢，30 s vs 6 s）。入口改 fail-closed + 7 处夹具改 panic + 守卫
    `check_test_setup_fail_closed.py`（preflight，扫 74 个夹具）
  - **修掉一个门禁自毁**：`check_file_coverage.py` 在默认参数下**写回**基线且只遍历本次 lcov 的键
    ⇒ 一份不完整报告曾把 `coverage_baseline.json` 从 **608 条砍成 1 条**、零警告。现改为保留 previous 的键
  - **新增三条门禁**：mock「忽略参数却假装成功」棘轮（106 → 102，只降）、改动面→必跑套件提示、
    **第一条能进 CI 的性能门禁**（每行成本折算 DB 往返次数，注入 N+1 实测 1.03 vs 正常 0.03–0.06）
  - **审计外发现并接通一条安全能力**：refresh token 重放"撤销全族"曾因三层判断互相阻断而**从未生效**
    （`validate_token` 入口即拒已撤销 token；第 1 层参数传错恒空转；第 2 层把重放判成良性并发）
    ⇒ 抽 `load_token_inner` + 5 s 并发窗口（两层都加）+ mock 有状态化，双向变异验证
  - **九条曾被写进报告的判断被自己证伪**（"6 处 mock 高危"→0 假绿、静默期测试→假测试、
    "看板会误读台账"→台账没有生产读者、"修第 1 层即可"→第 0 层才是阻断点、
    `deny_unknown_fields` 会让 `ts` 用例先红→不会、3 个 feature 关文件"陈旧"→根本没编译、
    22 个 storage mock"陈旧"→先是两腿报告口径、最终查明**门禁的 `TEST_ONLY_MARKERS` 压根不看它们**、
    `config/manager.rs`"最实的真缺口"→`ConfigManager` **整类零构造点**、`common/error.rs` 两个 helper 是死代码），
    全部记在报告 §7.6 —— 这类"看起来像 X"的结论，必须先 `grep` 消费者或做变异自证
  - **0% 文件的"59 个"是错的分母**（报告 §7.5 三次订正）：22 个 `test_mocks/` 与 5 个 `src/bin/*`+`main.rs`
    本就在门禁的两张排除清单里（`TEST_ONLY_MARKERS` / `non_unit_coverable_prefixes.txt`）⇒
    **真正落在棘轮里的是 32 个**：10 陈旧（**全部已实测非零**）/ 4 真 0（已实测）/ 3 `feature` 关（永不刷新）/
    7 无实质可测行 / **8 已消除**（2 个 trait impl + `event/models.rs` + 3 个领域错误转换补测；
    `ConfigManager` 整类与 2 个死 helper 删除）；**"等 CI 会给出答案"这条已证伪**（见 2026-10-27 条）；
    另一个可执行发现：`run_coverage.sh` 第 2 腿用了 `--exclude`（连报告一起排除），
    想给 storage 记账只能用 `--exclude-from-test`（本轮改用"补 `&dyn` 转发层测试"达成同一目的）

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
  - `FED-02`：新增共享 `verify_inbound_membership_pdu_integrity` 并接入 `/send_join` v1/v2、`/send_leave` v1/v2 与 `/knock`（2026-10-27 扩展，见全面报告 §8.15）；「有则校验/无则放行」兼容规范模板请求体
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
**最后审核**: 2026-10-26  
**下次计划更新**: 随 living audit（`COMPREHENSIVE_AUDIT_2026-10-22.md`）逐轮更新
