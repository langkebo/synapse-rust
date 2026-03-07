# 数据库迁移文件索引

## 概述

本文档记录 synapse-rust 项目的数据库迁移文件，包括版本号、描述和执行顺序。

## 迁移文件列表

### 核心架构迁移

| 版本号 | 文件名 | 描述 | 状态 |
|--------|--------|------|------|
| v5 | `00000000_unified_schema_v5.sql` | 统一数据库架构文件 | ✅ 核心 |

### 功能扩展迁移

| 版本号 | 文件名 | 描述 | 依赖 |
|--------|--------|------|------|
| 20260302000002 | `add_retention_and_space_tables.sql` | 消息保留策略和 Space 表 | v5 |
| 20260302000003 | `add_media_quota_and_notification_tables.sql` | 媒体配额和服务器通知表 | v5 |
| 20260307000001 | `add_matrixrtc_tables.sql` | MatrixRTC 会话表 | v5 |
| 20260307000002 | `add_missing_feature_tables.sql` | 新功能支持表 | v5 |
| 20260307000003 | `add_beacon_tables.sql` | Beacon 位置分享表 | v5 |

### 字段修复迁移

| 版本号 | 文件名 | 描述 | 依赖 |
|--------|--------|------|------|
| 20260305000001 | `fix_events_and_account_data_dependencies.sql` | 事件和账户数据依赖修复 | v5 |
| 20260305000002 | `align_openid_tokens_schema.sql` | OpenID 令牌架构对齐 | v5 |
| 20260305000004 | `fix_appservice_schema_compat.sql` | 应用服务架构兼容 | v5 |
| 20260307000001 | `fix_field_names_to_match_standards.sql` | 字段命名标准化 | v5 |
| 20260308000001 | `fix_field_naming_inconsistencies.sql` | 字段命名一致性修复 | v5 |

### 质量保障迁移

| 版本号 | 文件名 | 描述 | 依赖 |
|--------|--------|------|------|
| 20260308000002 | `add_missing_foreign_key_constraints.sql` | 外键约束添加 | 20260308000001 |
| 20260308000003 | `optimize_database_indexes.sql` | 索引优化 | 20260308000002 |
| 20260308000004 | `data_isolation_triggers.sql` | 数据隔离触发器 | 20260308000002 |

### 兼容性迁移

| 版本号 | 文件名 | 描述 | 依赖 |
|--------|--------|------|------|
| 20260305000005 | `legacy_appservice_runtime_compat.sql` | 遗留应用服务兼容 | 20260305000004 |
| 20260305000006 | `standardize_appservice_fields_phase1.sql` | 应用服务字段标准化 | 20260305000005 |
| 20260305000007 | `appservice_runtime_compat_guard.sql` | 应用服务兼容守护 | 20260305000006 |

### 其他迁移

| 版本号 | 文件名 | 描述 | 依赖 |
|--------|--------|------|------|
| 20260301000001 | `add_notifications_ts_column.sql` | 通知时间戳列 | v5 |

## 执行顺序

```
1. 00000000_unified_schema_v5.sql (核心架构)
   ↓
2. 20260302*.sql (功能扩展)
   ↓
3. 20260305*.sql (字段修复)
   ↓
4. 20260307*.sql (新功能 + 字段修复)
   ↓
5. 20260308*.sql (质量保障)
```

## 命名规范

迁移文件命名格式：`YYYYMMDDHHMMSS_description.sql`

- **YYYY**: 年份
- **MM**: 月份
- **DD**: 日期
- **HHMMSS**: 时分秒
- **description**: 简短描述（snake_case）

## 版本控制

所有迁移记录存储在 `schema_migrations` 表中：

```sql
SELECT * FROM schema_migrations ORDER BY applied_ts DESC;
```

## 回滚策略

每个迁移文件应包含回滚逻辑（注释形式）：

```sql
-- ============================================================================
-- Rollback:
-- DROP TABLE IF EXISTS table_name;
-- ============================================================================
```

## 注意事项

1. 迁移文件一旦执行，不应修改
2. 新迁移应添加到列表末尾
3. 确保迁移文件幂等（可重复执行）
4. 测试环境验证后再应用到生产环境

---

**文档版本**: 1.0.0  
**更新日期**: 2026-03-08
