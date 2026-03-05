# 数据库字段标准化交付清单

- 现状分析报告：`audit-current-state.md`
- 问题明细：`issues.csv`
- 字段映射关系：`field-mapping.csv`
- 标准化规范：`standards-spec.md`
- 实施计划：`implementation-plan.md`
- 变更管理流程：`change-management.md`
- 回滚与风险评估：`rollback-risk.md`
- 验证报告：`verification-report.md`

## 审计与执行入口

- 审计脚本：`/home/tzd/synapse-rust/scripts/db_field_audit.py`
- 迁移脚本：`/home/tzd/synapse-rust/scripts/db_migrate.sh`
- 重构迁移：
  - `20260305000005_legacy_appservice_runtime_compat.sql`
  - `20260305000006_standardize_appservice_fields_phase1.sql`
  - `20260305000007_appservice_runtime_compat_guard.sql`
