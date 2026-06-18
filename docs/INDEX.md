# synapse-rust 文档索引

> 最后更新: 2026-06-12
> 维护原则: 现行文档集中在 `docs/synapse-rust/` 与 `docs/{db,quality,sdk}/`；
> 归档文档统一进入 `docs/archive/`，不再修改，仅供历史溯源。

---

## 一、入口与基线

| 入口 | 用途 | 状态 |
|------|------|------|
| [`README.md`](../README.md) | 项目门面、构建/运行/测试命令 | 现行 |
| [`AGENTS.md`](../AGENTS.md) | 给 Codex/Claude 的项目工作流指引 | 现行 |
| [`CLAUDE.md`](../CLAUDE.md) | Claude IDE 项目规则 | 现行 |
| [`TESTING.md`](../TESTING.md) | 测试分层与门禁定义 | 现行 |
| [`CHECKLIST.md`](../CHECKLIST.md) | 发布前自检表 | 现行 |
| [`CHANGELOG.md`](../CHANGELOG.md) | 版本变更日志（Keep a Changelog + SemVer） | 现行（v10.0.0 基线，2026-06-12） |

---

## 二、审查与基线报告（**现行**）

> 每次重大重构后必须更新到本节，并附 `last_updated` 日期。

| 报告 | 范围 | 末次更新 |
|------|------|----------|
| [`COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`](./synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md) | **当前基线**：P0/P1/P2 + Step 1-12 执行状态 + 30 项修复路线 | 2026-06-12 |
| [`MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md`](./synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) | Matrix v1.18 / Synapse v1.153 协议对齐 + 优化 backlog | 2026-05-29 |
| [`SYNAPSE_RUST_OPTIMIZATION_BLUEPRINT_2026-05-27.md`](./synapse-rust/archive/SYNAPSE_RUST_OPTIMIZATION_BLUEPRINT_2026-05-27.md) | 优化总蓝图 | 2026-05-27 |
| [`SYNAPSE_UPSTREAM_RESEARCH_2026-05-27.md`](./synapse-rust/archive/SYNAPSE_UPSTREAM_RESEARCH_2026-05-27.md) | 上游 Synapse 行为研究 | 2026-05-27 |
| [`OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md`](./synapse-rust/archive/OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md) | 冗余消除与去重方案 | 2026-04-21 |
| [`SPEC_ALIGNMENT_PLAN_2026-05-01.md`](./synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md) | Matrix 规范对齐计划 | 2026-05-01 |

---

## 三、协议与接口（**现行**）

| 文档 | 用途 |
|------|------|
| [`SUPPORTED_MATRIX_SURFACE.md`](./synapse-rust/SUPPORTED_MATRIX_SURFACE.md) | 声明的 Matrix 客户端/服务端版本与能力面；提升规则 |
| [`API_COVERAGE_REPORT.md`](./synapse-rust/API_COVERAGE_REPORT.md) | API 覆盖率分析（vs Synapse v1.153.0） |
| [`API_SECURITY_VERIFICATION_REPORT.md`](./synapse-rust/API_SECURITY_VERIFICATION_REPORT.md) | API 安全验证报告 |
| [`admin-registration-guide.md`](./synapse-rust/admin-registration-guide.md) | 管理员注册流程 |
| [`LEDGER_EXPORT_SCHEMA.md`](./synapse-rust/LEDGER_EXPORT_SCHEMA.md) | Ledger 导出 schema |
| [`permission_matrix.csv`](./synapse-rust/permission_matrix.csv) | 权限矩阵 |

---

## 四、专题迁移与设计（**现行**）

