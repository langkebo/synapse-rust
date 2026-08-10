# synapse-rust 文档索引

> 最后更新: 2026-08-10
> 维护原则: 现行文档集中在 `docs/` 下各子目录；历史/一次性报告统一进入 `docs/archive/`，仅供溯源，不再作为契约引用。
> **契约真相**: 后端 HTTP 契约的机器权威是 `src/web/routes/route_ledger.rs` + 各模块 `*_route_manifest()`（启动时校验、集成测试 PATCH 探测）；人工可读的权威清单见 [`docs/synapse-rust/ROUTE_CONTRACT.md`](./synapse-rust/ROUTE_CONTRACT.md)。任何人工文档与代码冲突时以代码为准。

---

## 一、入口与基线（仓库根目录）

| 入口 | 用途 | 状态 |
|------|------|------|
| [`README.md`](../README.md) | 项目门面、构建/运行/测试命令 | 现行 |
| [`AGENTS.md`](../AGENTS.md) | 给 Codex/Claude 的项目工作流指引（含路由契约纪律） | 现行 |
| [`CLAUDE.md`](../CLAUDE.md) | Claude IDE 项目规则 | 现行 |
| [`TESTING.md`](../TESTING.md) | 测试分层与门禁定义 | 现行 |
| [`CHECKLIST.md`](../CHECKLIST.md) | 发布前自检表 | 现行 |
| [`CHANGELOG.md`](../CHANGELOG.md) | 版本变更日志 | 现行 |

---

## 二、契约与接口（**现行 · 最高优先**）

| 文档 | 用途 |
|------|------|
| [`ROUTE_CONTRACT.md`](./synapse-rust/ROUTE_CONTRACT.md) | **路由契约事实清单**：从 `src/web/routes/**` 真实注册面提取，逐模块列出 `(method, path)` 与 manifest 覆盖状态 |
| [`API_COVERAGE_REPORT.md`](./synapse-rust/API_COVERAGE_REPORT.md) | 相对 element-hq/synapse v1.153.0 的 API 覆盖率分析 |
| [`ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md`](./synapse-rust/ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md) | 与上游 Synapse 的能力差距分析（现行基线） |
| [`DEPENDENCY_UPGRADE_TRACKER.md`](./synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md) | 依赖升级追踪 |
| [`admin-registration-guide.md`](./synapse-rust/admin-registration-guide.md) | 管理员注册流程 |

> ⚠️ 历史上引用的 `SUPPORTED_MATRIX_SURFACE.md`、`API_SECURITY_VERIFICATION_REPORT.md`、`COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`、`M3_*.md`、`SPEC_ALIGNMENT_PLAN_2026-05-01.md`、`LEDGER_EXPORT_SCHEMA.md`、`ROUTE_STORAGE_MIGRATION_PLAN.md`、`MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md` 以及整个 `docs/db/` 目录**均已不存在**，已从本索引移除。如需要，相关内容可在 `docs/audit/`、`docs/archive/` 中检索。

---

## 三、路由契约纪律（来自 AGENTS.md）

- 每个特性/版本化路由必须在同一次变更中更新 `route_ledger` manifest（`*_route_manifest()` 或 `route_module::manifest_for`）。
- 启动时 `RouteLedger::validate` 会校验所有 `(method, path)` 不重复；集成测试 `tests/integration/api_route_ledger_tests.rs` 对每个声明做 PATCH 探测（断言 405）。
- `src/web/routes/handlers/versions.rs` 拥有 `/versions`、`.well-known`、`/capabilities`；改动版本/能力声明须用 typed builder 与 snapshot/contract 测试，禁止 ad hoc JSON 改写。

---

## 四、SDK 客户端契约（`docs/sdk/`）

| 文档 | 用途 |
|------|------|
| [`README.md`](./sdk/README.md) | SDK 契约总览 |
| [`authentication.md`](./sdk/authentication.md) | 认证端点契约 |
| [`rooms.md`](./sdk/rooms.md) | 房间端点契约 |
| [`messages.md`](./sdk/messages.md) | 消息端点契约 |
| [`media.md`](./sdk/media.md) | 媒体端点契约 |
| [`e2ee.md`](./sdk/e2ee.md) | 端到端加密契约 |
| [`friends.md`](./sdk/friends.md) | 好友（私有扩展）契约 |
| [`admin.md`](./sdk/admin.md) | 管理端点契约 |
| [`errors.md`](./sdk/errors.md) | 错误码契约 |

---

## 五、质量与安全（`docs/quality/`、`docs/security/`）

| 文档 | 用途 |
|------|------|
| [`API_ENDPOINTS_STATUS.md`](./quality/API_ENDPOINTS_STATUS.md) | API 端点状态 |
| [`FORMAT_STANDARDIZATION_AUDIT_2026-05-29.md`](./quality/FORMAT_STANDARDIZATION_AUDIT_2026-05-29.md) | 格式标准化审计 |
| [`FORMAT_DRIFT_TRACKING.md`](./quality/FORMAT_DRIFT_TRACKING.md) | 格式漂移追踪 |
| [`LOGGING_ENHANCEMENT.md`](./quality/LOGGING_ENHANCEMENT.md) | 日志增强 |
| [`PERMISSION_ANALYSIS.md`](./quality/PERMISSION_ANALYSIS.md) | 权限分析 |
| [`ci-security-grading.md`](./security/ci-security-grading.md) | CI 安全评级 |
| [`api-error.md`](./api-error.md) | API 错误记录（自动生成） |

---

## 六、历史审计与报告（`docs/audit/`）

按时间/主题组织的审计基线，关键入口：

- [`05_web_routes_review.md`](./audit/05_web_routes_review.md) — Web 路由层审查
- [`18_api_contract_review.md`](./audit/18_api_contract_review.md) — API 契约审查
- [`00_baseline_summary.md`](./audit/00_baseline_summary.md) — 基线总结
- [`07_security_audit.md`](./audit/07_security_audit.md) — 安全审计
- [`03_storage_review.md`](./audit/03_storage_review.md) — 存储层审查
- 其余 `audit/NN_*.md` 与 `superpowers/plans/` 为过程性报告，按需查阅。

---

## 七、归档（`docs/archive/`）

- `archive/api-option/` — API 选项讨论
- `archive/db/` — 数据库历史报告（原 `docs/db/` 已并入此处）
- `archive/quality/` — 质量修复历史报告
- `archive/redundant-summaries/` — 冗余总结归档

> 归档文档只读，不再维护，仅供历史溯源。
