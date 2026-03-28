# 迁移索引

## 核心迁移文件

| 文件 | 描述 | 状态 |
|------|------|------|
| `00000000_unified_schema_v6.sql` | 基础数据库 Schema | 必需 |
| `99999999_unified_incremental_migration.sql` | 综合迁移 (整合当前增量迁移) | 推荐 |

## 迁移策略

### 新环境部署
```bash
# 1. 执行基础 Schema
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql

# 2. 执行综合迁移
psql -U synapse -d synapse -f migrations/99999999_unified_incremental_migration.sql
```

### 现有环境升级
```bash
# 直接执行综合迁移 (幂等)
psql -U synapse -d synapse -f migrations/99999999_unified_incremental_migration.sql
```

## 回滚策略

### 回滚目录
回滚脚本位于 `rollback/` 目录，命名格式：`YYYYMMDDHHMMSS_description.rollback.sql`

### 回滚原则
1. **DROP TABLE 操作不可逆** - 回滚会删除表和数据
2. **列重命名可逆** - 使用条件判断安全回滚
3. **列添加通常不可逆** - PostgreSQL 不容易删除列

### 已提供回滚脚本的迁移

| 迁移文件 | 回滚脚本 | 可逆性 |
|----------|----------|--------|
| `20260330000001_add_thread_replies_and_receipts.sql` | `rollback/20260330000001_...rollback.sql` | 部分可逆 |
| `20260330000002_align_thread_schema_and_relations.sql` | `rollback/20260330000002_...rollback.sql` | 可逆 |
| `20260330000003_align_retention_and_room_summary_schema.sql` | `rollback/20260330000003_...rollback.sql` | 部分可逆 |
| `20260330000004_align_space_schema_and_add_space_events.sql` | `rollback/20260330000004_...rollback.sql` | 可逆 |
| `20260330000005_align_remaining_schema_exceptions.sql` | `rollback/20260330000005_...rollback.sql` | 部分可逆 |

### 执行回滚
```bash
# 按日期顺序回滚（逆序）
psql -U synapse -d synapse -f migrations/rollback/20260330000005_...rollback.sql
psql -U synapse -d synapse -f migrations/rollback/20260330000004_...rollback.sql
# ...
```

## 迁移整合说明

99999999_unified_incremental_migration.sql 已整合以下迁移:
- 20260320000001 - 密码字段重命名
- 20260320000002 - Olm 布尔字段重命名
- 20260320000004 - processed_ts 列
- 20260321000001 - 设备信任表
- 20260321000003 - 安全备份表
- 20260321000005 - 时间戳字段修复
- 20260321000006 - 字段一致性修复
- 20260321000007 - revoked_at 到 is_revoked

## 文档

- [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md) - 字段命名规范
- [SCHEMA_OPTIMIZATION_REPORT.md](./SCHEMA_OPTIMIZATION_REPORT.md) - Schema 优化报告