| 文档 | 用途 | 状态 |
|------|------|------|
| [`E2EE_VODOZEMAC_MIGRATION.md`](./synapse-rust/archive/E2EE_VODOZEMAC_MIGRATION.md) | C-5 E2EE → vodozemac 收敛（C-5 Phase 1-4） | Phase 1+2 ✅ / Phase 3 🚧 |
| [`M3_BATCH1_EXECUTION_PLAN.md`](./synapse-rust/M3_BATCH1_EXECUTION_PLAN.md) | M-3 `sqlx::query!` 关键路径加固执行计划（A-F 阶段 5-8 天） | ⏳ 待启动 |
| [`M3_SQLX_MIGRATION_PLAN.md`](./synapse-rust/M3_SQLX_MIGRATION_PLAN.md) | M-3 `sqlx::query!` 迁移计划（**stale** — 已被搁置/重定向到 Batch 1 计划） | ⚠️ **stale** |
| [`M3_PROGRESS.md`](./synapse-rust/M3_PROGRESS.md) | M-3 进度报告（动态 SQL → 编译期宏迁移跟踪） | 🚧 进行中 |
| [`ROUTE_STORAGE_MIGRATION_PLAN.md`](./synapse-rust/ROUTE_STORAGE_MIGRATION_PLAN.md) | M-4 路由层直查 storage 迁移 | ✅ |
| [`REDUNDANCY_CLEANUP_LOG_2026-05-28.md`](./synapse-rust/archive/REDUNDANCY_CLEANUP_LOG_2026-05-28.md) | 冗余清理变更日志 | ✅ |
| [`REDUNDANT_TABLE_DELETION_PLAN.md`](./synapse-rust/archive/REDUNDANT_TABLE_DELETION_PLAN.md) | 冗余表删除计划 | ✅ |

### M-3 Issue Tracker

| 文档 | 用途 |
|------|------|
| [`issues/README.md`](./synapse-rust/issues/README.md) | M-3 非阻塞 issue 索引（孤儿模块 / schema drift / nullable 审计） |
| [`issues/M3-ISSUE-1-orphan-module-audit.md`](./synapse-rust/issues/M3-ISSUE-1-orphan-module-audit.md) | 全仓孤儿模块审计 |
| [`issues/M3-ISSUE-2-federation-blacklist-drift.md`](./synapse-rust/issues/M3-ISSUE-2-federation-blacklist-drift.md) | federation_blacklist schema-drift |
| [`issues/M3-ISSUE-3-e2ee-nullable-drift.md`](./synapse-rust/issues/M3-ISSUE-3-e2ee-nullable-drift.md) | E2EE 多表 nullable 性审计 |
| [`issues/M3-ISSUE-4-media-link-signer-drift.md`](./synapse-rust/issues/M3-ISSUE-4-media-link-signer-drift.md) | media_service 字段缺失 |

---

## 五、数据库（现行：`docs/db/`）

### 5.1 规范与治理

| 文档 | 用途 |
|------|------|
| [`DATABASE_FIELD_STANDARDS.md`](./db/DATABASE_FIELD_STANDARDS.md) | 字段命名规范（snake_case / `_ts` / `_at` / `is_` 前缀） |
| [`MIGRATION_GOVERNANCE.md`](./db/MIGRATION_GOVERNANCE.md) | 迁移文件治理规则 |
| [`MIGRATION_INDEX.md`](./db/MIGRATION_INDEX.md) | 迁移文件清单与职责 |
| [`SCHEMA_VALIDATION_GUIDE.md`](./db/SCHEMA_VALIDATION_GUIDE.md) | Schema 校验工具使用指南 |
| [`FIELD_MAPPING_REPORT.md`](./db/FIELD_MAPPING_REPORT.md) | 字段映射报告（_ts/_at/is_ 桥接） |

### 5.2 审计报告

| 文档 | 用途 | 末次更新 |
|------|------|----------|
| [`DB_AUDIT_AND_REMEDIATION_2026-05-29.md`](./db/DB_AUDIT_AND_REMEDIATION_2026-05-29.md) | 数据库审计与修复记录 | 2026-05-29 |
| [`FULL_SCHEMA_AUDIT_REPORT.md`](./db/FULL_SCHEMA_AUDIT_REPORT.md) | 全量 Schema 审计（SQL 表 vs Rust Struct 匹配） | 2026-05-29 |
| [`SCHEMA_CODE_AUDIT_REPORT_2026-04-22.md`](./db/SCHEMA_CODE_AUDIT_REPORT_2026-04-22.md) | Schema vs Rust 代码模型审计（已完结） | 2026-04-22 |
| [`MIGRATION_CONSOLIDATION_PLAN_2026-05-07.md`](./db/MIGRATION_CONSOLIDATION_PLAN_2026-05-07.md) | 数据库重构与迁移整合方案 | 2026-05-07 |
| [`DB_REMEDIATION_BACKLOG_2026-05-07.md`](./db/DB_REMEDIATION_BACKLOG_2026-05-07.md) | 数据库修复待办清单 | 2026-05-07 |

