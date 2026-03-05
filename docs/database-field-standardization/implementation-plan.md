# 数据库字段标准化实施计划

## 目标

- 建立可持续的字段审计与治理流程
- 收敛跨模块字段命名与类型差异
- 补齐关键约束并降低数据漂移风险

## 分阶段执行

## Phase 0 审计基线

- 导出 `columns/constraints/foreign_keys` 基线数据
- 生成 `audit-current-state.md`、`issues.csv`、`field-mapping.csv`
- 建立问题分级：P0（运行风险）、P1（一致性）、P2（规范性）

## Phase 1 兼容性修复

- 应用 `20260305000005_legacy_appservice_runtime_compat.sql`
- 应用 `20260305000006_standardize_appservice_fields_phase1.sql`
- 应用 `20260305000007_appservice_runtime_compat_guard.sql`
- 修复历史迁移兼容性：`20260302000003_add_media_quota_and_notification_tables.sql`
- 修复迁移工具对多版本 `schema_migrations` 的兼容

## Phase 2 标准化重构

- 按 `field-mapping.csv` 逐域统一 ID 字段类型
- 将布尔字段统一至 `is_/has_` 前缀
- 将 `*_at` 统一迁移到 `*_ts`
- 对可建立引用关系的字段补充外键

## Phase 3 收敛与清理

- 删除完成替代的冗余字段
- 更新全部 SQL 查询与模型字段
- 补充回归测试与性能基线对比

## 执行顺序约束

- 先做兼容迁移，再做约束收紧
- 先双写或回填，再切换代码读取路径
- 先验证核心业务，再扩大到全模块