---

## 六、质量与可观测性（现行：`docs/quality/`）

| 文档 | 用途 |
|------|------|
| [`PRODUCTION_DEPLOYMENT_GUIDE.md`](./quality/PRODUCTION_DEPLOYMENT_GUIDE.md) | 生产部署指南 |
| [`PERMISSION_ANALYSIS.md`](./quality/PERMISSION_ANALYSIS.md) | 权限分析 |
| [`API_ENDPOINTS_STATUS.md`](./quality/API_ENDPOINTS_STATUS.md) | API 端点状态 |
| [`LOGGING_ENHANCEMENT.md`](./quality/LOGGING_ENHANCEMENT.md) | 日志增强 |
| [`FORMAT_DRIFT_TRACKING.md`](./quality/FORMAT_DRIFT_TRACKING.md) | 格式漂移追踪 |
| [`FORMAT_STANDARDIZATION_AUDIT_2026-05-29.md`](./quality/FORMAT_STANDARDIZATION_AUDIT_2026-05-29.md) | 格式标准化审计基线 |

---

## 七、SDK 文档（现行：`docs/sdk/`）

> 给 Matrix 客户端开发者看的对外 API 描述。

| 文档 | 用途 |
|------|------|
| [`README.md`](./sdk/README.md) | SDK 索引 |
| [`authentication.md`](./sdk/authentication.md) | 认证 |
| [`messages.md`](./sdk/messages.md) | 消息 |
| [`rooms.md`](./sdk/rooms.md) | 房间 |
| [`e2ee.md`](./sdk/e2ee.md) | 端到端加密 |
| [`media.md`](./sdk/media.md) | 媒体 |
| [`admin.md`](./sdk/admin.md) | 管理 API |
| [`errors.md`](./sdk/errors.md) | 错误码 |
| [`friends.md`](./sdk/friends.md) | 好友/私信扩展 |

---

## 八、CI 门禁与工作流

> 项目所有自动化检查都落在 `.github/workflows/` 与 `scripts/ci/` 下；本节是与文档对应的索引。

| 工作流 | 触发条件 | 用途 | 关联 |
|--------|----------|------|------|
| [`ci.yml`](../.github/workflows/ci.yml) | PR / push / 周一 02:00 UTC | 主 CI（含 `scripts/ci/supply_chain_gate.sh`） | Step 10 |
| [`e2ee-interop.yml`](../.github/workflows/e2ee-interop.yml) | PR (src/e2ee/**) / 周日 02:00 UTC / 手动 | C-5 Phase 3 vodozemac 互操作（含 `matrix-js-sdk` real-backend verification + 最小 Element Web 浏览器登录 smoke） | C-5 |
| [`mutation-testing.yml`](../.github/workflows/mutation-testing.yml) | 每日 03:00 UTC / 手动 | cargo-mutants（nightly，非阻塞） | Step 10 |
| [`schema-health-check.yml`](../.github/workflows/schema-health-check.yml) | PR (migrations/**) | Schema 漂移检测（M-3 替代门禁） | M-3 |
| [`db-migration-gate.yml`](../.github/workflows/db-migration-gate.yml) | PR (migrations/**) | 迁移文件门禁 | Step 7.5 |
| [`docs-quality-gate.yml`](../.github/workflows/docs-quality-gate.yml) | PR (docs/**) | 文档质量门禁 | Step 12 |
| [`format-governance.yml`](../.github/workflows/format-governance.yml) | PR | rustfmt 治理 | M-2 |
| [`format-drift-tracking.yml`](../.github/workflows/format-drift-tracking.yml) | 周一 | 格式漂移追踪 | M-2 |
| [`drift-detection.yml`](../.github/workflows/drift-detection.yml) | 每日 | 跨仓漂移检测 | Step 10 |
| [`benchmark.yml`](../.github/workflows/benchmark.yml) | PR label `bench` | 性能基准 | Step 9 |
| [`test.yml`](../.github/workflows/test.yml) | PR / push | 测试矩阵 | Step 8 |
| [`backend-validation.yml`](../.github/workflows/backend-validation.yml) | PR (backend/**) | 后端验证 | Step 6 |
| [`db-replica-consistency.yml`](../.github/workflows/db-replica-consistency.yml) | 每日 | 副本一致性 | M-6 |
| [`ledger-export.yml`](../.github/workflows/ledger-export.yml) | PR (ledger/**) | Ledger 导出 schema | Step 12 |

---

## 九、CI 脚本（`scripts/ci/`）

| 脚本 | 用途 |
|------|------|
| [`supply_chain_gate.sh`](../scripts/ci/supply_chain_gate.sh) | **Step 10 主门禁**：`cargo-deny check` + `cargo-audit --deny warnings` |
| [`check_route_storage_boundary.sh`](../scripts/ci/check_route_storage_boundary.sh) | M-4 配套：检测路由层直连 storage |
| [`check_route_layering.sh`](../scripts/quality/check_route_layering.sh) | C-4：路由分层门禁 |
| [`check_sqlx_dynamic_ratio.sh`](../scripts/ci/check_sqlx_dynamic_ratio.sh) | M-3：动态 SQL 占比门禁（已搁置期间保留观测） |
| [`check_sqlx_offline_cache.sh`](../scripts/ci/check_sqlx_offline_cache.sh) | M-3：`.sqlx/` 离线缓存检查 |
| `ci_schema_health_check.sh` (`scripts/`) | Schema 健康检查（表/列/索引漂移） |
| `run_cargo_audit.sh` (`scripts/`) | cargo-audit 单独运行入口 |

---

## 十、配置与基线文件（仓库根）

| 文件 | 用途 |
|------|------|
| [`deny.toml`](../deny.toml) | **Step 10**：`cargo-deny` 配置（advisories/bans/licenses/sources） |
| [`audit.toml`](../audit.toml) | `cargo-audit` 配置（canonical 文件名） |
| [`cargo-audit.toml`](../cargo-audit.toml) | 同上（保留作历史兼容） |
| [`.tarpaulin.toml`](../.tarpaulin.toml) | 覆盖率门槛（`range = 70..90`） |
| [`rustfmt.toml`](../rustfmt.toml) | rustfmt 配置 |
| [`.clippy.toml`](../.clippy.toml) | clippy 配置 |
| [`Cargo.toml`](../Cargo.toml) | 项目清单 |
| [`homeserver.yaml.example`](../homeserver.yaml.example) | 配置模板 |

---

## 十一、归档（`docs/archive/`）

> **只读**。所有内容已并入或被基线报告覆盖；保留供历史溯源。新工作请勿引用。

### 11.1 根级归档文件

早期草稿、完成报告、优化方案等，按时间线包括：

| 类别 | 示例文件 | 说明 |
|------|----------|------|
| 2026-03-30 草稿系列 | `2026-03-30-review.md` / `2026-03-30-test-report.md` / `2026-03-30-diff-report.md` | 初始审查草稿 |
| 2026-04-04 管理优化 | `ADMIN_OPTIMIZATION_SUMMARY_2026-04-04.md` / `ADMIN_VERIFICATION_MAPPING_2026-04-03.md` | 管理端优化记录 |
| 2026-04-15 API 合约系列 | `API_CONTRACT_*_2026-04-15.md`（9 个文件） | API 合约对齐工作记录 |
| 2026-04-15 优化收尾 | `ULTIMATE_FINAL_SUMMARY_2026-04-15.md` / `DAILY_SUMMARY_2026-04-15.md` / `OPTIMIZATION_*_2026-04-15.md` | 优化周期收尾报告 |
| 早期数据库审计 | `DB_REVIEW_REPORT.md` / `MIGRATION_OPERATIONS_GUIDE.md` / `MIGRATION_TOOLS_GUIDE.md` | 已被 `docs/db/` 现行文档覆盖 |
| 各模块优化方案 | `dm-optimization.md` / `media-optimization.md` / `e2ee-optimization.md` 等 | 已并入蓝图或已执行完毕 |
| 安全审计 | `SQL_INJECTION_AUDIT_REPORT.md` / `XSS_PROTECTION_ENHANCEMENT_PLAN.md` / `SECURITY_OPTIMIZATION_PLAN_2026-04-14.md` | 已并入安全基线 |
| 其他 | `DISASTER_RECOVERY_GUIDE.md` / `MONITORING_GUIDE.md` / `PERFORMANCE_OPTIMIZATION_GUIDE.md` | 已被现行文档覆盖或已过时 |

### 11.2 `archive/quality/` — 质量与缺陷归档

2026-04-26/27 权限修复与项目完成周期的工作记录，已被基线报告覆盖：

| 文件 | 说明 |
|------|------|
| `COMPLETE_SUCCESS_REPORT.md` | 完整成功报告 |
| `COMPLETE_FIX_RECORD.md` | 权限修复完整记录 |
| `COMPLETE_FIX_SOLUTION.md` | 完整修复方案 |
| `COMPLETE_FIX_SUMMARY.md` | 问题分析和修复总结 |
| `COMPLETE_OPTIMIZATION_PLAN.md` | 完整优化方案 |
| `COMPLETE_TEST_ANALYSIS.md` | 完整测试分析报告 |
| `FINAL_COMPLETION_REPORT.md` | 项目完成报告 |
| `FINAL_FIX_REPORT.md` | 权限修复最终报告 |
| `FINAL_FIX_SUMMARY.md` | 最终修复总结 |
| `FINAL_REPORT.md` | 项目优化完善最终报告 |
| `FINAL_SUMMARY.md` | 项目优化与部署最终总结 |
| `FINAL_TEST_REPORT.md` | 最终测试验证报告 |
| `FINAL_VERIFICATION_REPORT.md` | 权限修复验证报告 |
| `FINAL_VERIFICATION_REPORT_V2.md` | 权限修复最终验证报告 |
| `FIXES_SUMMARY.md` | 质量缺陷修复总结 |
| `OPTIMIZATION_SUMMARY.md` | 项目优化完善总结 |
| `PROJECT_COMPLETION_REPORT.md` | 项目优化完成报告 |
| `PROJECT_DEFECTS_ANALYSIS.md` | 项目缺陷分析报告 |
| `TEST_ANALYSIS_AND_FIX_PLAN.md` | 测试结果分析和修复计划 |
| `TEST_SKIP_ANALYSIS_AND_OPTIMIZATION.md` | 测试跳过案例分析和优化计划 |
| `defects_api_integration.md` | API 集成测试缺陷清单 |
| `defects_integration_test_analysis.md` | 集成测试缺陷分析 v1 |
| `defects_integration_test_analysis_v2.md` | 集成测试缺陷分析 v2 |

### 11.3 `archive/db/` — 数据库早期审计归档

| 文件 | 说明 |
|------|------|
| `DATABASE_AUDIT_REPORT.md` | 2026-03-26 数据库审查报告（已被 `FULL_SCHEMA_AUDIT_REPORT.md` 覆盖） |
| `DIAGNOSIS_REPORT.md` | 2026-03-28 数据库问题诊断方案（已被 `DB_AUDIT_AND_REMEDIATION_2026-05-29.md` 覆盖） |
| `_audit_extract.json` | 审计工具产物 |

### 11.4 `archive/api-option/` — API 优化方案归档

| 文件 | 说明 |
|------|------|
| `README.md` | API 优化文档总览（2026-03-27，已完成并归档） |

---

## 十二、文档命名规范

### 12.1 文件命名规则

| 规则 | 说明 | 示例 |
|------|------|------|
| 使用 UPPER_SNAKE_CASE | 审计报告、计划类文档 | `COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md` |
| 日期后缀格式 `_YYYY-MM-DD` | 带日期的报告必须使用此格式 | `DB_AUDIT_AND_REMEDIATION_2026-05-29.md` |
| 使用 kebab-case | 指南、教程类文档 | `admin-registration-guide.md` |
| 使用小写 | 数据清单、工具产物 | `permission_matrix.csv`、`_audit_extract.json` |
| 前缀下划线 | 工具自动生成的辅助文件 | `_audit_extract.json` |

### 12.2 目录组织规则

| 目录 | 用途 | 允许的文档类型 |
|------|------|----------------|
| `docs/synapse-rust/` | 项目审查、迁移计划、协议文档 | 审计报告、迁移计划、协议面声明 |
| `docs/synapse-rust/issues/` | M-3 非阻塞 issue 跟踪 | Issue 描述文档 |
| `docs/db/` | 数据库规范与审计 | 字段规范、迁移治理、Schema 审计 |
| `docs/quality/` | 质量与可观测性 | 部署指南、权限分析、格式追踪 |
| `docs/sdk/` | 对外 SDK 文档 | API 描述、错误码、快速开始 |
| `docs/archive/` | 历史归档（只读） | 所有已过时/已覆盖的文档 |
| `docs/archive/quality/` | 质量类历史归档 | 完成报告、修复记录、缺陷分析 |
| `docs/archive/db/` | 数据库类历史归档 | 早期审计报告、诊断方案 |
| `docs/archive/api-option/` | API 优化方案历史归档 | 已完成的优化方案 |

### 12.3 禁止事项

- 禁止在 `docs/` 顶层直接放报告文件（必须进子目录）
- 禁止在 `docs/synapse-rust/` 放 SDK/数据库/质量类文档
- 禁止在仓库根放散落 `.md`（除 `README.md` / `AGENTS.md` / `CLAUDE.md` / `TESTING.md` / `CHECKLIST.md`）
- 禁止修改 `docs/archive/` 中的任何文件

---

## 十三、文档维护指南

### 13.1 添加新文档流程

1. **确定文档类型**：审查报告 → `docs/synapse-rust/`；数据库 → `docs/db/`；质量 → `docs/quality/`；SDK → `docs/sdk/`
2. **按命名规范命名**：报告类加日期后缀 `_YYYY-MM-DD`；指南类用 kebab-case
3. **更新本索引**：在对应章节添加条目，填写用途和日期
4. **提交 PR**：修改 `docs/**` 的 PR 必须更新 `docs/INDEX.md`

### 13.2 文档降级为归档的条件

满足以下任一条件即应移入 `docs/archive/`：

1. **内容已被基线报告覆盖**：新版本审计报告已包含旧报告全部信息
2. **时间戳早于最近一次基线重审 60 天**：且内容未被其他现行文档引用
3. **代码路径已删除或重写**：文档描述的功能/模块已不存在
4. **完成报告/修复记录**：特定工作周期结束后，其过程性文档应归档

### 13.3 归档操作步骤

1. 将文件从现行目录移动到 `docs/archive/` 对应子目录
2. 在本索引的归档章节（十一）添加条目
3. 从现行章节移除该条目
4. 提交 PR 并说明归档原因

### 13.4 同步检查（PR 门禁）

- 修改 `src/**` 触发的 PR：必须更新 `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_*.md` 的状态行
- 修改 `migrations/**` 触发的 PR：必须更新 `docs/db/MIGRATION_INDEX.md`
- 修改 `docs/**` 触发的 PR：必须更新 `docs/INDEX.md`（本文档）

### 13.5 与 CI 的连接

- 文档质量门禁：`.github/workflows/docs-quality-gate.yml`
  - 强制：`README.md` / `INDEX.md` 存在
  - 检测：失效链接（`scripts/check_doc_spelling.sh`）
  - 漂移：跨仓漂移检测（`drift-detection.yml`）

---

## 十四、版本与同步

| 项 | 值 |
|----|----|
| Matrix Spec latest | v1.18 |
| Synapse 稳定标签 | v1.153.0 |
| Synapse 预发布 | v1.154.0rc1 |
| synapse-rust 当前版本 | 见 `Cargo.toml` |
| 最近基线审查 | 2026-06-12 |
| 文档索引版本 | 2026-06-12（测试覆盖率提升 + 编译错误修复） |

基线版本变更时，需同步更新：
1. `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_*.md`（最新基线报告）
2. `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`（协议面声明）
3. `docs/synapse-rust/API_COVERAGE_REPORT.md`（覆盖率）
4. `docs/INDEX.md`（本文档）
